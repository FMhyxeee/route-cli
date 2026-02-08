use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use tokio::process::Command;
use zip::ZipArchive;

use crate::config::{AppPaths, load_config, resolve_proxy_core_path, save_config};
use crate::proxy::{generate_sing_box_config, spawn_proxy_core, stop_process, wait_port_open};
use crate::subscription::{
    ProxyNode, download_subscription, parse_subscription, read_cached_subscription,
};

const GITHUB_LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/SagerNet/sing-box/releases/latest";
const TARGET_ASSET_SUFFIX: &str = "windows-amd64.zip";

fn node_region_priority(name: &str) -> u8 {
    let lower = name.to_lowercase();
    if name.contains("新加坡") || lower.contains("singapore") || lower.contains("sg") {
        return 0;
    }
    if name.contains("韩国") || lower.contains("korea") || lower.contains("kr") {
        return 1;
    }
    if name.contains("美国")
        || lower.contains("united states")
        || lower.contains("usa")
        || lower.contains("us")
    {
        return 2;
    }
    3
}

async fn ping_reachable(host: &str) -> bool {
    #[cfg(windows)]
    let mut cmd = {
        let mut c = Command::new("ping");
        c.arg("-n").arg("1").arg("-w").arg("1500").arg(host);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = Command::new("ping");
        c.arg("-c").arg("1").arg("-W").arg("2").arg(host);
        c
    };
    match cmd
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
    {
        Ok(s) => s.success(),
        Err(_) => false,
    }
}

#[cfg(windows)]
fn resolve_program_for_windows(program: &str) -> String {
    use std::env;
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};

    let p = Path::new(program);
    if p.extension().is_some() || p.components().count() > 1 {
        return program.to_string();
    }

    let pathext = env::var_os("PATHEXT")
        .map(|v| {
            v.to_string_lossy()
                .split(';')
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| {
            vec![
                ".COM".to_string(),
                ".EXE".to_string(),
                ".BAT".to_string(),
                ".CMD".to_string(),
            ]
        });

    let path_dirs = env::var_os("PATH")
        .map(|v| env::split_paths(&v).collect::<Vec<PathBuf>>())
        .unwrap_or_default();

    for dir in path_dirs {
        for ext in &pathext {
            let ext = if ext.starts_with('.') {
                ext.clone()
            } else {
                format!(".{ext}")
            };
            let mut file = OsString::from(program);
            file.push(ext);
            let candidate = dir.join(file);
            if candidate.exists() {
                return candidate.to_string_lossy().into_owned();
            }
        }
    }

    program.to_string()
}

#[cfg(not(windows))]
fn resolve_program_for_windows(program: &str) -> String {
    program.to_string()
}

fn pick_windows_amd64_asset(assets: &[serde_json::Value]) -> Option<(String, String)> {
    for asset in assets {
        let name = asset.get("name").and_then(|v| v.as_str())?;
        let url = asset.get("browser_download_url").and_then(|v| v.as_str())?;
        if name.ends_with(TARGET_ASSET_SUFFIX) && !url.is_empty() {
            return Some((name.to_string(), url.to_string()));
        }
    }
    None
}

async fn resolve_latest_sing_box_asset() -> Result<(String, String)> {
    let client = reqwest::Client::new();
    let response = client
        .get(GITHUB_LATEST_RELEASE_API)
        .header(reqwest::header::USER_AGENT, "route-cli")
        .send()
        .await
        .context("Failed to query latest sing-box release")?;
    let status = response.status();
    if !status.is_success() {
        bail!("Latest release request failed with status {status}");
    }
    let payload_text = response
        .text()
        .await
        .context("Failed to read latest release response body")?;
    let payload: serde_json::Value = serde_json::from_str(&payload_text)
        .context("Failed to parse latest release response JSON")?;
    let assets = payload
        .get("assets")
        .and_then(|v| v.as_array())
        .context("No assets found in latest release response")?;
    pick_windows_amd64_asset(assets).context(
        "No windows-amd64 release asset found. Use `route-cli install-core --url <zip-url>`",
    )
}

fn install_sing_box_zip(bytes: &[u8], target_dir: &Path) -> Result<PathBuf> {
    fs::create_dir_all(target_dir)
        .with_context(|| format!("Failed to create {}", target_dir.display()))?;
    let mut archive =
        ZipArchive::new(Cursor::new(bytes)).context("Invalid sing-box zip archive")?;

    let mut installed_exe: Option<PathBuf> = None;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .with_context(|| format!("Failed reading zip entry #{i}"))?;
        if !entry.is_file() {
            continue;
        }
        let Some(name) = entry.enclosed_name() else {
            continue;
        };
        let Some(file_name) = name.file_name().and_then(|s| s.to_str()) else {
            continue;
        };

        let lower = file_name.to_ascii_lowercase();
        let destination = if lower == "sing-box.exe" {
            target_dir.join("sing-box.exe")
        } else if lower.ends_with(".dll") {
            target_dir.join(file_name)
        } else {
            continue;
        };

        let mut buf = Vec::new();
        entry
            .read_to_end(&mut buf)
            .with_context(|| format!("Failed to extract zip entry: {}", name.display()))?;
        fs::write(&destination, buf)
            .with_context(|| format!("Failed to write {}", destination.display()))?;
        if lower == "sing-box.exe" {
            installed_exe = Some(destination);
        }
    }

    installed_exe.context("sing-box.exe not found in downloaded zip archive")
}

fn local_bundle_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let mut push_unique = |p: PathBuf| {
        if !candidates.iter().any(|x| x == &p) {
            candidates.push(p);
        }
    };

    push_unique(PathBuf::from("tools").join("sing-box"));
    if let Ok(cwd) = std::env::current_dir() {
        push_unique(cwd.join("tools").join("sing-box"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            push_unique(exe_dir.join("tools").join("sing-box"));
        }
    }
    candidates
}

fn install_sing_box_from_local_bundle(target_dir: &Path) -> Result<Option<(PathBuf, PathBuf)>> {
    for source_dir in local_bundle_candidates() {
        let source_exe = source_dir.join("sing-box.exe");
        if !source_exe.exists() {
            continue;
        }

        fs::create_dir_all(target_dir)
            .with_context(|| format!("Failed to create {}", target_dir.display()))?;

        let target_exe = target_dir.join("sing-box.exe");
        fs::copy(&source_exe, &target_exe).with_context(|| {
            format!(
                "Failed to copy {} to {}",
                source_exe.display(),
                target_exe.display()
            )
        })?;

        for entry in fs::read_dir(&source_dir)
            .with_context(|| format!("Failed to read {}", source_dir.display()))?
        {
            let entry = entry.with_context(|| {
                format!("Failed to read directory entry in {}", source_dir.display())
            })?;
            let file_type = entry
                .file_type()
                .with_context(|| format!("Failed to get type for {}", entry.path().display()))?;
            if !file_type.is_file() {
                continue;
            }
            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            if !ext.eq_ignore_ascii_case("dll") {
                continue;
            }
            let file_name = entry.file_name();
            let target_path = target_dir.join(&file_name);
            fs::copy(&path, &target_path).with_context(|| {
                format!(
                    "Failed to copy {} to {}",
                    path.display(),
                    target_path.display()
                )
            })?;
        }

        return Ok(Some((source_dir, target_exe)));
    }

    Ok(None)
}

async fn download_and_install_sing_box(
    asset_name: &str,
    download_url: &str,
    target_dir: &Path,
) -> Result<PathBuf> {
    println!("Installing sing-box asset: {asset_name}");
    println!("Download URL: {download_url}");

    let client = reqwest::Client::new();
    let response = client
        .get(download_url)
        .header(reqwest::header::USER_AGENT, "route-cli")
        .send()
        .await
        .with_context(|| format!("Failed to download sing-box from {download_url}"))?;
    let status = response.status();
    if !status.is_success() {
        bail!("Download request failed with status {status}");
    }
    let archive_bytes = response
        .bytes()
        .await
        .context("Failed to read sing-box archive response bytes")?;
    install_sing_box_zip(archive_bytes.as_ref(), target_dir)
}

pub async fn cmd_login_sub(url: String) -> Result<()> {
    let paths = AppPaths::discover()?;
    let mut cfg = load_config(&paths)?;
    cfg.subscription.url = Some(url);
    save_config(&paths, &cfg)?;
    println!("Subscription URL saved to {}", paths.config_toml.display());
    Ok(())
}

pub async fn cmd_install_core(url: Option<String>) -> Result<()> {
    let paths = AppPaths::discover()?;
    let mut cfg = load_config(&paths)?;
    let install_dir = paths
        .config_toml
        .parent()
        .context("Invalid config path: missing parent directory")?
        .join("bin");

    let installed_exe = match url {
        Some(u) => download_and_install_sing_box("custom.zip", &u, &install_dir).await?,
        None => {
            if let Some((source_dir, exe)) = install_sing_box_from_local_bundle(&install_dir)? {
                println!(
                    "Installed sing-box from local bundle: {}",
                    source_dir.display()
                );
                exe
            } else {
                let (asset_name, download_url) = resolve_latest_sing_box_asset().await?;
                download_and_install_sing_box(&asset_name, &download_url, &install_dir).await?
            }
        }
    };
    cfg.proxy_core.path = installed_exe.to_string_lossy().into_owned();
    save_config(&paths, &cfg)?;

    let core_check = Command::new(&cfg.proxy_core.path)
        .arg("version")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await;

    match core_check {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if version.is_empty() {
                println!("[OK] sing-box installed: {}", cfg.proxy_core.path);
            } else {
                println!(
                    "[OK] sing-box installed: {} ({version})",
                    cfg.proxy_core.path
                );
            }
        }
        Ok(_) => {
            println!(
                "[WARN] sing-box installed at {} but `version` returned non-zero",
                cfg.proxy_core.path
            );
        }
        Err(err) => {
            println!(
                "[WARN] sing-box installed at {} but failed to run `version`: {err}",
                cfg.proxy_core.path
            );
        }
    }

    println!("Updated config: {}", paths.config_toml.display());
    Ok(())
}

pub async fn cmd_update() -> Result<()> {
    let paths = AppPaths::discover()?;
    let cfg = load_config(&paths)?;
    let url = cfg
        .subscription
        .url
        .as_deref()
        .context("No subscription URL configured. Run `route-cli login-sub --url <URL>`")?;
    let raw = download_subscription(url, &paths).await?;
    let nodes = parse_subscription(&raw)?;
    println!(
        "Subscription updated: {} nodes cached at {}",
        nodes.len(),
        paths.subscription_yaml.display()
    );
    Ok(())
}

pub async fn cmd_list_nodes() -> Result<()> {
    let paths = AppPaths::discover()?;
    let raw = read_cached_subscription(&paths)?;
    let nodes = parse_subscription(&raw)?;
    for (idx, node) in nodes.iter().enumerate() {
        let support = if node.is_supported_for_sing_box() {
            "supported"
        } else {
            "unsupported"
        };
        println!(
            "{:03} | {} | {} | {}",
            idx + 1,
            node.name,
            node.node_type,
            support
        );
    }
    Ok(())
}

pub async fn cmd_use_node(node_name: String) -> Result<()> {
    let paths = AppPaths::discover()?;
    let mut cfg = load_config(&paths)?;
    let raw = read_cached_subscription(&paths)?;
    let nodes = parse_subscription(&raw)?;
    let node = nodes
        .iter()
        .find(|n| n.name == node_name)
        .with_context(|| format!("Node '{node_name}' not found in cached subscription"))?;
    if !node.is_supported_for_sing_box() {
        bail!(
            "Node '{}' type '{}' is not supported yet (supports socks5/socks/http/ss/vmess)",
            node.name,
            node.node_type
        );
    }
    cfg.runtime.selected_node = Some(node_name.clone());
    save_config(&paths, &cfg)?;
    println!("Selected node: {node_name}");
    Ok(())
}

pub async fn cmd_run(command: Vec<String>) -> Result<i32> {
    if command.is_empty() {
        bail!("No command passed. Example: route-cli run -- claude");
    }

    let paths = AppPaths::discover()?;
    let mut cfg = load_config(&paths)?;
    if cfg.subscription.url.is_none() {
        bail!("No subscription URL configured. Run `route-cli login-sub --url <URL>`");
    }
    if !paths.subscription_yaml.exists() {
        cmd_update().await?;
    }

    let raw = read_cached_subscription(&paths)?;
    let nodes = parse_subscription(&raw)?;
    let supported: Vec<&ProxyNode> = nodes
        .iter()
        .filter(|n| n.is_supported_for_sing_box())
        .collect();
    if supported.is_empty() {
        bail!("No supported node found. Use `route-cli list-nodes` then update subscription.");
    }

    let mut candidates: Vec<&ProxyNode> = Vec::new();
    if let Some(preferred) = cfg.runtime.selected_node.as_deref() {
        if let Some(node) = supported.iter().copied().find(|n| n.name == preferred) {
            candidates.push(node);
        }
    }
    let mut remaining: Vec<&ProxyNode> = supported
        .iter()
        .copied()
        .filter(|n| {
            !candidates
                .iter()
                .any(|selected| selected.name.as_str() == n.name.as_str())
        })
        .collect();
    remaining.sort_by_key(|n| node_region_priority(&n.name));
    candidates.extend(remaining);

    let mut selected: Option<&ProxyNode> = None;
    for node in candidates {
        let host = node.server.as_deref().unwrap_or("");
        let ok = ping_reachable(host).await;
        println!(
            "[{}] ping {} ({})",
            if ok { "OK" } else { "FAIL" },
            node.name,
            host
        );
        if ok {
            selected = Some(node);
            break;
        }
    }
    let selected = selected.context(
        "No reachable node after ping checks. Check network/subscription or switch nodes.",
    )?;

    if cfg.runtime.selected_node.as_deref() != Some(selected.name.as_str()) {
        cfg.runtime.selected_node = Some(selected.name.clone());
        save_config(&paths, &cfg)?;
    }

    let core_path = resolve_proxy_core_path(&cfg.proxy_core.path);

    generate_sing_box_config(&cfg, selected, &paths)?;
    let mut core = spawn_proxy_core(&core_path, &paths.sing_box_json.to_string_lossy())
        .await
        .context("Failed to launch proxy core")?;

    wait_port_open(cfg.proxy.mixed_port, Duration::from_secs(8))
        .await
        .context("Proxy core started but local mixed port is unavailable")?;

    let program = resolve_program_for_windows(&command[0]);
    let args = &command[1..];

    let http_proxy = format!("http://127.0.0.1:{}", cfg.proxy.mixed_port);
    let all_proxy = format!("socks5://127.0.0.1:{}", cfg.proxy.mixed_port);
    let no_proxy = cfg.routing.no_proxy.join(",");

    let mut child = Command::new(&program)
        .args(args)
        .env("HTTP_PROXY", &http_proxy)
        .env("HTTPS_PROXY", &http_proxy)
        .env("ALL_PROXY", &all_proxy)
        .env("NO_PROXY", &no_proxy)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("Failed to start command `{}`", command[0]))?;

    let status = child.wait().await.context("Failed waiting child command")?;
    stop_process(&mut core).await?;

    Ok(status.code().unwrap_or(1))
}

pub async fn cmd_doctor() -> Result<()> {
    let paths = AppPaths::discover()?;
    let cfg = load_config(&paths)?;
    let core_path = resolve_proxy_core_path(&cfg.proxy_core.path);

    println!("[OK] config path: {}", paths.config_toml.display());
    println!(
        "[{}] subscription URL configured",
        if cfg.subscription.url.is_some() {
            "OK"
        } else {
            "ERR"
        }
    );
    println!("[OK] proxy core configured path: {}", cfg.proxy_core.path);
    println!("[OK] proxy core resolved path: {core_path}");

    let core_check = Command::new(&core_path)
        .arg("version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;
    match core_check {
        Ok(status) if status.success() => println!("[OK] proxy core available"),
        Ok(_) => println!("[WARN] proxy core exists but `version` returned non-zero"),
        Err(err) => println!("[ERR] proxy core unavailable: {core_path} ({err})"),
    }

    if paths.subscription_yaml.exists() {
        let raw = read_cached_subscription(&paths)?;
        let nodes = parse_subscription(&raw)?;
        let supported = nodes
            .iter()
            .filter(|n| n.is_supported_for_sing_box())
            .count();
        println!(
            "[OK] cached nodes: {} total, {} supported",
            nodes.len(),
            supported
        );
        if supported == 0 {
            println!("[WARN] no supported nodes (requires socks5/socks/http/ss/vmess)");
        }
    } else {
        println!(
            "[WARN] no cached subscription: {}",
            paths.subscription_yaml.display()
        );
    }

    println!(
        "[OK] generated config location: {}",
        paths.sing_box_json.display()
    );
    println!("[OK] mixed proxy port: {}", cfg.proxy.mixed_port);
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::pick_windows_amd64_asset;

    #[test]
    fn picks_windows_amd64_asset() {
        let assets = vec![
            json!({
                "name": "sing-box-1.12.20-linux-amd64.tar.gz",
                "browser_download_url": "https://example.com/linux.tar.gz"
            }),
            json!({
                "name": "sing-box-1.12.20-windows-amd64.zip",
                "browser_download_url": "https://example.com/windows.zip"
            }),
        ];
        let selected = pick_windows_amd64_asset(&assets).expect("should pick windows asset");
        assert_eq!(selected.0, "sing-box-1.12.20-windows-amd64.zip");
        assert_eq!(selected.1, "https://example.com/windows.zip");
    }
}
