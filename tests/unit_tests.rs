#[cfg(test)]
mod unit_tests {
    use ipfs_tunnels_manager::models::{DesiredTunnel, TunnelMode};
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn test_desired_tunnel_creation() {
        let tunnel = DesiredTunnel {
            name: "test".to_string(),
            mode: TunnelMode::Client,
            local_ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port: 8080,
            peer_id: "Qm123...".to_string(),
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
            peer_id: "Qm456...".to_string(),
            protocol: "/x/ssh".to_string(),
            enabled: true,
        };

        let actual = DesiredTunnel {
            name: "ssh".to_string(),
            mode: TunnelMode::Server,  // 不同！
            local_ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port: 2222,
            peer_id: "Qm456...".to_string(),
            protocol: "/x/ssh".to_string(),
            enabled: true,
        };

        let is_drifted = desired.mode != actual.mode
            || desired.local_ip != actual.local_ip
            || desired.port != actual.port
            || desired.peer_id != actual.peer_id;

        assert!(is_drifted);
    }
}