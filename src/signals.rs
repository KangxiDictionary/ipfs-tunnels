use tracing::info;

pub async fn wait_for_shutdown_signal() -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        let mut sigterm = signal(SignalKind::terminate())?;
        let sigint = tokio::signal::ctrl_c();
        tokio::pin!(sigint);

        tokio::select! {
            _ = sigterm.recv() => {
                info!("接收到 SIGTERM 信号，正在准备安全退出...");
            }
            _ = &mut sigint => {
                info!("接收到退出指令 (Ctrl+C)，正在准备安全退出...");
            }
        }
    }

    #[cfg(windows)]
    {
        tokio::signal::ctrl_c().await?;
        info!("接收到退出指令 (Ctrl+C)，正在准备安全退出...");
    }

    Ok(())
}
