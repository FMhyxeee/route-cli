use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const APP_DIR: &str = "route";
const LEGACY_APP_DIR: &str = "codex-route";
const DEFAULT_PROXY_DOMAINS: [&str; 6] = [
    "openai.com",
    "api.openai.com",
    "chatgpt.com",
    "oaistatic.com",
    "oaiusercontent.com",
    "openaiapi-site.azureedge.net",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub subscription: SubscriptionConfig,
    pub proxy_core: ProxyCoreConfig,
    pub proxy: LocalProxyConfig,
    pub routing: RoutingConfig,
    pub runtime: RuntimeConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionConfig {
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyCoreConfig {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalProxyConfig {
    pub mixed_port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingConfig {
    pub proxy_domains: Vec<String>,
    pub no_proxy: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub selected_node: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            subscription: SubscriptionConfig { url: None },
            proxy_core: ProxyCoreConfig {
                path: "sing-box.exe".to_string(),
            },
            proxy: LocalProxyConfig { mixed_port: 27890 },
            routing: RoutingConfig {
                proxy_domains: DEFAULT_PROXY_DOMAINS
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                no_proxy: vec!["localhost".to_string(), "127.0.0.1".to_string()],
            },
            runtime: RuntimeConfig {
                selected_node: None,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub config_toml: PathBuf,
    pub subscription_yaml: PathBuf,
    pub generated_dir: PathBuf,
    pub sing_box_json: PathBuf,
}

impl AppPaths {
    pub fn discover() -> Result<Self> {
        let appdata = dirs::config_dir().context("Unable to locate config directory")?;
        let root = appdata.join(APP_DIR);
        migrate_legacy_root(&appdata, &root)?;
        let config_toml = root.join("config.toml");
        let subscription_yaml = root.join("cache").join("subscription.yaml");
        let generated_dir = root.join("generated");
        let sing_box_json = generated_dir.join("sing-box.json");
        Ok(Self {
            config_toml,
            subscription_yaml,
            generated_dir,
            sing_box_json,
        })
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        ensure_parent(&self.config_toml)?;
        ensure_parent(&self.subscription_yaml)?;
        fs::create_dir_all(&self.generated_dir)
            .with_context(|| format!("Failed to create {}", self.generated_dir.display()))?;
        Ok(())
    }
}

fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }
    Ok(())
}

fn migrate_legacy_root(appdata: &Path, new_root: &Path) -> Result<()> {
    if new_root.exists() {
        return Ok(());
    }

    let legacy_root = appdata.join(LEGACY_APP_DIR);
    if !legacy_root.exists() {
        return Ok(());
    }

    copy_dir_recursive(&legacy_root, new_root).with_context(|| {
        format!(
            "Failed to migrate legacy config from {} to {}",
            legacy_root.display(),
            new_root.display()
        )
    })?;
    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst).with_context(|| format!("Failed to create {}", dst.display()))?;
    for entry in
        fs::read_dir(src).with_context(|| format!("Failed to read directory {}", src.display()))?
    {
        let entry = entry
            .with_context(|| format!("Failed to read directory entry in {}", src.display()))?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        let file_type = entry
            .file_type()
            .with_context(|| format!("Failed to get type for {}", src_path.display()))?;
        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else if file_type.is_file() {
            fs::copy(&src_path, &dst_path).with_context(|| {
                format!(
                    "Failed to copy {} to {}",
                    src_path.display(),
                    dst_path.display()
                )
            })?;
        }
    }
    Ok(())
}

pub fn load_config(paths: &AppPaths) -> Result<AppConfig> {
    if !paths.config_toml.exists() {
        let cfg = AppConfig::default();
        save_config(paths, &cfg)?;
        return Ok(cfg);
    }
    let raw = fs::read_to_string(&paths.config_toml)
        .with_context(|| format!("Failed to read {}", paths.config_toml.display()))?;
    let cfg = toml::from_str::<AppConfig>(&raw).context("Invalid config.toml format")?;
    Ok(cfg)
}

pub fn save_config(paths: &AppPaths, cfg: &AppConfig) -> Result<()> {
    paths.ensure_dirs()?;
    let content = toml::to_string_pretty(cfg).context("Failed to serialize config")?;
    fs::write(&paths.config_toml, content)
        .with_context(|| format!("Failed to write {}", paths.config_toml.display()))?;
    Ok(())
}

pub fn resolve_proxy_core_path(configured_path: &str) -> String {
    let bundled_rel = PathBuf::from("tools/sing-box/sing-box.exe");
    if configured_path.eq_ignore_ascii_case("sing-box.exe") {
        if bundled_rel.exists() {
            return bundled_rel.to_string_lossy().into_owned();
        }
        if let Ok(cwd) = std::env::current_dir() {
            let candidate = cwd.join(&bundled_rel);
            if candidate.exists() {
                return candidate.to_string_lossy().into_owned();
            }
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(exe_dir) = exe.parent() {
                let candidate = exe_dir.join(&bundled_rel);
                if candidate.exists() {
                    return candidate.to_string_lossy().into_owned();
                }
            }
        }
    }

    let configured = PathBuf::from(configured_path);
    if configured.is_absolute() && configured.exists() {
        return configured_path.to_string();
    }

    if configured.exists() {
        return configured.to_string_lossy().into_owned();
    }

    if let Ok(cwd) = std::env::current_dir() {
        let candidate = cwd.join(&configured);
        if candidate.exists() {
            return candidate.to_string_lossy().into_owned();
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            let candidate = exe_dir.join(&configured);
            if candidate.exists() {
                return candidate.to_string_lossy().into_owned();
            }
        }
    }

    if configured_path.eq_ignore_ascii_case("tools/sing-box/sing-box.exe") {
        return "sing-box.exe".to_string();
    }

    configured_path.to_string()
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{copy_dir_recursive, migrate_legacy_root};

    fn make_temp_dir(prefix: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be valid")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{prefix}-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&path).expect("temp dir should be creatable");
        path
    }

    #[test]
    fn migrates_legacy_config_when_new_root_missing() {
        let appdata = make_temp_dir("route-cli-migrate");
        let legacy = appdata.join("codex-route");
        let new_root = appdata.join("route");
        std::fs::create_dir_all(legacy.join("cache")).expect("legacy cache dir should be created");
        std::fs::write(legacy.join("config.toml"), "x = 1")
            .expect("legacy config should be written");
        std::fs::write(
            legacy.join("cache").join("subscription.yaml"),
            "proxies: []",
        )
        .expect("legacy cache should be written");

        migrate_legacy_root(&appdata, &new_root).expect("migration should succeed");

        assert!(new_root.join("config.toml").exists());
        assert!(new_root.join("cache").join("subscription.yaml").exists());
    }

    #[test]
    fn does_not_override_when_new_root_exists() {
        let appdata = make_temp_dir("route-cli-preserve");
        let legacy = appdata.join("codex-route");
        let new_root = appdata.join("route");
        std::fs::create_dir_all(&legacy).expect("legacy dir should be created");
        std::fs::create_dir_all(&new_root).expect("new root should be created");
        std::fs::write(legacy.join("config.toml"), "legacy = true")
            .expect("legacy config should be written");
        std::fs::write(new_root.join("config.toml"), "new = true")
            .expect("new config should be written");

        migrate_legacy_root(&appdata, &new_root).expect("migration should not fail");

        let new_cfg = std::fs::read_to_string(new_root.join("config.toml"))
            .expect("new config should remain readable");
        assert_eq!(new_cfg, "new = true");
    }

    #[test]
    fn copies_nested_directories() {
        let src = make_temp_dir("route-cli-copy-src");
        let dst = make_temp_dir("route-cli-copy-dst");
        let src_root = src.join("root");
        let dst_root = dst.join("root");
        std::fs::create_dir_all(src_root.join("nested"))
            .expect("source nested dir should be created");
        std::fs::write(src_root.join("nested").join("a.txt"), "ok")
            .expect("source file should be written");

        copy_dir_recursive(&src_root, &dst_root).expect("recursive copy should succeed");

        assert_eq!(
            std::fs::read_to_string(dst_root.join("nested").join("a.txt"))
                .expect("copied file should be readable"),
            "ok"
        );
    }
}
