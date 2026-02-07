use std::fs;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::config::AppPaths;

#[derive(Debug, Clone, Deserialize)]
pub struct ClashSubscription {
    pub proxies: Option<Vec<ProxyNode>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProxyNode {
    pub name: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub server: Option<String>,
    pub port: Option<u16>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub uuid: Option<String>,
    #[serde(rename = "alterId")]
    pub alter_id: Option<u32>,
    pub cipher: Option<String>,
    pub tls: Option<bool>,
    pub network: Option<String>,
    pub servername: Option<String>,
    pub sni: Option<String>,
    #[serde(rename = "ws-opts")]
    pub ws_opts: Option<WsOpts>,
    #[serde(rename = "grpc-opts")]
    pub grpc_opts: Option<GrpcOpts>,
    pub plugin: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WsOpts {
    pub path: Option<String>,
    pub headers: Option<std::collections::BTreeMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GrpcOpts {
    #[serde(rename = "grpc-service-name")]
    pub grpc_service_name: Option<String>,
}

impl ProxyNode {
    pub fn is_supported_for_sing_box(&self) -> bool {
        let base = self.server.is_some() && self.port.is_some();
        if !base {
            return false;
        }
        match self.node_type.as_str() {
            "socks5" | "socks" | "http" => true,
            "ss" => self.password.is_some() && self.cipher.is_some() && self.plugin.is_none(),
            "vmess" => self.uuid.is_some(),
            _ => false,
        }
    }
}

pub async fn download_subscription(url: &str, paths: &AppPaths) -> Result<String> {
    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("Failed to download subscription from {url}"))?;
    let status = response.status();
    if !status.is_success() {
        bail!("Subscription request failed with status {status}");
    }
    let body = response
        .text()
        .await
        .context("Failed to decode subscription response as text")?;
    paths.ensure_dirs()?;
    fs::write(&paths.subscription_yaml, &body).with_context(|| {
        format!(
            "Failed to write subscription cache {}",
            paths.subscription_yaml.display()
        )
    })?;
    Ok(body)
}

pub fn read_cached_subscription(paths: &AppPaths) -> Result<String> {
    if !paths.subscription_yaml.exists() {
        bail!(
            "Subscription cache not found at {}. Run `codex-route update` first.",
            paths.subscription_yaml.display()
        );
    }
    let text = fs::read_to_string(&paths.subscription_yaml)
        .with_context(|| format!("Failed to read {}", paths.subscription_yaml.display()))?;
    Ok(text)
}

pub fn parse_subscription(raw: &str) -> Result<Vec<ProxyNode>> {
    let parsed: ClashSubscription =
        serde_yaml::from_str(raw).context("Invalid Clash subscription YAML")?;
    let proxies = parsed.proxies.unwrap_or_default();
    if proxies.is_empty() {
        bail!("No proxies were found in subscription");
    }
    Ok(proxies)
}
