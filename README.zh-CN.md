# route-cli

`route-cli` 是一个面向 Windows 的 Rust CLI 工具，用于让任意命令行工具按进程走代理。
它不会修改系统全局代理或系统路由。

## 适用场景

在企业网络中，全局 VPN 常常会影响内网访问。
`route-cli` 只给目标进程树注入代理环境变量（例如 `codex`、`claude` 等），不影响整个系统。

## 功能

- 下载并解析 Clash 订阅 YAML
- 本地缓存节点
- 列出节点可用性
- 自动/手动选择节点
- 生成 `sing-box` 运行配置
- 启动本地 `sing-box` 内核
- 启动目标命令并注入 `HTTP_PROXY` / `HTTPS_PROXY` / `ALL_PROXY` / `NO_PROXY`

## 节点选择规则

执行 `route-cli run -- <command>` 时：

1. 若已配置 `runtime.selected_node`，优先尝试该节点。
2. 其他节点按区域优先级排序：新加坡、韩国、美国、其他。
3. 每个节点先做 `ping` 检查。
4. 选取第一个可达节点并持久化。

## 支持的节点类型

- `socks5`
- `socks`
- `http`
- `ss`
- `vmess`

当前限制：

- 暂不支持带 plugin 的 `ss`。
- `vmess` 支持常见 `tcp/ws/grpc`，高级变体可能需补充映射。

## 目录结构

```text
route-cli/
  Cargo.toml
  src/
  tools/
    sing-box/
      sing-box.exe
```

建议发布包包含：

- `route-cli.exe`
- `tools/sing-box/sing-box.exe`
- `tools/sing-box/*.dll`（如构建版本需要）

## 环境要求

- Windows
- Rust 工具链（源码构建时需要）
- 目标 CLI 可在终端直接执行
- `sing-box`（通过 `route-cli install-core` 安装、随包携带或在 PATH 中）

## 快速开始

1. 安装代理内核：

```powershell
cargo run -- install-core
```

`install-core` 会优先使用本地 `tools/sing-box`，找不到时再从 GitHub 下载 Windows AMD64 版本。

2. 保存订阅地址：

```powershell
cargo run -- login-sub --url "https://example.com/subscription.yaml"
```

3. 更新订阅缓存：

```powershell
cargo run -- update
```

4. 可选：手动选节点：

```powershell
cargo run -- use-node "your-node-name"
```

5. 通过代理运行目标命令：

```powershell
cargo run -- run -- codex
cargo run -- run -- claude
```

## 命令

```text
route-cli install-core [--url <ZIP_URL>]
route-cli login-sub --url <SUB_URL>
route-cli update
route-cli list-nodes
route-cli use-node <NODE_NAME>
route-cli run -- <COMMAND...>
route-cli doctor
```

## 配置路径

Windows 下目录：

`%APPDATA%\route`

主要文件：

- `config.toml`
- `cache/subscription.yaml`
- `generated/sing-box.json`

## `sing-box` 路径解析顺序

1. 使用 `proxy_core.path`（绝对或相对路径）
2. 若 `proxy_core.path = "sing-box.exe"`，尝试 `tools/sing-box/sing-box.exe`
3. 回退到 PATH 中的 `sing-box.exe`

## 常见问题

### `No subscription URL configured`

```powershell
route-cli login-sub --url "<你的订阅地址>"
```

### `Failed to start command ...: program not found`

确认目标 CLI 已安装，并且当前终端可直接执行。

### `proxy core unavailable`

确认以下任一条件：

- PATH 中可执行 `sing-box.exe`（例如：`winget install SagerNet.sing-box`）
- 或本地有 `tools/sing-box/sing-box.exe`
- 或 `proxy_core.path` 指向有效绝对路径

## 安全边界

- 不修改系统全局代理
- 不修改系统全局路由
- 仅影响 `route-cli run -- ...` 启动的进程树
- 隧道与转发由 `sing-box` 执行，`route-cli` 负责编排与策略
