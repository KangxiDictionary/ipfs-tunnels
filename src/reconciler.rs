use crate::error::{ReconcileError, RollbackRecord};
use crate::ipfs::IpfsClient;
use crate::models::{ActualTunnel, DesiredTunnel, TunnelMode};
use crate::i18n::{tr, LogKey};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::fmt;
use tracing::{error, info, warn, instrument};

#[derive(Debug, Clone, Default)]
pub struct ReconcileOutcome {
    pub created: usize,
    pub updated: usize,
    pub deleted: usize,
    pub failed: usize,
    pub rollback_failures: Vec<RollbackRecord>,
    pub partial_success: bool,
}

// 🌟 P2 优化：实现 Display trait 便于日志输出
impl fmt::Display for ReconcileOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ReconcileOutcome {{ created: {}, updated: {}, deleted: {}, failed: {}, partial_success: {}, rollback_failures: {} }}",
            self.created, self.updated, self.deleted, self.failed, self.partial_success, self.rollback_failures.len()
        )
    }
}

fn format_local_multiaddr(ip: &std::net::IpAddr, port: u16) -> String {
    let proto_name = match ip {
        std::net::IpAddr::V4(_) => "ip4",
        std::net::IpAddr::V6(_) => "ip6",
    };
    format!("/{}{}/tcp/{}", proto_name, ip, port)
}

#[instrument(skip(client, tunnel))]
async fn apply_tunnel(client: &IpfsClient, tunnel: &DesiredTunnel) -> Result<(), ReconcileError> {
    let local_maddr = format_local_multiaddr(&tunnel.local_ip, tunnel.port);
    match tunnel.mode {
        TunnelMode::Client => {
            let target_maddr = crate::models::normalize_target(&tunnel.target);
            client.p2p_forward(&local_maddr, &tunnel.protocol, &target_maddr).await
        }
        TunnelMode::Server => {
            client.p2p_listen(&local_maddr, &tunnel.protocol).await
        }
    }
}

#[instrument(skip(client, tunnel))]
async fn teardown_tunnel(client: &IpfsClient, tunnel: &ActualTunnel) -> Result<(), ReconcileError> {
    client.p2p_close(&tunnel.protocol).await
}

pub async fn reconcile_all(
    client: Arc<IpfsClient>,
    desired: HashMap<String, DesiredTunnel>,
    actual: HashMap<String, ActualTunnel>,
) -> anyhow::Result<ReconcileOutcome> {
    let mut outcome = ReconcileOutcome::default();
    let mut failed_rollbacks = Vec::new();
    let mut successfully_applied = Vec::new();

    // 1. 将 desired 按 protocol 重新映射
    let mut desired_by_proto: HashMap<String, DesiredTunnel> = HashMap::new();
    for tunnel in desired.values() {
        if desired_by_proto.insert(tunnel.protocol.clone(), tunnel.clone()).is_some() {
            // 🌟 P0 修复：检测协议冲突并使用 i18n
            error!("{}", tr(LogKey::ProtocolConflictAbort));
            anyhow::bail!("Duplicate global protocol identifier detected in reconciler.");
        }
    }

    let mut all_protocols = HashSet::new();
    for proto in desired_by_proto.keys() {
        all_protocols.insert(proto.clone());
    }
    for proto in actual.keys() {
        all_protocols.insert(proto.clone());
    }

    // 2. 核心状态差分对齐
    for proto in all_protocols {
        let desired_opt = desired_by_proto.get(&proto);
        let actual_opt = actual.get(&proto);

        match (desired_opt, actual_opt) {
            (Some(desired_tunnel), None) => {
                if !desired_tunnel.enabled {
                    continue;
                }
                info!(protocol = %proto, name = %desired_tunnel.name, "{}", tr(LogKey::TunnelCreating));
                if let Err(e) = apply_tunnel(&client, desired_tunnel).await {
                    error!(protocol = %proto, name = %desired_tunnel.name, error = ?e, "{}", tr(LogKey::SyncFailed));
                    outcome.failed += 1;
                } else {
                    info!(protocol = %proto, name = %desired_tunnel.name, "{}", tr(LogKey::TunnelCreated));
                    outcome.created += 1;
                }
            }

            // ✨ 完美收拢在这里：当新期望和旧实例同时存在时
            (Some(desired_tunnel), Some(old_actual)) => {
                // A. 如果新配置要求禁用，直接拆除旧隧道
                if !desired_tunnel.enabled {
                    info!(protocol = %proto, name = %desired_tunnel.name, "{}", tr(LogKey::TunnelDisabling));
                    if let Err(e) = teardown_tunnel(&client, old_actual).await {
                        error!(protocol = %proto, name = %desired_tunnel.name, error = ?e, "{}", tr(LogKey::SyncFailed));
                        outcome.failed += 1;
                    } else {
                        info!(protocol = %proto, name = %desired_tunnel.name, "{}", tr(LogKey::TunnelDisabled));
                        outcome.deleted += 1;
                    }
                    continue;
                }

                // B. 🟡 优化点 1：使用 detect_drift 代替原来手写的 4 行繁琐 `if` 判断
                if let Some(reason) = desired_tunnel.detect_drift(old_actual) {
                    info!(protocol = %proto, name = %desired_tunnel.name, "检测到隧道发生状态漂移 ({:?})，正在触发重建...", reason);
                    info!(protocol = %proto, "{}", tr(LogKey::UpdateTeardown));

                    // 先拆除发生漂移的旧隧道
                    if let Err(e) = teardown_tunnel(&client, old_actual).await {
                        error!(protocol = %proto, error = ?e, "{}", tr(LogKey::TeardownFailed));
                        outcome.failed += 1;
                        continue;
                    }

                    // 尝试应用新隧道配置
                    if let Err(e) = apply_tunnel(&client, desired_tunnel).await {
                        error!(protocol = %proto, name = %desired_tunnel.name, error = ?e, "{}", tr(LogKey::ApplyFailedRollback));
                        outcome.failed += 1;

                        info!(protocol = %proto, "{}", tr(LogKey::TunnelRollbackAttempt));

                        // 🔴 优化点 2：修复高优先级回滚隐患！
                        // 直接根据 old_actual 的真实快照进行强行原样恢复，不再伪造带有 enabled: true 的临时 DesiredTunnel
                        if let Err(re) = rollback_to_actual(&client, &proto, old_actual).await {
                            error!(protocol = %proto, error = ?re, "{}", tr(LogKey::TunnelRollbackFailed));
                            failed_rollbacks.push(RollbackRecord {
                                protocol: proto.clone(),
                                desired_tunnel: desired_tunnel.clone(), // 保持记录引发异常地期望配置用于审计
                                rollback_err: format!("{:?}", re),
                            });
                        } else {
                            info!(protocol = %proto, "{}", tr(LogKey::RollbackSuccess));
                        }
                    } else {
                        info!(protocol = %proto, name = %desired_tunnel.name, "{}", tr(LogKey::TunnelUpdated));
                        outcome.updated += 1;
                        successfully_applied.push(proto.clone());
                    }
                }
            }

            (None, Some(old_actual)) => {
                info!(protocol = %proto, "{}", tr(LogKey::TunnelCleaning));
                if let Err(e) = teardown_tunnel(&client, old_actual).await {
                    error!(protocol = %proto, error = ?e, "{}", tr(LogKey::SyncFailed));
                    outcome.failed += 1;
                } else {
                    info!(protocol = %proto, "{}", tr(LogKey::TunnelCleaned));
                    outcome.deleted += 1;
                }
            }

            (None, None) => unreachable!(),
        }
    }

    if !failed_rollbacks.is_empty() {
        outcome.rollback_failures = failed_rollbacks.clone();
        outcome.partial_success = !successfully_applied.is_empty();

        error!(
            count = failed_rollbacks.len(),
            "{} (Count: {})",
            tr(LogKey::RollbackInconsistent),
            failed_rollbacks.len()
        );

        return if successfully_applied.is_empty() {
            Err(anyhow::anyhow!(ReconcileError::RollbackFailed(format!(
                "Fatal rollback failure on channels: {:?}",
                failed_rollbacks.iter().map(|r| &r.protocol).collect::<Vec<_>>()
            ))))
        } else {
            Err(anyhow::anyhow!(ReconcileError::PartialRollbackFailed {
                affected_count: failed_rollbacks.len(),
                records: failed_rollbacks,
            }))
        };
    }

    if outcome.failed > 0 && (outcome.created > 0 || outcome.updated > 0 || outcome.deleted > 0) {
        outcome.partial_success = true;
        warn!("{}", tr(LogKey::PartialSync));
    } else if outcome.failed == 0 {
        info!("{}", tr(LogKey::SyncComplete));
    }

    Ok(outcome)
}

async fn rollback_to_actual(
    client: &IpfsClient,
    protocol: &str,
    actual: &ActualTunnel,
) -> Result<(), ReconcileError> {
    let local_maddr = format_local_multiaddr(&actual.local_ip, actual.port);
    match actual.mode {
        TunnelMode::Client => {
            let target_maddr = crate::models::normalize_target(&actual.target);
            client.p2p_forward(&local_maddr, protocol, &target_maddr).await
        }
        TunnelMode::Server => {
            client.p2p_listen(&local_maddr, protocol).await
        }
    }
}