use std::fs;
use std::net::TcpStream;
use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value, json};
use tokio::process::{Child, Command};
use tokio::time::sleep;

use crate::config::{AppConfig, AppPaths};
use crate::subscription::ProxyNode;

fn node_to_outbound(node: &ProxyNode) -> Option<Value> {
    let server = node.server.as_ref()?;
    let port = node.port?;
    match node.node_type.as_str() {
        "socks5" | "socks" => Some(json!({
            "type": "socks",
            "tag": "proxy",
            "server": server,
            "server_port": port,
            "username": node.username,
            "password": node.password
        })),
        "http" => Some(json!({
            "type": "http",
            "tag": "proxy",
            "server": server,
            "server_port": port,
            "username": node.username,
            "password": node.password
        })),
        "ss" => {
            let method = node.cipher.as_ref()?;
            let password = node.password.as_ref()?;
            if node.plugin.is_some() {
                return None;
            }
            Some(json!({
                "type": "shadowsocks",
                "tag": "proxy",
                "server": server,
                "server_port": port,
                "method": method,
                "password": password
            }))
        }
        "vmess" => {
            let uuid = node.uuid.as_ref()?;
            let mut outbound = Map::<String, Value>::new();
            outbound.insert("type".to_string(), json!("vmess"));
            outbound.insert("tag".to_string(), json!("proxy"));
            outbound.insert("server".to_string(), json!(server));
            outbound.insert("server_port".to_string(), json!(port));
            outbound.insert("uuid".to_string(), json!(uuid));
            if let Some(alter_id) = node.alter_id {
                outbound.insert("alter_id".to_string(), json!(alter_id));
            }
            if let Some(cipher) = &node.cipher {
                outbound.insert("security".to_string(), json!(cipher));
            }

            if node.tls.unwrap_or(false) {
                let mut tls = Map::<String, Value>::new();
                tls.insert("enabled".to_string(), json!(true));
                if let Some(sni) = node.sni.as_deref().or(node.servername.as_deref()) {
                    tls.insert("server_name".to_string(), json!(sni));
                }
                outbound.insert("tls".to_string(), Value::Object(tls));
            }

            let network = node.network.as_deref().unwrap_or("tcp");
            match network {
                "ws" => {
                    let mut transport = Map::<String, Value>::new();
                    transport.insert("type".to_string(), json!("ws"));
                    if let Some(path) = node.ws_opts.as_ref().and_then(|w| w.path.as_ref()) {
                        transport.insert("path".to_string(), json!(path));
                    }
                    if let Some(headers) = node.ws_opts.as_ref().and_then(|w| w.headers.as_ref()) {
                        transport.insert("headers".to_string(), json!(headers));
                    }
                    outbound.insert("transport".to_string(), Value::Object(transport));
                }
                "grpc" => {
                    let mut transport = Map::<String, Value>::new();
                    transport.insert("type".to_string(), json!("grpc"));
                    if let Some(service_name) = node
                        .grpc_opts
                        .as_ref()
                        .and_then(|g| g.grpc_service_name.as_ref())
                    {
                        transport.insert("service_name".to_string(), json!(service_name));
                    }
                    outbound.insert("transport".to_string(), Value::Object(transport));
                }
                "tcp" => {}
                _ => {}
            }

            Some(Value::Object(outbound))
        }
        _ => None,
    }
}

pub fn generate_sing_box_config(cfg: &AppConfig, node: &ProxyNode, paths: &AppPaths) -> Result<()> {
    let outbound = node_to_outbound(node).with_context(|| {
        format!(
            "Selected node '{}' with type '{}' is unsupported by this MVP (supports socks5/socks/http/ss/vmess)",
            node.name, node.node_type
        )
    })?;

    let content = json!({
        "log": { "level": "warn" },
        "inbounds": [{
            "type": "mixed",
            "tag": "mixed-in",
            "listen": "127.0.0.1",
            "listen_port": cfg.proxy.mixed_port
        }],
        "outbounds": [
            outbound,
            { "type": "direct", "tag": "direct" }
        ],
        "route": {
            "rules": [{
                "domain_suffix": cfg.routing.proxy_domains,
                "outbound": "proxy"
            }],
            "final": "direct"
        }
    });

    paths.ensure_dirs()?;
    fs::write(
        &paths.sing_box_json,
        serde_json::to_string_pretty(&content).context("Failed to serialize sing-box config")?,
    )
    .with_context(|| format!("Failed to write {}", paths.sing_box_json.display()))?;
    Ok(())
}

pub async fn spawn_proxy_core(core_path: &str, config_path: &str) -> Result<Child> {
    let child = Command::new(core_path)
        .arg("run")
        .arg("-c")
        .arg(config_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("Failed to start proxy core: {core_path}"))?;
    Ok(child)
}

pub async fn wait_port_open(port: u16, timeout: Duration) -> Result<()> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return Ok(());
        }
        sleep(Duration::from_millis(200)).await;
    }
    bail!(
        "Proxy port 127.0.0.1:{port} is not reachable within {:?}",
        timeout
    );
}

pub async fn stop_process(child: &mut Child) -> Result<()> {
    if child.id().is_none() {
        return Ok(());
    }
    let _ = child.kill().await;
    let _ = child.wait().await;
    Ok(())
}
