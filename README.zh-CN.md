# codex-route

`codex-route` 是一个面向 Windows 的 Rust CLI 工具，用于让 `Codex CLI` 按进程走代理，而不是修改系统全局网络配置。

## 项目目标

- 只影响目标命令（例如 `codex`）及其子进程
- 不修改系统代理
- 不修改系统路由
- 通过 Clash 订阅 + `sing-box` 实现公网转发

## 适用场景

在公司网络中，常见问题是：
- 访问 Codex/OpenAI 需要代理
- 但全局 VPN 会影响内网资源访问

`codex-route` 的方案是“进程级代理”，仅对你启动的目标命令生效。

## 功能概览

- 拉取并解析 Clash 订阅 YAML
- 本地缓存节点
- 列出节点支持状态
- 支持手动选节点或自动选节点
- 生成 `sing-box` 运行配置
- 启动本地 `sing-box` 核心
- 启动目标命令并注入环境变量：
  - `HTTP_PROXY`
  - `HTTPS_PROXY`
  - `ALL_PROXY`
  - `NO_PROXY`

## 节点选择策略

执行 `codex-route run -- <command>` 时：

1. 如果 `runtime.selected_node` 已存在，先尝试该节点。
2. 其余候选按优先级排序：
   - 新加坡
   - 韩国
   - 美国
   - 其他
3. 每个候选先做一次 `ping` 连通性测试。
4. 选中首个可达节点，并写回 `runtime.selected_node`。

## 支持的节点类型

当前支持的 Clash 节点类型：

- `socks5`
- `socks`
- `http`
- `ss`
- `vmess`

当前限制：

- `ss` 带 plugin 字段暂不支持
- `vmess` 已支持常见 `tcp/ws/grpc`，高级变体可能需要补充映射

## 目录结构（推荐）

```text
codex-route/
  Cargo.toml
  src/
  tools/
    sing-box/
      sing-box.exe
```

建议发布包结构：

- `codex-route.exe`
- `tools/sing-box/sing-box.exe`
- `tools/sing-box/*.dll`（若该版本存在依赖 DLL）

## 环境要求

- Windows
- Rust（源码构建时需要）
- 已安装 Codex CLI，且终端可执行 `codex`
- 可用的 `sing-box` 二进制（内置或 PATH 可见）

## 构建

```powershell
cd D:\WorkSpace\ai-lab\codex-route
cargo build
```

## 快速开始

1. 配置订阅地址

```powershell
cargo run -- login-sub --url "https://example.com/subscription.yaml"
```

2. 拉取并缓存订阅

```powershell
cargo run -- update
```

3. 查看节点

```powershell
cargo run -- list-nodes
```

4. 可选：手动指定节点

```powershell
cargo run -- use-node "your-node-name"
```

5. 通过进程级代理启动 Codex

```powershell
cargo run -- run -- codex
```

## 登录方式（推荐设备码）

如果浏览器跳转受限，建议使用设备码登录：

```powershell
cargo run -- run -- codex login --device-auth
```

流程：

1. 终端输出验证网址和设备码
2. 在任意可上网浏览器（可用手机）打开验证网址
3. 输入设备码完成授权
4. 回到终端等待登录完成

查看登录状态：

```powershell
cargo run -- run -- codex login status
```

## 命令列表

```text
codex-route login-sub --url <SUB_URL>
codex-route update
codex-route list-nodes
codex-route use-node <NODE_NAME>
codex-route run -- <COMMAND...>
codex-route doctor
```

开发态等价命令：

```text
cargo run -- login-sub --url <SUB_URL>
cargo run -- update
cargo run -- list-nodes
cargo run -- use-node <NODE_NAME>
cargo run -- run -- <COMMAND...>
cargo run -- doctor
```

## 配置与缓存路径

Windows 路径：

`%APPDATA%\codex-route`

包含：

- `config.toml`
- `cache/subscription.yaml`
- `generated/sing-box.json`

主要配置项：

- `subscription.url`
- `proxy_core.path`（默认 `tools/sing-box/sing-box.exe`）
- `proxy.mixed_port`（默认 `27890`）
- `routing.proxy_domains`
- `routing.no_proxy`
- `runtime.selected_node`

## sing-box 路径解析顺序

程序按以下顺序解析 `sing-box`：

1. `proxy_core.path`（绝对或相对）
2. `tools/sing-box/sing-box.exe`（相对工作目录或可执行文件目录）
3. PATH 中的 `sing-box.exe`（回退）

可用以下命令检查：

```powershell
cargo run -- doctor
```

## 常见问题

### 1) `No subscription URL configured`

```powershell
cargo run -- login-sub --url "<你的订阅地址>"
```

### 2) `Failed to start command 'codex': program not found`

请确认：

- Codex CLI 已安装
- 当前终端可执行 `codex --version`

### 3) `proxy core unavailable`

请确认：

- `tools/sing-box/sing-box.exe` 存在
- 或 `proxy_core.path` 指向有效绝对路径

### 4) 节点很多但仍无法连接

请检查：

- 订阅内容是否有效
- 当前网络是否可访问节点服务器
- 本地策略是否屏蔽 `ping`（会影响预检）

### 5) 改名迁移后配置看起来丢失

若应用名变更（例如 `codex-vpn` -> `codex-route`），配置目录也会变化。
请在新名称下重新执行 `login-sub`。

## 安全边界

- 不改系统全局代理
- 不改系统全局路由
- 仅给目标进程树注入代理环境变量
- 隧道与转发由 `sing-box` 执行，`codex-route` 负责编排和策略

## 开发命令

```powershell
cargo fmt --all
cargo check
```
