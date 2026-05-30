use tracing::info;
use crate::i18n::{tr, LogKey};

pub async fn wait_for_shutdown_signal() -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        let mut sigterm = signal(SignalKind::terminate())?;
        let sigint = tokio::signal::ctrl_c();
        tokio::pin!(sigint);

        tokio::select! {
            _ = sigterm.recv() => {
                info!("{}", tr(LogKey::SigtermReceived));
            }
            _ = &mut sigint => {
                info!("{}", tr(LogKey::SigintReceived));
            }
        }
    }

    #[cfg(windows)]
    {
        tokio::signal::ctrl_c().await?;
        info!("{}", tr(LogKey::SigintReceived));
    }

    Ok(())
}
