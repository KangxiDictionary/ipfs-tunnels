use crate::error::ReconcileError;
use crate::models::{ActualTunnel, IpfsP2pLsResponse, TunnelMode};
use crate::i18n::{tr, LogKey};
use std::collections::HashMap;
use std::net::IpAddr;
use std::time::Duration;
use tracing::warn;
use multiaddr::{Multiaddr, Protocol};

const IPFS_RPC_BASE: &str = "http://127.0.0.1:5001/api/v0";
const MAX_RETRY_DELAY: Duration = Duration::from_secs(30);

pub struct IpfsClient {
    pub http: reqwest::Client,
}

impl IpfsClient {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .pool_max_idle_per_host(10)
            .build()
            .unwrap();
        Self { http }
    }

    pub async fn execute_with_retry<F, Fut, T>(&self, mut action: F) -> Result<T, ReconcileError>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T, ReconcileError>>,
    {
        let mut attempts = 3;
        let mut delay = Duration::from_millis(300);
        loop {
            match action().await {
                Ok(val) => return Ok(val),
                Err(e) => {
                    if !e.is_retryable() || attempts <= 1 {
                        return Err(e);
                    }
                    warn!(error = ?e, "{}", tr(LogKey::NetworkRetry));
                    tokio::time::sleep(delay).await;
                    attempts -= 1;
                    delay = std::cmp::min(delay * 2, MAX_RETRY_DELAY);
                }
            }
        }
    }

    pub async fn p2p_forward(&self, listen: &str, target: &str, proto: &str) -> Result<(), ReconcileError> {
        let url = format!("{}/p2p/forward?arg={}&arg={}&arg={}", IPFS_RPC_BASE, urlencoding::encode(proto), urlencoding::encode(listen), urlencoding::encode(target));
        let res = self.http.post(&url).send().await?;
        if !res.status().is_success() {
            return Err(ReconcileError::Rejected(res.text().await?));
        }
        Ok(())
    }

    pub async fn p2p_listen(&self, listen: &str, proto: &str) -> Result<(), ReconcileError> {
        let url = format!("{}/p2p/listen?arg={}&arg={}", IPFS_RPC_BASE, urlencoding::encode(proto), urlencoding::encode(listen));
        let res = self.http.post(&url).send().await?;
        if !res.status().is_success() {
            return Err(ReconcileError::Rejected(res.text().await?));
        }
        Ok(())
    }

    pub async fn p2p_close(&self, proto: &str) -> Result<(), ReconcileError> {
        let url = format!("{}/p2p/close?arg={}", IPFS_RPC_BASE, urlencoding::encode(proto));
        let res = self.http.post(&url).send().await?;
        if !res.status().is_success() {
            return Err(ReconcileError::Unavailable(res.text().await?));
        }
        Ok(())
    }

    pub async fn load_actual_state(&self) -> Result<HashMap<String, ActualTunnel>, ReconcileError> {
        let url = format!("{}/p2p/ls", IPFS_RPC_BASE);
        let res = self.http.post(&url).send().await?;
        let ls: IpfsP2pLsResponse = res.json().await?;

        let mut map = HashMap::new();
        for listener in ls.listeners {
            let is_client = listener.target_address.starts_with("/p2p/");
            let mode = if is_client { TunnelMode::Client } else { TunnelMode::Server };
            let local_multiaddr_str = if is_client { &listener.listen_address } else { &listener.target_address };

            // 👈 使用 Multiaddr 健壮、安全地进行强类型协议解构
            let maddr: Multiaddr = match local_multiaddr_str.parse() {
                Ok(m) => m,
                Err(e) => {
                    tracing::error!("从 IPFS 获取的 Multiaddr 路径解析失败 [{}]: {}", local_multiaddr_str, e);
                    continue;
                }
            };

            let mut parsed_ip = None;
            let mut parsed_port = None;

            for component in maddr.iter() {
                match component {
                    Protocol::Ip4(ipv4) => parsed_ip = Some(IpAddr::V4(ipv4)),
                    Protocol::Ip6(ipv6) => parsed_ip = Some(IpAddr::V6(ipv6)),
                    Protocol::Tcp(port) => parsed_port = Some(port),
                    _ => {} // 忽略其他不相关的 P2P 组件协议
                }
            }

            let (ip, port) = match (parsed_ip, parsed_port) {
                (Some(i), Some(p)) => (i, p),
                _ => {
                    warn!("Multiaddr 缺少必要的 IP 或 TCP 端口项: {}", local_multiaddr_str);
                    continue;
                }
            };

            // 提取对端 PeerID
            let peer_id = if is_client {
                let target_maddr: Multiaddr = match listener.target_address.parse() {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                let mut p_id = "-".to_string();
                for cmp in target_maddr.iter() {
                    if let Protocol::P2p(multihash) = cmp {
                        p_id = multihash.to_base58();
                        break;
                    }
                }
                p_id
            } else {
                "-".to_string()
            };

            map.insert(
                listener.protocol.clone(),
                ActualTunnel {
                    mode,
                    local_ip: ip,
                    port,
                    peer_id,
                    protocol: listener.protocol.clone(),
                },
            );
        }
        Ok(map)
    }
}

impl Default for IpfsClient {
    fn default() -> Self {
        Self::new()
    }
}