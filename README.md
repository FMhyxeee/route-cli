# route-cli

`route-cli` is a Windows-first Rust CLI for running any target CLI command through a process-scoped proxy.
It does not change system-wide proxy settings or routes.

## Why this tool

In enterprise networks, full-tunnel VPN often breaks internal resources.
`route-cli` applies proxy env vars only to the target process tree (for example `codex`, `claude`, or other CLIs).

## What it does

- Downloads and parses Clash subscription YAML
- Caches nodes locally
- Lists node support status
- Selects nodes automatically or manually
- Generates `sing-box` runtime config
- Starts local `sing-box` proxy core
- Launches target command with scoped env vars: `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`, `NO_PROXY`

## Node selection behavior

When you run `route-cli run -- <command>`:

1. If `runtime.selected_node` exists, try it first.
2. Remaining nodes are sorted by region priority: Singapore, Korea, United States, Others.
3. Each candidate is checked by `ping`.
4. The first reachable node is selected and persisted.

## Supported Clash node types

- `socks5`
- `socks`
- `http`
- `ss`
- `vmess`

Current limitations:

- `ss` with plugin fields is not supported yet.
- `vmess` supports common `tcp/ws/grpc`; advanced variants may need extra mapping.

## Project layout

```text
route-cli/
  Cargo.toml
  src/
  tools/
    sing-box/
      sing-box.exe
```

Recommended distribution bundle:

- `route-cli.exe`
- `tools/sing-box/sing-box.exe`
- `tools/sing-box/*.dll` (if required by your build)

## Requirements

- Windows
- Rust toolchain (if building from source)
- target CLI installed and available in terminal
- `sing-box` binary (installed via `route-cli install-core`, bundled, or on PATH)

## Build

```powershell
cargo build
```

## Quick start

1. Install proxy core:

```powershell
cargo run -- install-core
```

`install-core` first tries local `tools/sing-box`; if not found, it downloads latest Windows AMD64 release from GitHub.

2. Save subscription URL:

```powershell
cargo run -- login-sub --url "https://example.com/subscription.yaml"
```

3. Pull and cache subscription:

```powershell
cargo run -- update
```

4. Optional: pick a node manually:

```powershell
cargo run -- use-node "your-node-name"
```

5. Run target command through scoped proxy:

```powershell
cargo run -- run -- codex
cargo run -- run -- claude
```

## Commands

```text
route-cli install-core [--url <ZIP_URL>]
route-cli login-sub --url <SUB_URL>
route-cli update
route-cli list-nodes
route-cli use-node <NODE_NAME>
route-cli run -- <COMMAND...>
route-cli doctor
```

Dev mode equivalents:

```text
cargo run -- install-core -- [--url <ZIP_URL>]
cargo run -- login-sub --url <SUB_URL>
cargo run -- update
cargo run -- list-nodes
cargo run -- use-node <NODE_NAME>
cargo run -- run -- <COMMAND...>
cargo run -- doctor
```

## Config and cache paths

On Windows:

`%APPDATA%\route`

Files:

- `config.toml`
- `cache/subscription.yaml`
- `generated/sing-box.json`

Main `config.toml` keys:

- `subscription.url`
- `proxy_core.path` (default: `sing-box.exe`)
- `proxy.mixed_port` (default: `27890`)
- `routing.proxy_domains`
- `routing.no_proxy`
- `runtime.selected_node`

## sing-box path resolution order

`route-cli` resolves proxy core in this order:

1. `proxy_core.path` (absolute or relative)
2. If `proxy_core.path = "sing-box.exe"`, try `tools/sing-box/sing-box.exe`
3. `sing-box.exe` from PATH

Run diagnostics:

```powershell
cargo run -- doctor
```

## Troubleshooting

### Error: `No subscription URL configured`

```powershell
route-cli login-sub --url "<your-subscription-url>"
```

### Error: `Failed to start command ...: program not found`

Verify your target CLI is installed and callable in current terminal.

### Error: `proxy core unavailable`

Verify:

- `sing-box.exe` is available in PATH (for example: `winget install SagerNet.sing-box`)
- or `tools/sing-box/sing-box.exe` exists
- or set `proxy_core.path` to an absolute path

## Security and scope

- No system-wide proxy modifications
- No system route modifications
- Env vars apply only to the spawned process tree
- `sing-box` handles tunnel/forwarding; `route-cli` handles orchestration and policy
