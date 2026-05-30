#[cfg(test)]
mod unit_tests {
    use ipfs_tunnels_manager::models::{DesiredTunnel, TunnelMode, normalize_target};
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn test_desired_tunnel_creation() {
        let tunnel = DesiredTunnel {
            name: "test".to_string(),
            mode: TunnelMode::Client,
            local_ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port: 8080,
            target: "Qm123...".to_string(),
            protocol: "/x/custom".to_string(),
            enabled: true,
        };

        assert_eq!(tunnel.name, "test");
        assert_eq!(tunnel.mode, TunnelMode::Client);
        assert_eq!(tunnel.port, 8080);
        assert!(tunnel.enabled);
    }

    #[test]
    fn test_tunnel_mode_equality() {
        assert_eq!(TunnelMode::Client, TunnelMode::Client);
        assert_ne!(TunnelMode::Client, TunnelMode::Server);
    }

    #[test]
    fn test_tunnel_drift_detection_logic() {
        let desired = DesiredTunnel {
            name: "ssh".to_string(),
            mode: TunnelMode::Client,
            local_ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port: 2222,
            target: "Qm456...".to_string(),
            protocol: "/x/ssh".to_string(),
            enabled: true,
        };

        let actual = DesiredTunnel {
            name: "ssh".to_string(),
            mode: TunnelMode::Server,  // 不同！
            local_ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port: 2222,
            target: "Qm456...".to_string(),
            protocol: "/x/ssh".to_string(),
            enabled: true,
        };

        let is_drifted = desired.mode != actual.mode
            || desired.local_ip != actual.local_ip
            || desired.port != actual.port
            || desired.target != actual.target;

        assert!(is_drifted);
    }

    // 🟡 修复：添加 normalize_target 的单元测试
    #[test]
    fn test_normalize_target_with_p2p_prefix() {
        assert_eq!(normalize_target("/p2p/Qm123"), "/p2p/Qm123");
    }

    #[test]
    fn test_normalize_target_without_prefix() {
        assert_eq!(normalize_target("Qm123"), "/p2p/Qm123");
    }

    #[test]
    fn test_normalize_target_port_number() {
        assert_eq!(normalize_target("12345"), "/ip4/127.0.0.1/tcp/12345");
    }

    #[test]
    fn test_normalize_target_dash() {
        assert_eq!(normalize_target("-"), "-");
    }

    #[test]
    fn test_normalize_target_multiaddr() {
        assert_eq!(normalize_target("/ip4/192.168.1.1/tcp/9999"), "/ip4/192.168.1.1/tcp/9999");
    }
}
