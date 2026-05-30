#[cfg(test)]
mod reconciler_tests {
    use ipfs_tunnels_manager::error::ReconcileError;
    use ipfs_tunnels_manager::models::{ActualTunnel, DesiredTunnel, TunnelMode};
    use ipfs_tunnels_manager::reconciler::ReconcileOutcome;
    use std::collections::HashMap;
    use std::net::{IpAddr, Ipv4Addr};

    // ==========================================
    // 辅助函数
    // ==========================================
    fn create_desired_tunnel(
        name: &str,
        protocol: &str,
        mode: TunnelMode,
        enabled: bool,
    ) -> DesiredTunnel {
        DesiredTunnel {
            name: name.to_string(),
            mode,
            local_ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port: 25565,
            peer_id: "Qm123456...".to_string(),
            protocol: protocol.to_string(),
            enabled,
        }
    }

    fn create_actual_tunnel(protocol: &str, mode: TunnelMode) -> ActualTunnel {
        ActualTunnel {
            mode,
            local_ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port: 25565,
            peer_id: "Qm123456...".to_string(),
            protocol: protocol.to_string(),
        }
    }

    // ==========================================
    // 单元测试：ReconcileOutcome
    // ==========================================
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
    fn test_reconcile_outcome_modification() {
        // ✅ Clippy 建议：直接用初始化器而不是先 default() 再修改
        let outcome = ReconcileOutcome {
            created: 5,
            updated: 3,
            failed: 1,
            partial_success: true,
            ..Default::default()
        };

        assert_eq!(outcome.created, 5);
        assert_eq!(outcome.updated, 3);
        assert_eq!(outcome.failed, 1);
        assert!(outcome.partial_success);
    }

    // ==========================================
    // 单元测试：隧道创建和比较
    // ==========================================
    #[test]
    fn test_desired_tunnel_equality() {
        let t1 = create_desired_tunnel("test", "/x/test", TunnelMode::Client, true);
        let t2 = create_desired_tunnel("test", "/x/test", TunnelMode::Client, true);

        assert_eq!(t1, t2);
    }

    #[test]
    fn test_tunnel_drift_detection_mode() {
        let desired = create_desired_tunnel("ssh", "/x/ssh", TunnelMode::Client, true);
        let actual = create_actual_tunnel("/x/ssh", TunnelMode::Server);

        let is_drifted = desired.mode != actual.mode;
        assert!(is_drifted);
    }

    #[test]
    fn test_tunnel_drift_detection_port() {
        let mut desired = create_desired_tunnel("ssh", "/x/ssh", TunnelMode::Client, true);
        desired.port = 2222;

        let actual = create_actual_tunnel("/x/ssh", TunnelMode::Client);

        let is_drifted = desired.port != actual.port;
        assert!(is_drifted);
    }

    #[test]
    fn test_tunnel_drift_detection_ip() {
        let mut desired = create_desired_tunnel("ssh", "/x/ssh", TunnelMode::Client, true);
        desired.local_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));

        let actual = create_actual_tunnel("/x/ssh", TunnelMode::Client);

        let is_drifted = desired.local_ip != actual.local_ip;
        assert!(is_drifted);
    }

    #[test]
    fn test_tunnel_no_drift() {
        let desired = create_desired_tunnel("ssh", "/x/ssh", TunnelMode::Client, true);
        let actual = create_actual_tunnel("/x/ssh", TunnelMode::Client);

        let is_changed = desired.mode != actual.mode
            || desired.local_ip != actual.local_ip
            || desired.port != actual.port;

        assert!(!is_changed);
    }

    // ==========================================
    // 错误处理测试
    // ==========================================
    #[test]
    fn test_reconcile_error_transport_is_retryable() {
        let rejected = ReconcileError::Rejected("test".to_string());
        assert!(!rejected.is_retryable());

        let unavailable = ReconcileError::Unavailable("test".to_string());
        assert!(!unavailable.is_retryable());

        let rollback_failed = ReconcileError::RollbackFailed("test".to_string());
        assert!(!rollback_failed.is_retryable());
    }

    #[test]
    fn test_partial_rollback_failed_error() {
        let err = ReconcileError::PartialRollbackFailed {
            affected_count: 2,
            records: Vec::new(),
        };

        assert!(!err.is_retryable());
        let err_str = err.to_string();
        assert!(err_str.contains("部分隧道同步成功"));
    }

    // ==========================================
    // 场景测试：隧道状态转换逻辑
    // ==========================================
    #[test]
    fn test_scenario_new_tunnel_creation() {
        // 期望: minecraft 隧道，实际: 不存在
        let desired = create_desired_tunnel("minecraft", "/x/minecraft", TunnelMode::Client, true);
        let actual: HashMap<String, ActualTunnel> = HashMap::new();

        // ✅ 修复：使用实际变量而不是每次创建新的空 HashMap
        let should_create = desired.enabled && !actual.contains_key("/x/minecraft");

        assert!(should_create);
    }

    #[test]
    fn test_scenario_tunnel_disabled() {
        // 期望: 禁用，实际: 存在
        let desired = create_desired_tunnel("old-tunnel", "/x/old", TunnelMode::Client, false);
        let actual_exists = true;

        // 模拟逻辑：应该关闭
        let should_close = !desired.enabled && actual_exists;
        assert!(should_close);
    }

    #[test]
    fn test_scenario_tunnel_orphan() {
        // 期望: 不存在，实际: 存在（孤儿）
        let desired_exists = false;
        let actual_exists = true;

        // 模拟逻辑：应该清理
        let should_cleanup = !desired_exists && actual_exists;
        assert!(should_cleanup);
    }

    #[test]
    fn test_scenario_tunnel_no_change() {
        // 期望和实际都相同
        let desired = create_desired_tunnel("stable", "/x/stable", TunnelMode::Server, true);
        let actual = create_actual_tunnel("/x/stable", TunnelMode::Server);

        let is_changed = desired.mode != actual.mode
            || desired.local_ip != actual.local_ip
            || desired.port != actual.port;

        assert!(!is_changed);
    }

    // ==========================================
    // 集成场景测试
    // ==========================================
    #[test]
    fn test_multiple_tunnels_mixed_operations() {
        // 场景: 3 个隧道，不同的操作
        let tunnel_create = create_desired_tunnel("new_tunnel", "/x/new", TunnelMode::Client, true);
        let tunnel_disable = create_desired_tunnel("old_tunnel", "/x/old", TunnelMode::Client, false);
        let tunnel_stable = create_desired_tunnel("stable", "/x/stable", TunnelMode::Server, true);

        assert!(tunnel_create.enabled);
        assert!(!tunnel_disable.enabled);
        assert!(tunnel_stable.enabled);
    }

    #[test]
    fn test_tunnel_mode_variants() {
        let client = TunnelMode::Client;
        let server = TunnelMode::Server;

        assert_eq!(client, TunnelMode::Client);
        assert_eq!(server, TunnelMode::Server);
        assert_ne!(client, server);
    }

    #[test]
    fn test_rollback_record_structure() {
        let tunnel = create_desired_tunnel("test", "/x/test", TunnelMode::Client, true);
        let record = ipfs_tunnels_manager::error::RollbackRecord {
            protocol: "/x/test".to_string(),
            desired_tunnel: tunnel,
            rollback_err: "Mock error".to_string(),
        };

        assert_eq!(record.protocol, "/x/test");
        assert_eq!(record.rollback_err, "Mock error");
    }

    // ==========================================
    // HashMap 状态检查测试
    // ==========================================
    #[test]
    fn test_actual_state_presence_check() {
        // ✅ 正确的方式：显式指定 HashMap 的类型
        let mut actual: HashMap<String, ActualTunnel> = HashMap::new();

        // 初始状态：为空
        assert!(!actual.contains_key("/x/minecraft"));

        // 添加隧道
        actual.insert(
            "/x/minecraft".to_string(),
            create_actual_tunnel("/x/minecraft", TunnelMode::Client),
        );

        // 现在存在
        assert!(actual.contains_key("/x/minecraft"));
    }

    #[test]
    fn test_desired_by_proto_mapping() {
        // ✅ 演示如何构建 desired_by_proto 映射
        let mut desired: HashMap<String, DesiredTunnel> = HashMap::new();
        desired.insert(
            "minecraft".to_string(),
            create_desired_tunnel("minecraft", "/x/minecraft", TunnelMode::Client, true),
        );

        // 翻转映射：从 name 到 protocol
        let mut desired_by_proto: HashMap<String, DesiredTunnel> = HashMap::new();
        for tunnel in desired.values() {
            desired_by_proto.insert(tunnel.protocol.clone(), tunnel.clone());
        }

        assert!(desired_by_proto.contains_key("/x/minecraft"));
        assert_eq!(desired_by_proto.len(), 1);
    }

    #[test]
    fn test_tunnel_comparison_with_hashmap() {
        // ✅ 演示如何检查隧道是否存在及是否漂移
        let mut desired_by_proto: HashMap<String, DesiredTunnel> = HashMap::new();
        let desired = create_desired_tunnel("ssh", "/x/ssh", TunnelMode::Client, true);
        desired_by_proto.insert("/x/ssh".to_string(), desired.clone());

        let mut actual: HashMap<String, ActualTunnel> = HashMap::new();
        let actual_tunnel = create_actual_tunnel("/x/ssh", TunnelMode::Client);
        actual.insert("/x/ssh".to_string(), actual_tunnel.clone());

        // 检查是否存在
        let d = desired_by_proto.get("/x/ssh").unwrap();
        let a = actual.get("/x/ssh").unwrap();

        // 检查是否漂移
        let is_drifted = d.mode != a.mode
            || d.local_ip != a.local_ip
            || d.port != a.port
            || d.peer_id != a.peer_id;

        assert!(!is_drifted);
    }
}
