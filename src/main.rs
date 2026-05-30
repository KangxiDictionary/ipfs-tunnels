mod config;
mod error;
mod ipfs;
mod models;
mod reconciler;
mod signals;
mod i18n; // 👈 引入多语言模块

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
use i18n::{tr, LogKey}; // 👈 导入翻译方法

#[derive(Debug)]
enum TriggerEvent {
    FileChanged,
    PeriodicTick,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. 优先初始化语言环境
    i18n::init();

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let subscriber = FmtSubscriber::builder().with_env_filter(filter).finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ipfs-tunnels");
    let config_file = config_dir.join("tunnels.conf");

    ensure_config_exists(&config_file)?;

    let client = Arc::new(IpfsClient::new());

    // 👈 结构化输出翻译后的文本
    info!("{}", tr(LogKey::ServiceStarting));
    client.check_health().await.context(tr(LogKey::IpfsConnectError))?;

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

    // 定时器
    let tx_tick = tx.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            let _ = tx_tick.send(TriggerEvent::PeriodicTick);
        }
    });

    info!("{}", tr(LogKey::InitialSync));
    let _ = run_reconcile_cycle(&client, &config_file).await;

    let shutdown_signal = wait_for_shutdown_signal();
    tokio::pin!(shutdown_signal);

    loop {
        tokio::select! {
            Some(event) = rx.recv() => {
                match event {
                    TriggerEvent::FileChanged => info!("{}", tr(LogKey::ConfigChanged)),
                    TriggerEvent::PeriodicTick => info!("{}", tr(LogKey::PeriodicCheck)),
                }
                let _ = run_reconcile_cycle(&client, &config_file).await;
            }
            _ = &mut shutdown_signal => {
                break;
            }
        }
    }

    info!("{}", tr(LogKey::ServiceStopped));
    Ok(())
}

async fn run_reconcile_cycle(client: &Arc<IpfsClient>, config_file: &PathBuf) -> anyhow::Result<()> {
    let desired = match load_desired_state(config_file) {
        Ok(d) => d,
        Err(e) => {
            error!(error = ?e, "{}", tr(LogKey::ConfigReadError));
            return Err(e);
        }
    };

    let mut allocated_ports = HashSet::new();
    for tunnel in desired.values() {
        if tunnel.enabled && !allocated_ports.insert(tunnel.port) {
            // 👈 把 port 当作结构化字段传入，字符串本体保持静态以支持国际化
            error!(port = tunnel.port, "{}", tr(LogKey::PortConflict));
            return Err(anyhow::anyhow!("Local port conflict"));
        }
    }

    let actual = match client.load_actual_state().await {
        Ok(a) => a,
        Err(e) => {
            error!(error = ?e, "{}", tr(LogKey::IpfsReadError));
            return Err(e);
        }
    };

    if let Err(reconcile_err) = reconcile_all(client.clone(), desired, actual).await {
        warn!(error = %reconcile_err, "{}", tr(LogKey::SyncFailed));
    } else {
        info!("{}", tr(LogKey::SyncComplete));
    }

    Ok(())
}
