use crate::error::ReconcileError;
use crate::models::{ActualTunnel, IpfsP2pLsResponse, TunnelMode};
use std::collections::HashMap;
use std::net::IpAddr;
use std::time::Duration;

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
                    // 👈 简化：去掉“瞬时故障”、“指数退避”等花哨词汇
                    tracing::warn!(error = %e, "网络请求失败，正在重试... 剩余次数: {}", attempts - 1);
                    tokio::time::sleep(delay).await;
                    attempts -= 1;
                    delay = std::cmp::min(delay * 2, MAX_RETRY_DELAY);
                }
            }
        }
    }

    pub async fn check_health(&self) -> Result<(), ReconcileError> {
        let url = format!("{}/id", IPFS_RPC_BASE);
        let res = self.http.post(&url).send().await?;
        if !res.status().is_success() {
            return Err(ReconcileError::Unavailable(format!("HTTP {}", res.status())));
        }
        Ok(())
    }

    pub async fn p2p_close(&self, protocol: &str) -> Result<(), ReconcileError> {
        let url = format!("{}/p2p/close", IPFS_RPC_BASE);
        let res = self.http.post(&url).query(&[("arg", protocol)]).send().await?;
        if !res.status().is_success() {
            return Err(ReconcileError::Rejected(res.text().await.unwrap_or_default()));
        }
        Ok(())
    }

    pub async fn p2p_forward(&self, listen: &str, target: &str, protocol: &str) -> Result<(), ReconcileError> {
        let url = format!("{}/p2p/forward", IPFS_RPC_BASE);
        let res = self.http.post(&url)
            .query(&[("arg", protocol), ("arg", listen), ("arg", target)])
            .send()
            .await?;
        if !res.status().is_success() {
            return Err(ReconcileError::Rejected(res.text().await.unwrap_or_default()));
        }
        Ok(())
    }

    pub async fn p2p_listen(&self, target_local: &str, protocol: &str) -> Result<(), ReconcileError> {
        let url = format!("{}/p2p/listen", IPFS_RPC_BASE);
        let res = self.http.post(&url)
            .query(&[("arg", protocol), ("arg", target_local)])
            .send().await?;
        if !res.status().is_success() {
            return Err(ReconcileError::Rejected(res.text().await.unwrap_or_default()));
        }
        Ok(())
    }

    pub async fn load_actual_state(&self) -> anyhow::Result<HashMap<String, ActualTunnel>> {
        let url = format!("{}/p2p/ls", IPFS_RPC_BASE);
        let res = self.http.post(&url).send().await?;
        let ls: IpfsP2pLsResponse = res.json().await?;

        let mut map = HashMap::new();
        for listener in ls.listeners {
            let is_client = listener.target_address.starts_with("/p2p/");
            let mode = if is_client { TunnelMode::Client } else { TunnelMode::Server };
            let local_multiaddr = if is_client { &listener.listen_address } else { &listener.target_address };
            let local_parts: Vec<&str> = local_multiaddr.split('/').filter(|s| !s.is_empty()).collect();
            if local_parts.len() < 4 { continue; }

            let ip: IpAddr = match local_parts[1].parse() { Ok(v) => v, Err(_) => continue };
            let port: u16 = match local_parts[3].parse() { Ok(v) => v, Err(_) => continue };

            let peer_id = if is_client {
                let target_parts: Vec<&str> = listener.target_address.split('/').filter(|s| !s.is_empty()).collect();
                if target_parts.len() < 2 { continue; }
                target_parts[1].to_string()
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
                    protocol: listener.protocol
                },
            );
        }
        Ok(map)
    }
}
