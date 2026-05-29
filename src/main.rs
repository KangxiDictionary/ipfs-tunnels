//! src/main.rs
mod config;
mod error;
mod ipfs;
mod models;
mod reconciler;
mod signals;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

use config::{ensure_config_exists, load_desired_state};
use ipfs::IpfsClient;
use reconciler::reconcile_all;
use signals::wait_for_shutdown_signal; // 👈 引入跨平台信号包装

#[derive(Debug)]
enum TriggerEvent {
    FileChanged,
    PeriodicTick,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let subscriber = FmtSubscriber::builder().with_env_filter(filter).finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ipfs-tunnels");
    let config_file = config_dir.join("tunnels.conf");

    ensure_config_exists(&config_file)?;

    let client = Arc::new(IpfsClient::new());
    info!("🚀 IPFS Tunnels Operator 正在作为守护进程启动...");
    client.check_health().await.context("IPFS Daemon 离线，控制器拒绝初始化")?;

    let (tx, mut rx) = mpsc::unbounded_channel::<TriggerEvent>();

    // Hot-Reload 文件监听
    let tx_file = tx.clone();
    let config_file_cb = config_file.clone();
    let mut watcher = RecommendedWatcher::new(
        move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                if matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_))
                    && event.paths.iter().any(|p| p == &config_file_cb) {
                        let _ = tx_file.send(TriggerEvent::FileChanged);
                    }
            }
        },
        notify::Config::default(),
    )?;
    watcher.watch(&config_file, RecursiveMode::NonRecursive)?;

    // Anti-Drift 定时防漂移时钟
    let tx_tick = tx.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            let _ = tx_tick.send(TriggerEvent::PeriodicTick);
        }
    });

    // 首次开机主动强制对齐
    info!("执行开机初次声明式拓扑对齐...");
    let _ = run_reconcile_cycle(&client, &config_file).await;

    // 创建信号 Future 实例（固定 Pin 或直接在 select 中作为引用的 Future 执行）
    let shutdown_signal = wait_for_shutdown_signal();
    tokio::pin!(shutdown_signal);

    // ==========================================================
    // 核心事件循环 (Event Loop)：完美跨平台兼容
    // ==========================================================
    loop {
        tokio::select! {
            // 分支 A：接收到热更新或定时防漂移时钟信号
            Some(event) = rx.recv() => {
                match event {
                    TriggerEvent::FileChanged => info!("⚡ 检测到 tunnels.conf 配置文件热更新，开始触发增量调和..."),
                    TriggerEvent::PeriodicTick => info!("⏰ 触发 60s 例行周期健康度审查，同步实际拓扑中..."),
                }
                let _ = run_reconcile_cycle(&client, &config_file).await;
            }

            // 分支 B：不论是 Linux 的 SIGTERM/SIGINT 还是 Windows 的 Ctrl+C，只要被捕获就退出
            _ = &mut shutdown_signal => {
                break;
            }
        }
    }

    info!("✨ P2P Tunnel Operator 进程已安全优雅排空并平稳退出。");
    Ok(())
}

async fn run_reconcile_cycle(client: &Arc<IpfsClient>, config_file: &PathBuf) -> anyhow::Result<()> {
    let desired = match load_desired_state(config_file) {
        Ok(d) => d,
        Err(e) => {
            error!("❌ 读取本地新配置文件失败，放弃本轮调和，维持现网旧配置拓扑。原因: {:?}", e);
            return Err(e);
        }
    };

    let mut allocated_ports = HashSet::new();
    for tunnel in desired.values() {
        if tunnel.enabled && !allocated_ports.insert(tunnel.port) {
            error!("❌ 致命错误 (Pre-flight 拦截): 冲突的本地分配端口 [{}]！终止本轮配置下发。", tunnel.port);
            return Err(anyhow::anyhow!("本地静态端口映射冲突"));
        }
    }

    let actual = match client.load_actual_state().await {
        Ok(a) => a,
        Err(e) => {
            error!("⚠️ 无法从 IPFS Daemon 获取远程实际状态，跳过此轮调和。错误: {:?}", e);
            return Err(e);
        }
    };

    if let Err(reconcile_err) = reconcile_all(client.clone(), desired, actual).await {
        warn!("⚠️ 调和收敛执行结束，但控制面报出不完全一致警报: {}", reconcile_err);
    } else {
        info!("✅ 声明式拓扑状态成功进入完全收敛收尾。");
    }

    Ok(())
}
