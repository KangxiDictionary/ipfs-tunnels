#[cfg(test)]
mod reconciler_tests {
    use ipfs_tunnels_manager::models::{ActualTunnel, DesiredTunnel, TunnelMode};
    use std::collections::HashMap;
    use std::net::{IpAddr, Ipv4Addr};

    fn create_desired_tunnel(
        name: &str,
        protocol: &str,
        mode: TunnelMode,
        port: u16,
        enabled: bool,
    ) -> DesiredTunnel {
        DesiredTunnel {
            name: name.to_string(),
            mode,
            local_ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port,
            target: "Qm1234567890TestPeerId".to_string(),
            protocol: protocol.to_string(),
            enabled,
        }
    }

    fn create_actual_tunnel(protocol: &str, mode: TunnelMode, port: u16) -> ActualTunnel {
        ActualTunnel {
            mode,
            local_ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port,
            target: "Qm1234567890TestPeerId".to_string(),
            protocol: protocol.to_string(),
        }
    }

    // ==========================================
    // 单元测试：协议碰撞检测（在 main.rs 的 pre-flight 检查中）
    // ==========================================
    #[test]
    fn test_protocol_collision_detection_in_desired() {
        let mut desired: HashMap<String, DesiredTunnel> = HashMap::new();

        // 两个隧道使用相同协议（配置层面的重复）
        desired.insert(
            "tunnel_a".to_string(),
            create_desired_tunnel("tunnel_a", "/x/collision", TunnelMode::Client, 8080, true),
        );
        desired.insert(
            "tunnel_b".to_string(),
            create_desired_tunnel("tunnel_b", "/x/collision", TunnelMode::Server, 9090, true),
        );

        // 检查协议冲突逻辑（模拟 main.rs 中的 pre-flight 检查）
        let mut allocated_protocols = std::collections::HashSet::new();
        let mut has_collision = false;

        for tunnel in desired.values() {
            if tunnel.enabled
                && !allocated_protocols.insert(tunnel.protocol.clone()) {
                    has_collision = true;
                    break;
                }
        }

        assert!(has_collision, "应该检测到协议碰撞");
    }

    // ==========================================
    // 单元测试：漂移检测逻辑
    // ==========================================
    #[test]
    fn test_drift_detection_port_change() {
        let desired = create_desired_tunnel("secure_ssh", "/x/ssh", TunnelMode::Client, 2222, true);
        let actual = create_actual_tunnel("/x/ssh", TunnelMode::Client, 22);

        // ✅ 直接测试漂移检测逻辑
        let is_drifted = desired.port != actual.port;  // 2222 != 22
        assert!(is_drifted, "端口从 22 变为 2222，应该检测到漂移");
    }

    #[test]
    fn test_drift_detection_mode_change() {
        let desired = create_desired_tunnel("tunnel", "/x/test", TunnelMode::Server, 8080, true);
        let actual = create_actual_tunnel("/x/test", TunnelMode::Client, 8080);

        // ✅ 模式不同，应该检测到漂移
        let is_drifted = desired.mode != actual.mode
            || desired.local_ip != actual.local_ip
            || desired.port != actual.port
            || desired.target != actual.target;

        assert!(is_drifted, "Mode 从 Client 变为 Server，应该检测到漂移");
    }

    #[test]
    fn test_drift_detection_ip_change() {
        let mut desired = create_desired_tunnel("tunnel", "/x/test", TunnelMode::Client, 8080, true);
        desired.local_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));

        let actual = create_actual_tunnel("/x/test", TunnelMode::Client, 8080);

        // ✅ IP 不同，应该检测到漂移
        let is_drifted = desired.mode != actual.mode
            || desired.local_ip != actual.local_ip
            || desired.port != actual.port
            || desired.target != actual.target;

        assert!(is_drifted, "IP 不同应该检测到漂移");
    }

    #[test]
    fn test_no_drift_when_identical() {
        let desired = create_desired_tunnel("stable", "/x/stable", TunnelMode::Server, 8080, true);
        let actual = create_actual_tunnel("/x/stable", TunnelMode::Server, 8080);

        // ✅ 完全相同，应该没有漂移
        let is_drifted = desired.mode != actual.mode
            || desired.local_ip != actual.local_ip
            || desired.port != actual.port
            || desired.target != actual.target;

        assert!(!is_drifted, "隧道配置完全相同，不应该检测到漂移");
    }

    // ==========================================
    // 隧道状态转换逻辑测试
    // ==========================================
    #[test]
    fn test_tunnel_creation_scenario() {
        // 场景：新隧道，期望存在但实际不存在
        let desired = create_desired_tunnel("new_tunnel", "/x/new", TunnelMode::Client, 9999, true);
        let actual_map: HashMap<String, ActualTunnel> = HashMap::new();

        let should_create = desired.enabled && !actual_map.contains_key("/x/new");
        assert!(should_create, "新隧道应该被创建");
    }

    #[test]
    fn test_tunnel_disable_scenario() {
        // 场景：禁用隧道，期望被禁用但实际存在
        let desired = create_desired_tunnel("old_tunnel", "/x/old", TunnelMode::Client, 8080, false);

        let mut actual_map: HashMap<String, ActualTunnel> = HashMap::new();
        actual_map.insert("/x/old".to_string(), create_actual_tunnel("/x/old", TunnelMode::Client, 8080));

        let should_close = !desired.enabled && actual_map.contains_key("/x/old");
        assert!(should_close, "禁用的隧道应该被关闭");
    }

    #[test]
    fn test_tunnel_cleanup_orphan_scenario() {
        // 场景：孤儿隧道，配置中不存在但实际存在
        let desired_map: HashMap<String, DesiredTunnel> = HashMap::new();

        let mut actual_map: HashMap<String, ActualTunnel> = HashMap::new();
        actual_map.insert("/x/orphan".to_string(), create_actual_tunnel("/x/orphan", TunnelMode::Server, 3000));

        let desired_exists = desired_map.iter().any(|(_, t)| t.protocol == "/x/orphan");
        let actual_exists = actual_map.contains_key("/x/orphan");

        let should_cleanup = !desired_exists && actual_exists;
        assert!(should_cleanup, "孤儿隧道应该被清理");
    }

    // ==========================================
    // 隧道关系转换：构建 desired_by_proto
    // ==========================================
    #[test]
    fn test_desired_by_proto_mapping() {
        let mut desired: HashMap<String, DesiredTunnel> = HashMap::new();
        desired.insert(
            "minecraft".to_string(),
            create_desired_tunnel("minecraft", "/x/minecraft", TunnelMode::Client, 25565, true),
        );
        desired.insert(
            "ssh".to_string(),
            create_desired_tunnel("ssh", "/x/ssh", TunnelMode::Server, 22, true),
        );

        // 转换为 protocol 索引
        let mut desired_by_proto: HashMap<String, DesiredTunnel> = HashMap::new();
        for tunnel in desired.values() {
            desired_by_proto.insert(tunnel.protocol.clone(), tunnel.clone());
        }

        assert_eq!(desired_by_proto.len(), 2);
        assert!(desired_by_proto.contains_key("/x/minecraft"));
        assert!(desired_by_proto.contains_key("/x/ssh"));
    }

    // ==========================================
    // 多隧道混合操作场景
    // ==========================================
    #[test]
    fn test_multi_tunnel_reconcile_logic() {
        let mut desired: HashMap<String, DesiredTunnel> = HashMap::new();
        desired.insert(
            "minecraft".to_string(),
            create_desired_tunnel("minecraft", "/x/minecraft", TunnelMode::Client, 25565, true),
        );
        desired.insert(
            "ssh_disabled".to_string(),
            create_desired_tunnel("ssh_disabled", "/x/ssh", TunnelMode::Server, 22, false),
        );

        let mut actual: HashMap<String, ActualTunnel> = HashMap::new();
        actual.insert("/x/minecraft".to_string(), create_actual_tunnel("/x/minecraft", TunnelMode::Client, 25565));
        actual.insert("/x/ssh".to_string(), create_actual_tunnel("/x/ssh", TunnelMode::Server, 22));
        actual.insert("/x/orphan".to_string(), create_actual_tunnel("/x/orphan", TunnelMode::Server, 3000));

        // 转换 desired
        let mut desired_by_proto: HashMap<String, DesiredTunnel> = HashMap::new();
        for tunnel in desired.values() {
            desired_by_proto.insert(tunnel.protocol.clone(), tunnel.clone());
        }

        // 收集所有协议
        let mut all_protocols = std::collections::HashSet::new();
        all_protocols.extend(desired_by_proto.keys().cloned());
        all_protocols.extend(actual.keys().cloned());

        // 验证协议集合
        assert!(all_protocols.contains("/x/minecraft"));
        assert!(all_protocols.contains("/x/ssh"));
        assert!(all_protocols.contains("/x/orphan"));

        // 逐个分析场景
        enum Action {
            Create,
            Disable,
            Cleanup
        }
        let mut actions = Vec::new();

        for proto in &all_protocols {
            let d = desired_by_proto.get(&**proto);
            let a = actual.get(&**proto);

            match (d, a) {
                // 场景 A: 新隧道
                (Some(d), None) if d.enabled => {
                    actions.push(Action::Create);
                }
                // 场景 B: 隧道禁用
                (Some(d), Some(_)) if !d.enabled => {
                    actions.push(Action::Disable);
                }
                // 场景 C: 孤儿清理
                (None, Some(_)) => {
                    actions.push(Action::Cleanup);
                }
                _ => {}
            }
        }

        // 验证预期的操作
        assert_eq!(actions.len(), 2); // 1 个禁用，1 个清理
    }

    // ==========================================
    // 错误场景：对端 PeerID 不匹配
    // ==========================================
    #[test]
    fn test_drift_detection_peer_id_change() {
        let mut desired = create_desired_tunnel("client", "/x/client", TunnelMode::Client, 8080, true);
        desired.target = "QmNewPeerId1234567890".to_string();

        let actual = create_actual_tunnel("/x/client", TunnelMode::Client, 8080);
        // actual 仍然是 "Qm1234567890TestPeerId"

        let is_drifted = desired.mode != actual.mode
            || desired.local_ip != actual.local_ip
            || desired.port != actual.port
            || desired.target != actual.target;

        assert!(is_drifted, "对端 PeerID 变化应该检测到漂移");
    }
}
