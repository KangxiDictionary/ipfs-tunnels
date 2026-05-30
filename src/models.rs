use serde::{Deserialize, Deserializer};
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
    pub target: String,  // 客户端填对方 PeerID，服务端填 "-" 作为占位
    pub protocol: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActualTunnel {
    pub mode: TunnelMode, // 👈 实际状态也区分角色
    pub local_ip: IpAddr,
    pub port: u16,
    pub target: String,
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
    #[serde(rename = "Listeners", deserialize_with = "deserialize_null_to_empty")]
    pub listeners: Vec<IpfsListener>,
}

// 💡 辅助反序列化函数：如果 IPFS 返回 null，将其安全转为空的 Vec
fn deserialize_null_to_empty<'de, D>(deserializer: D) -> Result<Vec<IpfsListener>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<Vec<IpfsListener>>::deserialize(deserializer)?;
    Ok(opt.unwrap_or_default())
}

#[derive(Debug, PartialEq, Eq)]
pub enum DriftReason {
    ModeMismatch,
    AddressMismatch,
    PeerIdMismatch,
}

/// 🟢 优化：标准化目标地址格式
/// 支持多种输入格式：
/// - "/p2p/Qm..." → 保留原样
/// - "Qm..." → 自动补全为 "/p2p/Qm..."
/// - "12345" → 识别为端口号，转换为 "/ip4/127.0.0.1/tcp/12345"
/// - 其他完整 multiaddr → 原样返回
pub fn normalize_target(target: &str) -> String {
    let trimmed = target.trim();

    // 已经是完整的 Multiaddr（以 / 开头）
    if trimmed.starts_with('/') {
        return trimmed.to_string();
    }

    // 纯数字识别为端口号
    if trimmed.chars().all(|c| c.is_numeric() || c == '-') {
        if trimmed == "-" {
            // 服务端占位符
            return "-".to_string();
        }
        return format!("/ip4/127.0.0.1/tcp/{}", trimmed);
    }

    // 普通的 PeerID，自动补全为标准的 /p2p/ 格式
    format!("/p2p/{}", trimmed)
}

impl DesiredTunnel {
    /// 对比实际状态，检测是否存在属性漂移
    pub fn detect_drift(&self, actual: &ActualTunnel) -> Option<DriftReason> {
        if self.mode != actual.mode {
            return Some(DriftReason::ModeMismatch);
        }
        if self.local_ip != actual.local_ip || self.port != actual.port {
            return Some(DriftReason::AddressMismatch);
        }

        // 🟡 优化：比较 target 时先经过标准化处理，防止因简写导致误判
        if normalize_target(&self.target) != normalize_target(&actual.target) {
            return Some(DriftReason::PeerIdMismatch);
        }
        None
    }
}
