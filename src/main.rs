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
use signals::wait_for_shutdown_signal;

#[derive(Debug)]
enum TriggerEvent {
    FileChanged,
    PeriodicTick,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let subscriber = FmtSubscriber::builder().with_env_filter(filter).finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ipfs-tunnels");
    let config_file = config_dir.join("tunnels.conf");

    ensure_config_exists(&config_file)?;

    let client = Arc::new(IpfsClient::new());
    // 👈 简化：告别中二词汇，直接说干啥
    info!("服务正在启动...");
    client.check_health().await.context("无法连接到本地 IPFS 节点，请检查服务是否运行")?;

    let (tx, mut rx) = mpsc::unbounded_channel::<TriggerEvent>();

    // Hot-Reload
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

    // 定时检查
    let tx_tick = tx.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            let _ = tx_tick.send(TriggerEvent::PeriodicTick);
        }
    });

    info!("正在执行初始状态同步...");
    let _ = run_reconcile_cycle(&client, &config_file).await;

    let shutdown_signal = wait_for_shutdown_signal();
    tokio::pin!(shutdown_signal);

    loop {
        tokio::select! {
            Some(event) = rx.recv() => {
                match event {
                    // 👈 简化：直白明了
                    TriggerEvent::FileChanged => info!("检测到 tunnels.conf 发生修改，开始同步状态..."),
                    TriggerEvent::PeriodicTick => info!("执行 60 秒定时状态检查..."),
                }
                let _ = run_reconcile_cycle(&client, &config_file).await;
            }
            _ = &mut shutdown_signal => {
                break;
            }
        }
    }

    info!("服务已安全停止。");
    Ok(())
}

async fn run_reconcile_cycle(client: &Arc<IpfsClient>, config_file: &PathBuf) -> anyhow::Result<()> {
    let desired = match load_desired_state(config_file) {
        Ok(d) => d,
        Err(e) => {
            error!("读取配置文件失败，跳过本次同步。原因: {:?}", e);
            return Err(e);
        }
    };

    let mut allocated_ports = HashSet::new();
    for tunnel in desired.values() {
        if tunnel.enabled && !allocated_ports.insert(tunnel.port) {
            // 👈 简化：去掉 Pre-flight 拦截等术语
            error!("配置错误：端口 [{}] 存在冲突！终止状态同步。", tunnel.port);
            return Err(anyhow::anyhow!("本地端口冲突"));
        }
    }

    let actual = match client.load_actual_state().await {
        Ok(a) => a,
        Err(e) => {
            error!("无法从 IPFS 读取运行状态，跳过本次同步。原因: {:?}", e);
            return Err(e);
        }
    };

    if let Err(reconcile_err) = reconcile_all(client.clone(), desired, actual).await {
        warn!("部分隧道同步失败: {}", reconcile_err);
    } else {
        info!("所有隧道状态同步完成。");
    }

    Ok(())
}
