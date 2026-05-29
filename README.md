# IPFS Tunnels Manager

IPFS Tunnels Manager 是一个基于声明式设计（Declarative Architecture）的 IPFS P2P 隧道编排守护进程。它参考了类似于 Kubernetes 的调和循环（Reconcile Loop）思想，通过周期性审查与文件热更新驱动，自动使 IPFS 节点当前的实际隧道状态（Actual State）向配置文件定义的期望状态（Desired State）逼近并最终完全收敛。

---

## 核心特性

* **双角色模式支持**：完整支持客户端模式（Client / `p2p forward`）与服务端模式（Server / `p2p listen`）。
* **自动状态调和**：自动挂载新隧道、清理陈旧废弃隧道、安全下线禁用隧道。
* **配置热重载（Hot-Reload）**：基于异步文件事件监听，检测到配置文件修改后立即触发增量调和，无需重启进程。
* **防漂移审查（Anti-Drift）**：内置每 60 秒一次的例行健康度审查，防止因外部因素导致的网络拓扑漂移。
* **鲁棒事务与回滚**：当配置更新失败时，系统提供事务性紧急回滚能力，尽可能将隧道恢复至变更前的可用状态。
* **指数退避重试**：针对瞬时网络故障提供指数退避重试机制，并设有一致性的最大延迟硬限幅，防止高并发下时间戳计算溢出。
* **跨平台安全优雅退出**：完美兼容 Linux（SIGINT/SIGTERM）与 Windows（Ctrl+C）系统的信号中断，保障退出时内存与网络连接的安全平稳排空。

---

## 配置文件说明

程序启动时，会在系统默认的配置目录下（例如 Linux 下的 `~/.config/ipfs-tunnels/` 或 Windows 的 AppData 对应目录）自动生成默认的规范化配置文件 `tunnels.conf`。

### 配置格式

配置文件采用 7 列竖线（`|`）分隔的扁平文本设计，支持使用 `#` 进行单行注释：

```text
# name | mode | local_ip | port | peer_id | protocol | enabled
mc_client  | client | 127.0.0.1 | 25565 | 12D3Koo... | /x/minecraft | true
ssh_server | server | 127.0.0.1 | 22    | -          | /x/ssh       | true

```

### 字段详解

| 字段名 | 允许值 | 说明 |
| --- | --- | --- |
| **name** | 字符串 | 隧道的人类可读标识名称。 |
| **mode** | `client` 或 `server` | 隧道的角色。`client` 对应本地端口前向转发；`server` 对应本地服务监听挂载。 |
| **local_ip** | IPv4 / IPv6 地址 | 本地映射或监听的 IP 地址（如 `127.0.0.1`）。 |
| **port** | 1 - 65535 | 本地映射或监听的 TCP 端口号。 |
| **peer_id** | IPFS PeerID / `-` | 目标节点的 PeerID。在 `server` 模式下，该项通常填写 `-` 作为占位。 |
| **protocol** | 字符串 | 唯一的 P2P 协议流路径（必须以 `/` 开头，如 `/x/ssh`）。作为核心主键使用。 |
| **enabled** | `true` 或 `false` | 是否启用该隧道。设为 `false` 时，调和器会自动在现网中将其关闭下线。 |

---

## 运行环境要求

1. **IPFS Daemon**：必须在本机运行 IPFS 节点，且其 RPC API 地址（默认 `http://127.0.0.1:5001`）需保持畅通。
2. **Rust 工具链**：编译需要 Rust 2021 edition 或更高版本。

---

## 快速开始

### 1. 克隆并编译项目

```bash
git clone https://github.com/your-username/ipfs-tunnels-manager.git
cd ipfs-tunnels-manager
cargo build --release

```

### 2. 启动服务

直接运行编译后的二进制程序：

```bash
cargo run --release

```

首次运行会在控制台提示创建默认模版配置。可以根据输出的路径找到 `tunnels.conf` 并编辑添加隧道配置。

### 3. 日志诊断

项目支持通过 `RUST_LOG` 环境变量动态解析复杂的过滤规则。默认输出 `info` 级别日志：

```bash
# 开启 debug 级别日志以观察调和循环细节
RUST_LOG=ipfs_tunnels_manager=debug,reconciler=debug ./target/release/ipfs-tunnels-manager

```

---

## 架构简述

系统核心事件循环采用 Tokio 的异步选择多路复用驱动：

* **开机初始化**：检查 IPFS 守护进程健康度，若离线则拒绝初始化。首次开机执行一次强制性声明式拓扑全量对齐。
* **文件变更通道**：通过 `notify` 监听文件事件，当 `tunnels.conf` 触发 `Modify` 或 `Create` 时，向主事件循环发送 `TriggerEvent::FileChanged` 信号。
* **定时器通道**：由独立异步任务驱动，每 60 秒发送一次 `TriggerEvent::PeriodicTick` 信号，用于防漂移同步。
* **预检查拦截（Pre-flight Check）**：在下发任何调和命令前，系统会静态扫描期望配置中是否存在本地端口冲突，若存在则直接熔断中止该轮调和，确保安全。
