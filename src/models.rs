use serde::Deserialize;
use std::net::IpAddr;

#[derive(Debug, Clone, PartialEq)]
pub enum TunnelMode {
    Client, // 对应 forward
    Server, // 对应 listen
}

#[derive(Debug, Clone, PartialEq)]
pub struct DesiredTunnel {
    pub name: String,
    pub mode: TunnelMode, // 👈 新增角色模式
    pub local_ip: IpAddr,
    pub port: u16,
    pub peer_id: String,  // 客户端填对方 PeerID，服务端填 "-" 作为占位
    pub protocol: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActualTunnel {
    pub mode: TunnelMode, // 👈 实际状态也区分角色
    pub local_ip: IpAddr,
    pub port: u16,
    pub peer_id: String,
    pub protocol: String,
}

#[derive(Deserialize, Debug)]
pub struct IpfsListener {
    #[serde(rename = "Protocol")]
    pub protocol: String,
    #[serde(rename = "ListenAddress")]
    pub listen_address: String,
    #[serde(rename = "TargetAddress")]
    pub target_address: String,
}

#[derive(Deserialize, Debug)]
pub struct IpfsP2pLsResponse {
    #[serde(rename = "Listeners", default)]
    pub listeners: Vec<IpfsListener>,
}
