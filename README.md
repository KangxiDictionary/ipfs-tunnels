# ipfs-tunnels-manager

**A Declarative IPFS P2P Tunnel Operator**

[中文说明](./README_zh.md)

`ipfs-tunnels-manager` is a lightweight background daemon designed to build secure, reliable peer-to-peer tunnels over the IPFS network. Adopting a declarative paradigm inspired by Kubernetes Operators, it runs a continuous reconcile loop to drive the actual network topology toward the state declared in your local configuration file.

It perfectly supports both `client` (port forwarding) and `server` (service exposing) modes, making it ideal for remotely accessing internal SSH, hosting Minecraft servers, or exposing temporary Web services without static public IPs.

---

## 🚀 Key Features

- **Declarative & Event-Driven**: Leverages a robust Reconcile loop with config hot-reloading (via `notify`) and a 60s periodic anti-drift enforcement clock.
- **Transactional Updates**: Prevents partial topology failures with a built-in roll-back engine when downstream IPFS RPC calls fail.
- **Pre-flight Conflict Validation**: Intercepts local port assignment overlap before touching production configurations.
- **Resilient Network Handling**: Exponential backoff retry engine for handling transient network dropouts elegantly.
- **Cross-Platform Daemonization**: Pure-native background service management script support for both Linux and Windows.

---

## 📦 Quick Start

### Prerequisites
- A running IPFS Daemon (Default RPC endpoint: `http://127.0.0.1:5001`)
- Stable Rust toolchain (`cargo`, `rustc`)

### 1. Clone & Build
```bash
git clone [https://github.com/KangxiDictionary/ipfs-tunnels.git](https://github.com/KangxiDictionary/ipfs-tunnels.git)
cd ipfs-tunnels

```

### 2. Automatic Background Installation

We provide native native scripts to install the executable and wire it up directly into your user-level system supervisor.

#### On Linux (Arch / CachyOS / Ubuntu)

This compiles the binary into `~/.cargo/bin/` and registers a `systemd --user` service:

```bash
chmod +x scripts/install.sh
./scripts/install.sh

```

*To watch real-time logs:* `journalctl --user -u ipfs-tunnels-manager -f`

#### On Windows

This compiles the executable and leverages the native Windows **Task Scheduler** to register a silent, windowless background task running at logon (No Admin/NSSM required!):

```powershell
./scripts/install.ps1

```

---

## ⚙️ Configuration

On its very first launch, the manager automatically populates a fully documented template file inside your user profile directory:

* **Linux**: `~/.config/ipfs-tunnels/tunnels.conf`
* **Windows**: `C:\Users\<Your-Username>\AppData\Roaming\ipfs-tunnels\tunnels.conf`

### Spec Format

The configuration uses a strict, pipe-separated (`|`) grid schema. You can edit it arbitrarily in real-time:

```text
# name | mode | local_ip | port | peer_id | protocol | enabled

# Client Mode: Map a remote Minecraft server hosted over IPFS onto your local port 25565
mc_client  | client | 127.0.0.1 | 25565 | 12D3KooWxxxxxxxxxxxxxxxxxxxxxxxx | /x/minecraft | true

# Server Mode: Safely expose your local SSH instance onto the IPFS network
ssh_server | server | 127.0.0.1 | 22    | -          | /x/ssh       | true

```

### Fields Definitions

| Field | Requirement | Description | Example |
| --- | --- | --- | --- |
| **name** | Optional | Arbitrary nickname for tracing logs | `mc_client` |
| **mode** | **Required** | `client` (forwarding) or `server` (listening) | `client` / `server` |
| **local_ip** | **Required** | Local loopback or binding interface | `127.0.0.1` |
| **port** | **Required** | Target network port (must be unique among enabled nodes) | `25565` |
| **peer_id** | Conditional | Remote IPFS Node ID (Must provide in client mode; use `-` for server) | `12D3KooW...` |
| **protocol** | **Required** | **Global Unique Identifier (Primary Key)** for the P2P stream. | `/x/minecraft` |
| **enabled** | **Required** | Toggles the runtime mount state dynamically | `true` / `false` |

---

## 🛠️ Troubleshooting

### 1. Operator fails to initialize on startup
**Symptom:** Log says `IPFS Daemon is offline, controller refused to initialize.`
**Fix:** The manager requires a responsive IPFS node. Ensure your node is healthy by querying its status manually:
```bash
# Verify IPFS RPC is reachable
curl -X POST [http://127.0.0.1:5001/api/v0/id](http://127.0.0.1:5001/api/v0/id)

```

### 2. Local Port Allocation Conflict

**Symptom:** Log says `Fatal Error (Pre-flight Interception): Conflicting local port allocation [xxxx]! Terminating current reconcile cycle.`
**Fix:** Two or more tunnels marked as `enabled | true` are binding to the exact same `port`. Open `tunnels.conf` and assign distinct local ports to fix the collision.

---

## 📜 License

This project is licensed under the **GNU General Public License v3.0** (GPL-3.0). See the [LICENSE](https://www.google.com/search?q=LICENSE) file for details.