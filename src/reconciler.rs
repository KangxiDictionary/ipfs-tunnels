use crate::error::{ReconcileError, RollbackRecord};
use crate::ipfs::IpfsClient;
use crate::models::{ActualTunnel, DesiredTunnel, TunnelMode};
use crate::i18n::{tr, get_lang, Lang, LogKey};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tracing::{error, info, warn};

/// 结构化调和周期反馈结果，便于对接 Metrics 或发送 Webhook 告警
#[derive(Debug, Clone, Default)]
pub struct ReconcileOutcome {
    pub created: usize,
    pub updated: usize,
    pub deleted: usize,
    pub failed: usize,
    pub rollback_failures: Vec<RollbackRecord>,
    pub partial_success: bool,
}

fn format_local_multiaddr(ip: &std::net::IpAddr, port: u16) -> String {
    let proto_name = match ip {
        std::net::IpAddr::V4(_) => "ip4",
        std::net::IpAddr::V6(_) => "ip6",
    };
    format!("/{}{}/tcp/{}", proto_name, ip, port)
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
) -> Result<ReconcileOutcome, anyhow::Error> {
    let mut outcome = ReconcileOutcome::default();

    let mut desired_by_proto: HashMap<String, DesiredTunnel> = HashMap::new();
    for tunnel in desired.values() {
        desired_by_proto.insert(tunnel.protocol.clone(), tunnel.clone());
    }

    let mut all_protocols = HashSet::new();
    all_protocols.extend(desired_by_proto.keys().cloned());
    all_protocols.extend(actual.keys().cloned());

    let mut teardown_tasks = Vec::new();
    let mut setup_tasks = Vec::new();

    enum TeardownReason {
        Disable,
        Clean,
        Update
    }

    for proto in all_protocols {
        match (desired_by_proto.get(&proto), actual.get(&proto)) {
            (Some(d), None) if d.enabled => {
                setup_tasks.push((d.clone(), None));
            }
            (Some(d), Some(a)) => {
                if d.enabled {
                    let is_drifted = d.mode != a.mode
                        || d.local_ip != a.local_ip
                        || d.port != a.port
                        || d.peer_id != a.peer_id;

                    if is_drifted {
                        teardown_tasks.push((proto.clone(), a.clone(), TeardownReason::Update));
                        setup_tasks.push((d.clone(), Some(a.clone())));
                    }
                } else {
                    teardown_tasks.push((proto.clone(), a.clone(), TeardownReason::Disable));
                }
            }
            (None, Some(a)) => {
                teardown_tasks.push((proto.clone(), a.clone(), TeardownReason::Clean));
            }
            _ => {}
        }
    }

    // ==========================================
    // 阶段一：严格顺序解构
    // ==========================================
    for (proto, _actual_tunnel, reason) in teardown_tasks {
        match reason {
            TeardownReason::Update => {
                info!(protocol = %proto, "检测到隧道配置发生变更，正在安全下线旧协议流...");
            }
            TeardownReason::Disable => {
                info!(protocol = %proto, "{}", tr(LogKey::TunnelDisabling));
            }
            TeardownReason::Clean => {
                warn!(protocol = %proto, "{}", tr(LogKey::TunnelCleaning));
            }
        }

        if let Err(e) = client.execute_with_retry(|| client.p2p_close(&proto)).await {
            error!(protocol = %proto, error = ?e, "解构下线旧隧道流失败，终止后续创建步骤以防拓扑受损！");
            outcome.failed += 1;
            anyhow::bail!("Phase-1 Teardown failed, breaking cycle sequence.");
        }

        match reason {
            TeardownReason::Disable => info!(protocol = %proto, "{}", tr(LogKey::TunnelDisabled)),
            TeardownReason::Clean => {
                info!(protocol = %proto, "{}", tr(LogKey::TunnelCleaned));
                outcome.deleted += 1;
            }
            TeardownReason::Update => {}
        }
    }

    // ==========================================
    // 阶段二：安全顺序构建与精准原子回滚
    // ==========================================
    let mut successfully_applied = Vec::new();
    let mut failed_rollbacks = Vec::new();

    for (desired_tunnel, rollback_ctx) in setup_tasks {
        let is_update = rollback_ctx.is_some();
        if is_update {
            info!(name = %desired_tunnel.name, "{}", tr(LogKey::TunnelUpdating));
        } else {
            info!(name = %desired_tunnel.name, "{}", tr(LogKey::TunnelCreating));
        }

        match client.execute_with_retry(|| apply_tunnel(&client, &desired_tunnel)).await {
            Ok(_) => {
                successfully_applied.push(desired_tunnel.clone());

                if is_update {
                    info!(name = %desired_tunnel.name, "{}", tr(LogKey::TunnelUpdated));
                    outcome.updated += 1;
                } else {
                    info!(name = %desired_tunnel.name, "{}", tr(LogKey::TunnelCreated));
                    outcome.created += 1;
                }
            }
            Err(create_err) => {
                error!(name = %desired_tunnel.name, error = ?create_err, "配置项下发失败！尝试激活事务性安全回滚引擎...");
                outcome.failed += 1;

                if let Some(old_actual) = rollback_ctx {
                    warn!(name = %desired_tunnel.name, "{}", tr(LogKey::TunnelRollbackAttempt));

                    let rollback_desired = DesiredTunnel {
                        name: desired_tunnel.name.clone(),
                        mode: old_actual.mode,
                        local_ip: old_actual.local_ip,
                        port: old_actual.port,
                        peer_id: old_actual.peer_id.clone(),
                        protocol: old_actual.protocol.clone(),
                        enabled: true,
                    };

                    match client.execute_with_retry(|| apply_tunnel(&client, &rollback_desired)).await {
                        Ok(_) => {
                            info!(name = %desired_tunnel.name, "回滚旧配置成功，隧道已恢复到上一个稳定状态");
                        }
                        Err(rollback_err) => {
                            error!(
                                name = %desired_tunnel.name,
                                protocol = %desired_tunnel.protocol,
                                error = ?rollback_err,
                                "{}",
                                tr(LogKey::TunnelRollbackFailed)
                            );

                            failed_rollbacks.push(RollbackRecord {
                                protocol: desired_tunnel.protocol.clone(),
                                desired_tunnel: rollback_desired,
                                rollback_err: rollback_err.to_string(),
                            });
                        }
                    }
                }
            }
        }
    }

    // ==========================================
    // 阶段三：清理、诊断与状态汇总
    // ==========================================

    if !failed_rollbacks.is_empty() {
        outcome.partial_success = true;
        outcome.rollback_failures = failed_rollbacks.clone();

        error!(
            count = failed_rollbacks.len(),
            "发现 {} 个隧道的回滚操作失败，系统可能处于不一致状态！",
            failed_rollbacks.len()
        );

        if !successfully_applied.is_empty() {
            warn!(
                success_count = successfully_applied.len(),
                failed_count = outcome.failed,
                "本轮调和出现部分成功：成功应用 {} 个，失败 {} 个",
                successfully_applied.len(),
                outcome.failed
            );
        }
    }

    // ✅ 修复：移除未使用的 _err_msg 变量
    if outcome.failed > 0 && successfully_applied.is_empty() {
        let err_msg = match get_lang() {
            Lang::Zh => format!("本轮同步未完全完成，共有 {} 个错误，且无隧道成功应用", outcome.failed),
            Lang::En => format!("Reconciliation cycle failed, total {} errors detected, no tunnel successfully applied", outcome.failed),
        };

        tracing::debug!("Reconciliation error message: {}", err_msg);

        return Err(anyhow::anyhow!(ReconcileError::PartialRollbackFailed {
            affected_count: failed_rollbacks.len(),
            records: failed_rollbacks,
        }));
    }

    if !failed_rollbacks.is_empty() && !successfully_applied.is_empty() {
        return Err(anyhow::anyhow!(ReconcileError::PartialRollbackFailed {
            affected_count: failed_rollbacks.len(),
            records: failed_rollbacks,
        }));
    }

    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn test_format_local_multiaddr_ipv4() {
        let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let result = format_local_multiaddr(&ip, 8080);
        assert_eq!(result, "/ip4/127.0.0.1/tcp/8080");
    }

    #[test]
    fn test_reconcile_outcome_default() {
        let outcome = ReconcileOutcome::default();
        assert_eq!(outcome.created, 0);
        assert_eq!(outcome.updated, 0);
        assert_eq!(outcome.deleted, 0);
        assert_eq!(outcome.failed, 0);
        assert_eq!(outcome.rollback_failures.len(), 0);
        assert!(!outcome.partial_success);
    }

    #[test]
    fn test_reconcile_outcome_clone() {
        let outcome1 = ReconcileOutcome {
            created: 1,
            updated: 2,
            deleted: 3,
            failed: 0,
            rollback_failures: Vec::new(),
            partial_success: false,
        };

        let outcome2 = outcome1.clone();
        assert_eq!(outcome1.created, outcome2.created);
        assert_eq!(outcome1.updated, outcome2.updated);
    }
}