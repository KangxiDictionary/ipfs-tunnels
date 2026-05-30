use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

// 🌟 修改：通过库包名引入内部模块
use ipfs_tunnels_manager::config::{ensure_config_exists, load_desired_state};
use ipfs_tunnels_manager::error::ReconcileError;
use ipfs_tunnels_manager::i18n::{self, tr, LogKey};
use ipfs_tunnels_manager::ipfs::IpfsClient;
use ipfs_tunnels_manager::reconciler::{self, reconcile_all};
use ipfs_tunnels_manager::signals::wait_for_shutdown_signal;

#[derive(Debug)]
enum TriggerEvent {
    FileChanged,
    PeriodicTick,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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
    info!("{}", tr(LogKey::ServiceStarting));

    if let Err(e) = client.load_actual_state().await {
        error!("IPFS Daemon 离线或联络失败，控制器拒绝初始化启动。错误原因: {:?}", e);
        return Err(anyhow::anyhow!("IPFS connection refused on startup"));
    }

    let (tx, mut rx) = mpsc::channel(32);

    let tx_file = tx.clone();
    let mut watcher = RecommendedWatcher::new(
        move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                if matches!(event.kind, EventKind::Modify(_)) {
                    let _ = tx_file.try_send(TriggerEvent::FileChanged);
                }
            }
        },
        notify::Config::default(),
    )?;
    watcher.watch(&config_file, RecursiveMode::NonRecursive)?;

    let tx_tick = tx.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            let _ = tx_tick.try_send(TriggerEvent::PeriodicTick);
        }
    });

    info!("{}", tr(LogKey::InitialSync));
    // 👈 显式处理初始化首轮调和周期的报错反馈
    if let Err(e) = run_reconcile_cycle(&client, &config_file).await {
        error!("首轮全局拓扑同步失败: {:?}", e);
        // 检查是否是致命错误，不是可重试错误
        if !matches!(e.downcast_ref::<ReconcileError>(), Some(ReconcileError::Transport(_))) {
            return Err(e);  // 快速失败
        }
    }

    let mut shutdown_signal = Box::pin(wait_for_shutdown_signal());

    loop {
        tokio::select! {
            Some(event) = rx.recv() => {
                match event {
                    TriggerEvent::FileChanged => info!("{}", tr(LogKey::ConfigChanged)),
                    TriggerEvent::PeriodicTick => info!("{}", tr(LogKey::PeriodicCheck)),
                }

                // 👈 核心修改：明确对每一轮事件循环的返回结果进行健康度归纳与捕获
                match run_reconcile_cycle(&client, &config_file).await {
                    Ok(outcome) => {
                        info!("本轮声明式调和圆满收敛成功！运行快照: {:?}", outcome);
                    }
                    Err(e) => {
                        error!("本轮声明式同步遭遇阶段性阻塞或技术回滚: {:?}", e);
                    }
                }
            }
            _ = &mut shutdown_signal => {
                break;
            }
        }
    }

    info!("{}", tr(LogKey::ServiceStopped));
    Ok(())
}

/// 核心修改：返回结构化的 ReconcileOutcome 报表结果
async fn run_reconcile_cycle(client: &Arc<IpfsClient>, config_file: &PathBuf) -> anyhow::Result<reconciler::ReconcileOutcome> {
    let desired = match load_desired_state(config_file) {
        Ok(d) => d,
        Err(e) => {
            error!(error = ?e, "{}", tr(LogKey::ConfigReadError));
            return Err(e);
        }
    };

    // Pre-flight 静态审查扩展：同时拦截冲突的端口和冲突的全局 P2P 协议主键
    let mut allocated_ports = HashSet::new();
    let mut allocated_protocols = HashSet::new();
    for tunnel in desired.values() {
        if tunnel.enabled {
            if !allocated_ports.insert(tunnel.port) {
                error!(port = tunnel.port, "{}", tr(LogKey::PortConflict));
                return Err(anyhow::anyhow!("Local port conflict"));
            }
            if !allocated_protocols.insert(tunnel.protocol.clone()) {
                error!(protocol = %tunnel.protocol, "致命配置错误 (Pre-flight 拦截): 发现了重复分配的全局 P2P 协议流主键！");
                return Err(anyhow::anyhow!("Duplicate global protocol identifier"));
            }
        }
    }

    let actual = match client.load_actual_state().await {
        Ok(a) => a,
        Err(e) => {
            error!(error = ?e, "{}", tr(LogKey::IpfsReadError));
            return Err(e.into());
        }
    };

    let outcome = reconcile_all(Arc::clone(client), desired, actual).await?;
    info!("{}", tr(LogKey::SyncComplete));

    Ok(outcome)
}