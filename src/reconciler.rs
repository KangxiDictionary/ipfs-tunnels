use crate::error::ReconcileError;
use crate::ipfs::IpfsClient;
use crate::models::{ActualTunnel, DesiredTunnel, TunnelMode};
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
                        // 👈 统一命名规范，突出操作动作
                        info!(name = %d.name, "发现新配置，正在创建隧道...");
                        client.execute_with_retry(|| apply_tunnel(&client, &d)).await?;
                        info!(name = %d.name, "隧道创建成功");
                    }

                    (Some(d), Some(a)) if d.enabled
                        && (d.mode != a.mode || d.local_ip != a.local_ip || d.port != a.port || d.peer_id != a.peer_id) => {
                            // 👈 去掉“配置漂移”、“事务更新”，改用普通开发都懂的“状态不一致”、“更新/回滚”
                            warn!(name = %d.name, "检测到实际状态与配置不符，正在更新隧道...");

                            client.execute_with_retry(|| client.p2p_close(&d.protocol)).await?;

                            if let Err(create_err) = client.execute_with_retry(|| apply_tunnel(&client, &d)).await {
                                error!(name = %d.name, error = %create_err, "新配置应用失败，正在尝试回滚旧配置...");

                                let rollback_desired = DesiredTunnel {
                                    name: d.name.clone(), mode: a.mode, local_ip: a.local_ip,
                                    port: a.port, peer_id: a.peer_id, protocol: a.protocol.clone(), enabled: true,
                                };

                                if let Err(rollback_err) = client.execute_with_retry(|| apply_tunnel(&client, &rollback_desired)).await {
                                    error!(name = %d.name, fatal_err = %rollback_err, "致命错误：旧配置回滚失败！隧道当前状态可能损坏！");
                                    return Err(ReconcileError::RollbackFailed(rollback_err.to_string()));
                                }
                                return Err(create_err);
                            }
                            info!(name = %d.name, "旧隧道更新成功");
                        }

                    (Some(d), Some(a)) if !d.enabled => {
                        // 👈 拒绝“显式禁用”、“安全下线”，直接说关闭
                        info!(name = %d.name, "隧道已在配置中禁用，正在关闭...");
                        client.execute_with_retry(|| client.p2p_close(&a.protocol)).await?;
                        info!(name = %d.name, "解绑关闭成功");
                    }

                    (None, Some(a)) => {
                        // 👈 拒绝“陈旧残留过时”，用“未定义”更直白
                        warn!("发现配置中未定义的残留隧道，正在清理...");
                        client.execute_with_retry(|| client.p2p_close(&a.protocol)).await?;
                        info!("残留隧道清理完成");
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
            error!(error = %e, "隧道处理失败");
            err_count += 1;
        }
    }

    if err_count > 0 {
        anyhow::bail!("本轮同步未完全完成，共有 {} 个错误", err_count);
    }
    Ok(())
}
