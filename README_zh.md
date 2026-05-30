# ipfs-tunnels-manager

**声明式 IPFS P2P 隧道管理器**

[English Version](./README.md)

`ipfs-tunnels-manager` 是一个轻量级的后台守护进程，旨在通过 IPFS 网络实现安全可靠的内网服务穿透。它采用了借鉴自 Kubernetes Operator 的**声明式设计与调和（Reconcile）模式**，会自动运行一个持续的事件循环，将系统现网的实际网络拓扑状态，强行收敛至你在本地配置文件中所宣告的“期望状态”。

项目完美支持 `client`（端口转发）与 `server`（服务暴露）两种工作模式。非常适合在没有公网 IP 的环境下，远程安全访问内网 SSH、联机托管游戏服务器（如 Minecraft）或临时暴露本地 Web 服务。

---

## 🚀 核心特性

- **声明式与事件驱动**：通过 `notify` 监听文件实现配置秒级热重载，并辅以 60 秒周期性硬核防漂移时钟，确保拓扑绝对安全。
- **事务性状态更新**：当下发新拓扑遭遇 IPFS 拒绝或底层 RPC 故障时，自动触发原子级快照回滚，避免网络接口悬挂损坏。
- **开机前端口冲突拦截**：内置 Pre-flight 静态审查，在配置正式下发前强行拦截重复的本地端口分配。
- **指数退避容错**：面对瞬时网络抖动或 RPC 拥堵时，采用带硬限幅保护的指数退避重试机制。
- **双路原生常驻守护**：针对 Linux 与 Windows 平台提供无第三方依赖的原生后台服务化部署脚本。

---

## 📦 快速开始

### 前置条件
- 正在运行的 IPFS Daemon 节点（默认 RPC 端口：`5001`）
- 稳定的 Rust 编译工具链（`cargo`、`rustc`）

### 1. 克隆项目
```bash
git clone [https://github.com/KangxiDictionary/ipfs-tunnels.git](https://github.com/KangxiDictionary/ipfs-tunnels.git)
cd ipfs-tunnels

```

### 2. 自动化后台常驻部署

项目内置了自动化安装脚本，会自动将其编译、安装并注册进系统级服务管理器。

#### Linux 平台 (Arch / CachyOS / Ubuntu 等)

执行脚本会自动通过 Cargo 编译 release 产物，并无缝注册为 `systemd --user` 用户级后台服务（无需 sudo 权限）：

```bash
chmod +x scripts/install.sh
./scripts/install.sh

```

*实时查看后台彩色日志：* `journalctl --user -u ipfs-tunnels-manager -f`

#### Windows 平台

执行 PowerShell 脚本会自动调用编译，并利用 Windows 原生的**任务计划程序**注册一个随用户登录自动静默拉起的后台任务（无需管理员权限，完美绕过原生服务超时强杀，无黑窗口弹窗）：

```powershell
./scripts/install.ps1

```

---

## ⚙️ 配置文件说明

程序在初次开机启动时，会自动在用户的家目录/配置目录下初始化一份带完整注释的模板文件：

* **Linux**：`~/.config/ipfs-tunnels/tunnels.conf`
* **Windows**：`C:\Users\<你的用户名>\AppData\Roaming\ipfs-tunnels\tunnels.conf`

### 规范格式

配置文件使用管道符 (`|`) 作为列分隔符。你可以随时直接用编辑器对其进行修改，变更会自动被热重载捕获：

```text
# name | mode | local_ip | port | peer_id | protocol | enabled

# 客户端示例：将远程通过 IPFS 暴露的 Minecraft 服务，映射到本地的 25565 端口
mc_client  | client | 127.0.0.1 | 25565 | 12D3KooWxxxxxxxxxxxxxxxxxxxxxxxx | /x/minecraft | true

# 服务端示例：将本地的 SSH (22端口) 安全暴露到整个 IPFS 网络的 P2P 协议流中
ssh_server | server | 127.0.0.1 | 22    | -          | /x/ssh       | true

```

### 字段说明表

| 字段 | 是否必填 | 作用说明 | 示例值 |
| --- | --- | --- | --- |
| **name** | 选填 | 隧道别名，仅用于日志追踪标识 | `mc_client` |
| **mode** | **必填** | 角色模式：`client` (端口转发) 或 `server` (暴露本地服务) | `client` 或 `server` |
| **local_ip** | **必填** | 本地绑定的回环地址或网卡 IP | `127.0.0.1` |
| **port** | **必填** | 本地监听或转接的端口（在所有启用的隧道中必须唯一） | `25565` |
| **peer_id** | 条件必填 | 对端 IPFS 节点的 PeerID（Client模式必填；Server模式下用 `-` 占位） | `12D3KooW...` |
| **protocol** | **必填** | **全局唯一的主键**，代表 IPFS P2P 网络中的流协议标识 | `/x/minecraft` |
| **enabled** | **必填** | 动态控制该隧道的启用与下线状态 | `true` 或 `false` |

---

## 🛠️ 常见故障排查

### 1. 启动时报初始化拒绝错误

**现象：** 日志抛出 `IPFS Daemon 离线，控制器拒绝初始化`。
**解决：** 本程序高度依赖本地 IPFS 节点的 RPC 接口。请确保你的 IPFS Daemon 已正常启动且 5001 端口可连通，可通过以下命令测试：

```bash
curl -X POST [http://127.0.0.1:5001/api/v0/id](http://127.0.0.1:5001/api/v0/id)

```

### 2. 静态本地端口冲突拦截

**现象：** 日志抛出 `致命错误 (Pre-flight 拦截): 冲突的本地分配端口...`。
**解决：** 配置文件中有两个或以上状态为 `enabled | true` 的隧道指定了完全相同的 `port`。请打开 `tunnels.conf` 将它们的本地端口修改错开即可。

---

## 📜 开源协议

本项目基于 **GNU General Public License v3.0** (GPL-3.0) 协议开源。详情请参阅 [LICENSE](https://www.google.com/search?q=LICENSE) 文件。