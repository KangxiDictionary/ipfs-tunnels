use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;
use tracing::{error, info, instrument};
use tracing_subscriber::{EnvFilter, FmtSubscriber};
use clap::Parser;

use ipfs_tunnels_manager::config::{ensure_config_exists, load_desired_state};
use ipfs_tunnels_manager::error::ReconcileError;
use ipfs_tunnels_manager::http_server::{self, AppCtx};
use ipfs_tunnels_manager::i18n::{self, tr, LogKey};
use ipfs_tunnels_manager::ipfs::IpfsClient;
use ipfs_tunnels_manager::reconciler::{self, reconcile_all};
use ipfs_tunnels_manager::signals::wait_for_shutdown_signal;

#[derive(Debug)]
enum TriggerEvent {
    FileChanged,
    PeriodicTick,
}

#[derive(Parser, Debug)]
#[command(name = "ipfs-tunnels-manager", version, about = "IPFS P2P Tunnel Port Forwarding Reconciler")]
struct Args {
    /// 配置文件 tunnels.conf 的路径（默认: ~/.config/ipfs-tunnels/tunnels.conf）
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// 同步状态的轮询间隔时间 (秒)
    #[arg(short, long, default_value_t = 30)]
    interval: u64,

    /// Prometheus Metrics 与健康检查服务的端口
    #[arg(long, default_value_t = 9000)]
    metrics_port: u16,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    i18n::init();

    // 1. 解析命令行参数
    let args = Args::parse();

    // 2. 初始化日志
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let subscriber = FmtSubscriber::builder().with_env_filter(filter).finish();
    tracing::subscriber::set_global_default(subscriber)?;

    // 3. 确定配置文件路径
    let config_file = args.config.clone().unwrap_or_else(|| {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ipfs-tunnels")
            .join("tunnels.conf")
    });

    // 4. 确保配置文件存在
    ensure_config_exists(&config_file)?;

    // 5. 创建 IPFS 客户端
    let client = Arc::new(IpfsClient::new());
    info!("{}", tr(LogKey::ServiceStarting));

    // 6. 验证 IPFS 节点可连接性
    if let Err(e) = client.load_actual_state().await {
        error!("IPFS Daemon 离线或联络失败，控制器拒绝初始化启动。错误原因: {:?}", e);
        return Err(anyhow::anyhow!("IPFS connection refused on startup"));
    }

    // 7. 🔴 修复：初始化 Prometheus Recorder 并获取 Handle 句柄
    let metrics_handle = init_metrics_recorder()?;

    let ipfs_connected = Arc::new(AtomicBool::new(true));

    // 注入句柄到上下文
    let ctx = AppCtx {
        ipfs_connected: ipfs_connected.clone(),
        metrics_handle,
    };
    http_server::start_server(args.metrics_port, ctx).await?;

    // 8. 设置文件变更监听
    let (tx, mut rx) = mpsc::channel(32);

    let tx_file = tx.clone();
    let config_file_clone = config_file.clone();
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
    watcher.watch(&config_file_clone, RecursiveMode::NonRecursive)?;

    // 9. 定时轮询
    let tx_tick = tx.clone();
    let interval_duration = Duration::from_secs(args.interval);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(interval_duration);
        loop {
            interval.tick().await;
            let _ = tx_tick.try_send(TriggerEvent::PeriodicTick);
        }
    });

    info!("{}", tr(LogKey::InitialSync));
    // 首轮同步
    if let Err(e) = run_reconcile_cycle(&client, &config_file, &ipfs_connected).await {
        error!("首轮全局拓扑同步失败: {:?}", e);
        // 检查是否是致命错误，不是可重试错误
        if !matches!(e.downcast_ref::<ReconcileError>(), Some(ReconcileError::Transport(_))) {
            return Err(e);
        }
    }

    let mut shutdown_signal = Box::pin(wait_for_shutdown_signal());

    // 10. 主事件循环
    loop {
        tokio::select! {
            Some(event) = rx.recv() => {
                match event {
                    TriggerEvent::FileChanged => info!("{}", tr(LogKey::ConfigChanged)),
                    TriggerEvent::PeriodicTick => info!("{}", tr(LogKey::PeriodicCheck)),
                }

                match run_reconcile_cycle(&client, &config_file, &ipfs_connected).await {
                    Ok(outcome) => {
                        info!(
                            created = outcome.created,
                            updated = outcome.updated,
                            deleted = outcome.deleted,
                            failed = outcome.failed,
                            partial_success = outcome.partial_success,
                            "本轮声明式调和完成: {}",
                            outcome
                        );
                    }
                    Err(e) => {
                        error!(error = ?e, "本轮声明式同步遭遇阶段性阻塞或技术回滚");
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

/// 🔴 修复：将原来的 `install_recorder()` 替换为 `install()`
/// 它除了注册全局监控组件外，还会返回我们需要渲染指标数据的 Handle
/// 🔴 修复：通过 .build() 同时获取 Recorder 和 Handle
/// 手动注册 Recorder 并返回 Handle 供 Axum 使用
fn init_metrics_recorder() -> anyhow::Result<metrics_exporter_prometheus::PrometheusHandle> {
    use metrics_exporter_prometheus::PrometheusBuilder;

    // 1. .build() 返回的是 (PrometheusRecorder, ExporterFuture)
    let (recorder, exporter_future) = PrometheusBuilder::new().build()?;

    // 2. 🎯 从 recorder 中提取出真正的 PrometheusHandle 句柄
    let handle = recorder.handle();

    // 3. 将 recorder 显式注册为全局指标收集器
    metrics::set_global_recorder(Box::new(recorder))
        .map_err(|e| anyhow::anyhow!("Failed to set global recorder: {:?}", e))?;

    // 4. 将底层的背景 Future 任务丢进 Tokio 异步运行时，让其在后台安全运行
    tokio::spawn(async move {
        if let Err(e) = exporter_future.await {
            tracing::error!("Prometheus background exporter error: {:?}", e);
        }
    });

    // 5. 成功返回真正类型匹配的 handle
    Ok(handle)
}

/// 核心调和周期
#[instrument(skip(client, config_file, ipfs_connected))]
async fn run_reconcile_cycle(
    client: &Arc<IpfsClient>,
    config_file: &PathBuf,
    ipfs_connected: &Arc<std::sync::atomic::AtomicBool>,
) -> anyhow::Result<reconciler::ReconcileOutcome> {
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
                error!(
                    protocol = %tunnel.protocol,
                    "{}",
                    tr(LogKey::ProtocolConflictAbort)
                );
                return Err(anyhow::anyhow!("Duplicate global protocol identifier"));
            }
        }
    }

    // 从 IPFS 加载实际状态并更新连接状态指示器
    let actual = match client.load_actual_state().await {
        Ok(a) => {
            ipfs_connected.store(true, Ordering::Relaxed);
            metrics::gauge!("ipfs_tunnels_online").set(1.0);
            a
        }
        Err(e) => {
            ipfs_connected.store(false, Ordering::Relaxed);
            metrics::gauge!("ipfs_tunnels_online").set(0.0);
            metrics::counter!("ipfs_tunnels_errors_total").increment(1);
            error!(error = ?e, "{}", tr(LogKey::IpfsReadError));
            return Err(e.into());
        }
    };

    let outcome = reconcile_all(Arc::clone(client), desired, actual).await?;

    Ok(outcome)
}