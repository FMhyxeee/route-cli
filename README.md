# codex-route

`codex-route` is a Windows-first Rust CLI for running `Codex CLI` through a scoped proxy path.
It does not change system-wide proxy settings or system routes.

## Why this tool

In many enterprise networks, you need proxy/VPN access for public services, but full-tunnel VPN breaks internal resources.
`codex-route` solves this by applying proxy settings only to the target process tree (for example, `codex`), not the whole OS.

## What it does

- Downloads and parses Clash subscription YAML
- Caches nodes locally
- Lists node support status
- Selects nodes automatically or manually
- Generates `sing-box` runtime config
- Starts local `sing-box` proxy core
- Launches the target command with process-scoped proxy env vars:
  - `HTTP_PROXY`
  - `HTTPS_PROXY`
  - `ALL_PROXY`
  - `NO_PROXY`

## Node selection behavior

When you run `codex-route run -- <command>`:

1. If `runtime.selected_node` exists, it tries that node first.
2. Remaining nodes are sorted by region priority:
   - Singapore
   - Korea
   - United States
   - Others
3. Each candidate is checked with `ping` before use.
4. The first reachable node is selected and persisted as `runtime.selected_node`.

## Supported Clash node types

- `socks5`
- `socks`
- `http`
- `ss`
- `vmess`

Current limitations:

- `ss` with plugin fields is not supported yet.
- `vmess` supports common `tcp/ws/grpc` mappings; advanced variants may need extra mapping.

## Project layout

```text
codex-route/
  Cargo.toml
  src/
  tools/
    sing-box/
      sing-box.exe
```

Recommended distribution bundle:

- `codex-route.exe`
- `tools/sing-box/sing-box.exe`
- `tools/sing-box/*.dll` (if required by the downloaded build)

## Requirements

- Windows
- Rust toolchain (for building from source)
- `codex` CLI installed and available from terminal
- `sing-box` binary (bundled or on PATH)

## Build

```powershell
cd D:\WorkSpace\ai-lab\codex-route
cargo build
```

## Quick start

1. Save subscription URL:

```powershell
cargo run -- login-sub --url "https://example.com/subscription.yaml"
```

2. Pull and cache subscription:

```powershell
cargo run -- update
```

3. List nodes:

```powershell
cargo run -- list-nodes
```

4. Optional: manually pick a node:

```powershell
cargo run -- use-node "your-node-name"
```

5. Run Codex through scoped proxy:

```powershell
cargo run -- run -- codex
```

## Login flow (device auth recommended)

If browser callback/jump is inconvenient in your network, use device auth:

```powershell
cargo run -- run -- codex login --device-auth
```

Flow:

1. Terminal prints verification URL + device code.
2. Open the URL from any internet-capable browser (desktop or mobile).
3. Enter device code and approve.
4. Return to terminal and wait for completion.

Check login status:

```powershell
cargo run -- run -- codex login status
```

## Commands

```text
codex-route login-sub --url <SUB_URL>
codex-route update
codex-route list-nodes
codex-route use-node <NODE_NAME>
codex-route run -- <COMMAND...>
codex-route doctor
```

Dev mode equivalents:

```text
cargo run -- login-sub --url <SUB_URL>
cargo run -- update
cargo run -- list-nodes
cargo run -- use-node <NODE_NAME>
cargo run -- run -- <COMMAND...>
cargo run -- doctor
```

## Config and cache paths

On Windows:

`%APPDATA%\codex-route`

Files:

- `config.toml`
- `cache/subscription.yaml`
- `generated/sing-box.json`

Main `config.toml` keys:

- `subscription.url`
- `proxy_core.path` (default: `tools/sing-box/sing-box.exe`)
- `proxy.mixed_port` (default: `27890`)
- `routing.proxy_domains`
- `routing.no_proxy`
- `runtime.selected_node`

## sing-box path resolution order

`codex-route` resolves proxy core in this order:

1. `proxy_core.path` (absolute or relative)
2. `tools/sing-box/sing-box.exe` (relative to current working dir or executable dir)
3. `sing-box.exe` from PATH (fallback)

Run diagnostics:

```powershell
cargo run -- doctor
```

## Troubleshooting

### Error: `No subscription URL configured`

Run:

```powershell
cargo run -- login-sub --url "<your-subscription-url>"
```

### Error: `Failed to start command 'codex': program not found`

Verify:

- Codex CLI is installed
- `codex --version` works in the current terminal

### Error: `proxy core unavailable`

Verify:

- `tools/sing-box/sing-box.exe` exists
- or set `proxy_core.path` to an absolute valid path

### Many nodes but still cannot connect

Check:

- subscription is valid
- network can reach node servers
- `ping` is not blocked by local policy/firewall

### Config appears lost after rename/migration

If project/app name changed, config folder may also change (for example `codex-vpn` -> `codex-route`).
Re-run `login-sub` in the new app context.

## Security and scope

- No system-wide proxy modifications
- No system route modifications
- Proxy env vars only apply to the spawned process tree
- Tunnel and forwarding are handled by `sing-box`; `codex-route` orchestrates policy and process lifecycle

## Development

```powershell
cargo fmt --all
cargo check
```
