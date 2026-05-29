use tracing::warn;

/// 统一的跨平台信号等待拦截器
/// 在 Unix 上同时监听 SIGINT 和 SIGTERM；在 Windows 上监听 Ctrl+C (SIGINT)。
pub async fn wait_for_shutdown_signal() -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        let mut sigterm = signal(SignalKind::terminate())?;
        let sigint = tokio::signal::ctrl_c();
        tokio::pin!(sigint);

        tokio::select! {
            _ = sigterm.recv() => {
                warn!("🛑 接收到 SIGTERM 系统终止指令。执行 Operator 安全下线程序...");
            }
            _ = &mut sigint => {
                warn!("🛑 接收到 SIGINT (Ctrl+C) 终止信号。执行 Cancellation Safety 保护退出...");
            }
        }
    }

    #[cfg(windows)]
    {
        tokio::signal::ctrl_c().await?;
        warn!("🛑 接收到 Ctrl+C 终止信号。执行 Operator 安全下线程序...");
    }

    Ok(())
}
