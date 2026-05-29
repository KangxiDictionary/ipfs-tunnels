use crate::error::ReconcileError;
use crate::ipfs::IpfsClient;
use crate::models::{ActualTunnel, DesiredTunnel, TunnelMode};
use futures::stream::{self, StreamExt};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tracing::{error, info, warn};

// fn make_ipfs_multiaddr(ip: &std::net::IpAddr, port: u16, peer_id: &str) -> (String, String) {
//     let proto_name = match ip {
//         std::net::IpAddr::V4(_) => "ip4",
//         std::net::IpAddr::V6(_) => "ip6",
//     };
//     (
//         format!("/{}/{}/tcp/{}", proto_name, ip, port),
//         format!("/p2p/{}", peer_id),
//     )
// }

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
            // Server 不需要 peer_id，它将网络协议流量引入到本地 multiaddr
            client.p2p_listen(&local_maddr, &tunnel.protocol).await
        }
    }
}

// 解决问题 1：重构为返回 Result，允许外部监控本轮调和是否存在部分失败
pub async fn reconcile_all(
    client: Arc<IpfsClient>,
    desired: HashMap<String, DesiredTunnel>,
    actual: HashMap<String, ActualTunnel>,
) -> Result<(), anyhow::Error> {
    let protocols: HashSet<String> = desired.keys().chain(actual.keys()).cloned().collect();

    // 使用原子计数器或结果收集器收集多并发中的子错误
    let results: Vec<Result<(), ReconcileError>> = stream::iter(protocols)
        .map(|proto| {
            let desired = desired.get(&proto).cloned();
            let actual = actual.get(&proto).cloned();
            let client = client.clone();

            async move {
                let _span = tracing::info_span!("reconcile_loop", protocol = %proto).entered();
                match (desired, actual) {
                    (Some(d), None) if d.enabled => {
                        info!(tunnel_name = %d.name, mode = ?d.mode, "发现未就绪隧道，正在创建...");
                        client.execute_with_retry(|| apply_tunnel(&client, &d)).await?; // 👈 使用统一 apply 代理
                        info!("隧道成功挂载");
                    }

                    (Some(d), Some(a)) if d.enabled
                        && (d.mode != a.mode || d.local_ip != a.local_ip || d.port != a.port || d.peer_id != a.peer_id) => {
                            warn!(tunnel_name = %d.name, "⚠️ 检测到配置存在漂移！启动事务性更新...");

                            client.execute_with_retry(|| client.p2p_close(&d.protocol)).await?;

                            if let Err(create_err) = client.execute_with_retry(|| apply_tunnel(&client, &d)).await {
                                error!(error = %create_err, "新配置状态激活失败！触发紧急事务回滚...");

                                // 回滚：这里为了保持严谨，我们需要把 ActualTunnel 临时转为伪 Desired 格式去调用 apply
                                let rollback_desired = DesiredTunnel {
                                    name: d.name.clone(), mode: a.mode, local_ip: a.local_ip,
                                    port: a.port, peer_id: a.peer_id, protocol: a.protocol.clone(), enabled: true,
                                };

                                if let Err(rollback_err) = client.execute_with_retry(|| apply_tunnel(&client, &rollback_desired)).await {
                                    error!(critical_err = %rollback_err, "🚨 回滚失败！隧道处于悬挂损毁状态！");
                                    return Err(ReconcileError::RollbackFailed(rollback_err.to_string()));
                                }
                                return Err(create_err);
                            }
                            info!("事务性更新成功提交，漂移已修复");
                        }

                    // 情况 3：配置中显式关闭 (enabled == false)，但现网还在运行 -> 予以关闭
                    (Some(d), Some(a)) if !d.enabled => {
                        warn!(tunnel_name = %d.name, "🔒 隧道在配置中被显式禁用，正在执行安全下线...");
                        client.execute_with_retry(|| client.p2p_close(&a.protocol)).await?;
                        info!("禁用隧道下线成功");
                    }

                    // 情况 4：配置被完全删除，但现网还在运行 -> 残留清理
                    (None, Some(a)) => {
                        warn!("发现陈旧过时残留隧道，正在下线收容...");
                        client.execute_with_retry(|| client.p2p_close(&a.protocol)).await?;
                        info!("陈旧隧道清理完毕");
                    }

                    // 边界情况 5：配置中存在且显式关闭，实际也没有运行 -> 完美符合终态，无视
                    // 边界情况 6：两边都没有 -> 无视
                    _ => {}
                }
                Ok(())
            }
        })
        .buffer_unordered(10)
        .collect()
        .await;

    // 检查是否有任何资源在调和中抛出了致命异常
    let mut err_count = 0;
    for res in results {
        if let Err(e) = res {
            error!(error = %e, "子资源调和失败");
            err_count += 1;
        }
    }

    if err_count > 0 {
        anyhow::bail!("本轮状态调和未完全收敛，存在 {} 个资源异常", err_count);
    }
    Ok(())
}
