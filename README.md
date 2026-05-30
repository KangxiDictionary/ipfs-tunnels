# ipfs-tunnels

**声明式 IPFS P2P 隧道管理器**

ipfs-tunnels 是一个轻量级守护进程，旨在通过 IPFS 网络实现安全可靠的内网服务穿透。它采用声明式设计，借鉴 Kubernetes Operator 的 Reconcile 模式，自动将实际运行状态收敛至配置文件所定义的期望状态。

支持 `client`（端口转发）和 `server`（服务暴露）两种模式，适用于 SSH、Minecraft、Web 服务等场景。

## 核心特性

- 双模式支持（`p2p forward` / `p2p listen`）
- 配置热重载（修改配置文件后自动生效）
- 周期性防漂移检查（每 60 秒）
- 事务性更新与失败回滚机制
- 本地端口冲突预检测
- 网络故障指数退避重试
- 跨平台优雅退出支持

## 快速开始

### 前置条件

- 已运行的 IPFS Daemon（默认 RPC 端口 5001）
- Rust 稳定版工具链

### 编译安装

```bash
git clone https://github.com/KangxiDictionary/ipfs-tunnels.git
cd ipfs-tunnels

# 通过 cargo 安装
cargo install --path .
```

### 启动

首次运行会自动在以下位置创建默认配置文件：

- Linux/macOS：`~/.config/ipfs-tunnels/tunnels.conf`
- Windows：`%APPDATA%\ipfs-tunnels\tunnels.conf`

```bash
ipfs-tunnels
```

程序启动后会自动检测 IPFS 节点连通性。

## 配置文件

采用 `|` 分隔的纯文本格式：

```text
# name | mode | local_ip | port | peer_id | protocol | enabled

# 客户端示例：将本地 Minecraft 服务通过 IPFS 转发
mc_client | client | 127.0.0.1 | 25565 | 12D3KooWxxxxxxxxxxxxxxxxxxxxxxxx | /x/minecraft | true

# 服务端示例：将本地 SSH 服务暴露到 IPFS 网络
ssh_server | server | 127.0.0.1 | 22 | - | /x/ssh | true
```

### 字段说明

| 字段       | 说明                              | 示例值 |
|------------|-----------------------------------|--------|
| name       | 隧道名称（仅标识）                | mc_client |
| mode       | 工作模式                          | client / server |
| local_ip   | 本地绑定 IP                       | 127.0.0.1 |
| port       | 本地端口                          | 25565 |
| peer_id    | 对方 PeerID（server 模式填 `-`） | 12D3KooW... |
| protocol   | 协议标识（全局唯一主键）          | /x/minecraft |
| enabled    | 是否启用                          | true / false |

**注意**：`protocol` 字段必须全局唯一。

## 使用场景

- 内网 SSH 服务通过 IPFS 安全访问
- 跨地域游戏服务器联机
- 临时暴露 Web 服务
- 替代传统端口映射，减少公网暴露风险

## 构建与开发

```bash
cargo build --release
cargo test
```

## License

本项目基于 **GNU General Public License v3.0**（GPL-3.0）开源。

详见 [LICENSE](LICENSE) 文件。
