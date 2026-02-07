use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

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
                path: "tools/sing-box/sing-box.exe".to_string(),
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
        let root = appdata.join("codex-route");
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
