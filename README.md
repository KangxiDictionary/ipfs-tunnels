---

# IPFS Tunnels Operator 🚀

`IPFS Tunnels Operator` 是一个基于 Rust 编写的、采用声明式（Declarative）架构的 IPFS P2P 隧道守护进程。它类似于 Kubernetes 的 Controller 模式，通过不断比对“用户期望状态（Desired State）”与“网关实际状态（Actual State）”，自动执行增量调和（Reconcile），确保 P2P 转发与监听拓扑的最终一致性。

支持跨平台的安全优雅停机，并具备生产级的抗漂移、高并发错误指数退避与事务性回滚机制。

---

## 核心特性 ✨

* **声明式拓扑对齐 (Declarative Reconciliation)**：
通过配置文件定义隧道，Operator 自动发现未就绪隧道、清理陈旧残留、下线禁用路由。
* **双角色支持 (Client / Server)**：
完美映射并统一调度 IPFS 的 `p2p forward` (Client 模式) 与 `p2p listen` (Server 模式)。
* **事务性更新与紧急回滚 (Transactional Rollback)**：
配置变更或现网漂移时，采用先下线旧路由、再挂载新路由的事务性逻辑。若新配置激活失败，将触发自动紧急回滚，防止隧道处于悬挂损毁状态。
* **主动防漂移时钟 (Anti-Drift Clock)**：
除支持配置文件热更新（Hot-Reload）触发外，内置 60 秒例行周期审查，强力纠正外部手动干预导致的拓扑漂移。
* **生产级健壮性设计**：
* **Pre-flight 拦截**：下线配置前进行静态本地端口冲突校验，杜绝冲突配置下发。
* **安全指数退避**：网络瞬时故障自动触发指数退避重试，并使用硬限幅（上限 30s）配合安全的边界计算，防止高并发重试时时间溢出。


* **优雅停机与跨平台信号包装 (Cancellation Safety)**：
统一封装跨平台信号拦截，Unix（SIGINT/SIGTERM）与 Windows（Ctrl+C）均可实现进程安全排空并平稳退出。

---

## 架构原理 🧭

1. **事件触发**：由 `notify` 文件修改事件（热更新）或 `tokio::time::interval`（定时防漂移）向核心事件循环发送 `TriggerEvent`。
2. **状态收集**：解析 `tunnels.conf` 获取 Desired 状态；通过 IPFS RPC (`/api/v0/p2p/ls`) 解析并鉴别 Actual 状态。
3. **并发调和**：使用 `futures::stream::buffer_unordered` 并发（上限 10）处理每一个协议隧道的生命周期状态机，计算差异并按需调用 IPFS 接口。

---

## 配置文件规范 📝

配置文件路径默认为：`~/.config/ipfs-tunnels/tunnels.conf`（若不存在，启动时会自动生成模板）。
配置采用 **7 列严格规范格式**，以 `|` 作为分隔符。

```text
# name       | mode   | local_ip  | port  | peer_id     | protocol     | enabled
mc_client    | client | 127.0.0.1 | 25565 | 12D3Koo...  | /x/minecraft | true
ssh_server   | server | 127.0.0.1 | 22    | -           | /x/ssh       | true

```

### 字段说明：

* **name**: 隧道别名，用于日志追踪与可读性。
* **mode**: 角色模式。可选 `client`（执行 forward 转发）或 `server`（执行 listen 监听）。
* **local_ip**: 本地监听或绑定的 IP 地址（支持 IPv4 / IPv6）。
* **port**: 本地静态映射端口。**全局不可重复占用。**
* **peer_id**: 对端的 IPFS PeerID。在 `server` 模式下无需填写，使用 `-` 占位即可。
* **protocol**: P2P 协议标识符路径（必须以 `/` 开头）。
* **enabled**: 是否启用。设为 `false` 会触发 Operator 自动安全下线该隧道。

---

## 编译与运行 🛠️

### 前提条件

1. 已安装 [Rust 工具链](https://rustup.rs/) (Cargo & rustc)。
2. 本地正在运行 IPFS Daemon，且 RPC 接口默认开启在 `http://127.0.0.1:5001`。

### 编译项目

```bash
cargo build --release

```

### 运行守护进程

可以直接通过 Cargo 运行，或将编译产物作为 Systemd / 守护服务挂载：

```bash
# 默认读取或生成 ~/.config/ipfs-tunnels/tunnels.conf
RUST_LOG=info cargo run

```

---

## 日志追踪样例 📋

**开机初次对齐与热更新：**

```text
INFO  ipfs_tunnels_operator: 🚀 IPFS Tunnels Operator 正在作为守护进程启动...
INFO  ipfs_tunnels_operator: 执行开机初次声明式拓扑对齐...
INFO  reconcile_loop{protocol="/x/minecraft"}: 发现未就绪隧道，正在创建... tunnel_name=mc_client mode=Client
INFO  reconcile_loop{protocol="/x/minecraft"}: 隧道成功挂载
INFO  ipfs_tunnels_operator: ✅ 声明式拓扑状态成功进入完全收敛收尾。
INFO  ipfs_tunnels_operator: ⚡ 检测到 tunnels.conf 配置文件热更新，开始触发增量调和...
WARN  reconcile_loop{protocol="/x/minecraft"}: ⚠️ 检测到配置存在漂移！启动事务性更新... tunnel_name=mc_client

```

**优雅停机：**

```text
^CWARN  ipfs_tunnels_operator::signals: 🛑 接收到 SIGINT (Ctrl+C) 终止信号。执行 Cancellation Safety 保护退出...
INFO  ipfs_tunnels_operator: ✨ P2P Tunnel Operator 进程已安全优雅排空并平稳退出。

```

---

## 项目结构 📁

* `main.rs`: 守护进程的驱动核心，调度异步事件循环（Event Loop）与热加载。
* `config.rs`: 负责 7 列配置文件的声明式解析、校验以及模板自生成。
* `models.rs`: 统一的内部领域模型（`DesiredTunnel` / `ActualTunnel`）和 IPFS 响应反序列化结构。
* `ipfs.rs`: 封装面向 IPFS RPC 接口的底层 HTTP 客户端，内置安全的指数退避重试网络代理。
* `reconciler.rs`: 控制面核心。高并发计算拓扑差异，编排创建、下线、禁用清理及回滚事务。
* `error.rs`: 基于 `thiserror` 定义的错误枚举，明确区分可重试网络故障与致命配置拒绝。
* `signals.rs`: 跨平台的系统信号拦截器，保障多平台底层的优雅停机行为一致。

---

## License 📄

本项目遵循开源社区规范，基于明晰的开放源代码精神构建。详情请参阅项目中的 LICENSE 文件。