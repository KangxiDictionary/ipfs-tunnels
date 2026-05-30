use crate::error::ReconcileError;
use crate::ipfs::IpfsClient;
use crate::models::{ActualTunnel, DesiredTunnel, TunnelMode};
use crate::i18n::{tr, get_lang, Lang, LogKey};

use futures::stream::{self, StreamExt};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tracing::{error, info, warn};

fn format_local_multiaddr(ip: &std::net::IpAddr, port: u16) -> String {
    let proto_name = match ip {
        std::net::IpAddr::V4(_) => "ip4",
        std::net::IpAddr::V6(_) => "ip6",
    };
    format!("/{}/{}/tcp/{}", proto_name, ip, port)
}

async fn apply_tunnel(client: &IpfsClient, tunnel: &DesiredTunnel) -> Result<(), ReconcileError> {
    let local_maddr = format_local_multiaddr(&tunnel.local_ip, tunnel.port);
    match tunnel.mode {
        TunnelMode::Client => {
            let target_maddr = format!("/p2p/{}", tunnel.peer_id);
            client.p2p_forward(&local_maddr, &target_maddr, &tunnel.protocol).await
        }
        TunnelMode::Server => {
            client.p2p_listen(&local_maddr, &tunnel.protocol).await
        }
    }
}

pub async fn reconcile_all(
    client: Arc<IpfsClient>,
    desired: HashMap<String, DesiredTunnel>,
    actual: HashMap<String, ActualTunnel>,
) -> Result<(), anyhow::Error> {
    let protocols: HashSet<String> = desired.keys().chain(actual.keys()).cloned().collect();

    let results: Vec<Result<(), ReconcileError>> = stream::iter(protocols)
        .map(|proto| {
            let desired = desired.get(&proto).cloned();
            let actual = actual.get(&proto).cloned();
            let client = client.clone();

            async move {
                let _span = tracing::info_span!("tunnel", protocol = %proto).entered();
                match (desired, actual) {
                    (Some(d), None) if d.enabled => {
                        info!(name = %d.name, "{}", tr(LogKey::TunnelCreating));
                        client.execute_with_retry(|| apply_tunnel(&client, &d)).await?;
                        info!(name = %d.name, "{}", tr(LogKey::TunnelCreated));
                    }

                    (Some(d), Some(a)) if d.enabled
                        && (d.mode != a.mode || d.local_ip != a.local_ip || d.port != a.port || d.peer_id != a.peer_id) => {
                            warn!(name = %d.name, "{}", tr(LogKey::TunnelUpdating));

                            client.execute_with_retry(|| client.p2p_close(&d.protocol)).await?;

                            if let Err(create_err) = client.execute_with_retry(|| apply_tunnel(&client, &d)).await {
                                error!(name = %d.name, error = %create_err, "{}", tr(LogKey::TunnelRollbackAttempt));

                                let rollback_desired = DesiredTunnel {
                                    name: d.name.clone(), mode: a.mode, local_ip: a.local_ip,
                                    port: a.port, peer_id: a.peer_id, protocol: a.protocol.clone(), enabled: true,
                                };

                                if let Err(rollback_err) = client.execute_with_retry(|| apply_tunnel(&client, &rollback_desired)).await {
                                    error!(name = %d.name, fatal_err = %rollback_err, "{}", tr(LogKey::TunnelRollbackFailed));
                                    return Err(ReconcileError::RollbackFailed(rollback_err.to_string()));
                                }
                                return Err(create_err);
                            }
                            info!(name = %d.name, "{}", tr(LogKey::TunnelUpdated));
                        }

                    (Some(d), Some(a)) if !d.enabled => {
                        info!(name = %d.name, "{}", tr(LogKey::TunnelDisabling));
                        client.execute_with_retry(|| client.p2p_close(&a.protocol)).await?;
                        info!(name = %d.name, "{}", tr(LogKey::TunnelDisabled));
                    }

                    (None, Some(a)) => {
                        warn!("{}", tr(LogKey::TunnelCleaning));
                        client.execute_with_retry(|| client.p2p_close(&a.protocol)).await?;
                        info!("{}", tr(LogKey::TunnelCleaned));
                    }

                    _ => {}
                }
                Ok(())
            }
        })
        .buffer_unordered(10)
        .collect()
        .await;

    let mut err_count = 0;
    for res in results {
        if let Err(e) = res {
            error!(error = %e, "{}", tr(LogKey::SyncFailed));
            err_count += 1;
        }
    }

    if err_count > 0 {
        // 对于最终抛出返回给运行环境的系统级异常，采用匹配实时包装错误
        let err_msg = match get_lang() {
            Lang::Zh => format!("本轮同步未完全完成，共有 {} 个错误", err_count),
            Lang::En => format!("Reconciliation cycle incomplete, total {} errors detected", err_count),
        };
        anyhow::bail!(err_msg);
    }
    Ok(())
}
