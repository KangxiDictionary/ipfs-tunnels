use crate::error::ReconcileError;
use crate::models::{ActualTunnel, IpfsP2pLsResponse, TunnelMode};
use std::collections::HashMap;
use std::net::IpAddr;
use std::time::Duration;
use tracing::{warn, instrument};
use multiaddr::{Multiaddr, Protocol};

// 🌟 P1 优化：提取常量，避免硬编码
const IPFS_RPC_BASE_URL: &str = "http://127.0.0.1:5001/api/v0";
const MAX_RETRY_ATTEMPTS: u32 = 3;
const INITIAL_RETRY_DELAY_MS: u64 = 300;
const MAX_RETRY_DELAY: Duration = Duration::from_secs(30);

pub struct IpfsClient {
    pub http: reqwest::Client,
    pub base_url: String,
}

impl IpfsClient {
    /// 默认构造器提供生产级别的基础 URL
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .pool_max_idle_per_host(10)
            .build()
            .unwrap();
        Self {
            http,
            base_url: IPFS_RPC_BASE_URL.to_string(),
        }
    }

    /// Builder 模式转换器：允许在测试层任意覆写目标端点
    pub fn with_base_url(mut self, url: String) -> Self {
        self.base_url = url;
        self
    }

    pub async fn execute_with_retry<F, Fut, T>(&self, mut action: F) -> Result<T, ReconcileError>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T, ReconcileError>>,
    {
        let mut attempts = MAX_RETRY_ATTEMPTS;
        let mut delay = Duration::from_millis(INITIAL_RETRY_DELAY_MS);

        loop {
            match action().await {
                Ok(val) => return Ok(val),
                Err(e) => {
                    if !e.is_retryable() || attempts <= 1 {
                        return Err(e);
                    }
                    attempts -= 1;
                    tokio::time::sleep(delay).await;
                    // 🌟 P0 修复：恢复指数退避上限
                    delay = std::cmp::min(delay * 2, MAX_RETRY_DELAY);
                }
            }
        }
    }

    /// 💡 统一处理 IPFS 响应状态码的私有助手
    async fn handle_response(&self, resp: reqwest::Response) -> Result<(), ReconcileError> {
        if resp.status().is_success() {
            Ok(())
        } else {
            let err_text = resp.text().await.unwrap_or_default();
            Err(ReconcileError::Rejected(err_text))
        }
    }

    #[instrument(skip(self))]
    pub async fn p2p_forward(&self, local_maddr: &str, protocol: &str, target_maddr: &str) -> Result<(), ReconcileError> {
        self.execute_with_retry(|| async {
            let url = format!("{}/p2p/forward", self.base_url);
            let resp = self.http.post(&url)
                .query(&[("arg", protocol), ("arg", local_maddr), ("arg", target_maddr)])
                .send()
                .await?;
            self.handle_response(resp).await
        }).await
    }

    #[instrument(skip(self))]
    pub async fn p2p_listen(&self, local_maddr: &str, protocol: &str) -> Result<(), ReconcileError> {
        self.execute_with_retry(|| async {
            let url = format!("{}/p2p/listen", self.base_url);
            let resp = self.http.post(&url)
                .query(&[("arg", protocol), ("arg", local_maddr)])
                .send()
                .await?;
            self.handle_response(resp).await
        }).await
    }

    #[instrument(skip(self))]
    pub async fn p2p_close(&self, protocol: &str) -> Result<(), ReconcileError> {
        self.execute_with_retry(|| async {
            let url = format!("{}/p2p/close", self.base_url);
            // 🌟 P0 修复：移除危险的 ("all", "true") 参数
            // 只关闭指定的 protocol，不要一次关掉所有隧道！
            let resp = self.http.post(&url)
                .query(&[("arg", protocol)])
                .send()
                .await?;
            self.handle_response(resp).await
        }).await
    }

    #[instrument(skip(self))]
    pub async fn load_actual_state(&self) -> Result<HashMap<String, ActualTunnel>, ReconcileError> {
        let url = format!("{}/p2p/ls", self.base_url);
        let resp = self.http.post(&url).send().await?;

        if !resp.status().is_success() {
            let err_text = resp.text().await.unwrap_or_default();
            return Err(ReconcileError::Unavailable(err_text));
        }

        let body: IpfsP2pLsResponse = resp.json().await?;
        let mut map = HashMap::new();

        for listener in body.listeners {
            // 🌟 P2 优化：改用 multiaddr 解析判断 client/server
            // 而不是靠 IP 地址的 127.0.0.1 / ::1 启发式判断
            let is_client = listener.target_address.starts_with("/p2p/");
            let mode = if is_client { TunnelMode::Client } else { TunnelMode::Server };

            // 选择合适的 multiaddr 字符串进行解析
            let local_multiaddr_str = if is_client {
                &listener.listen_address
            } else {
                &listener.target_address
            };

            let maddr: Multiaddr = match local_multiaddr_str.parse() {
                Ok(m) => m,
                Err(e) => {
                    tracing::error!("Multiaddr 解析失败 [{}]: {}", local_multiaddr_str, e);
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
                    _ => {}
                }
            }

            let (ip, port) = match (parsed_ip, parsed_port) {
                (Some(i), Some(p)) => (i, p),
                _ => {
                    warn!("Multiaddr 缺少必要的 IP 或 TCP 端口: {}", local_multiaddr_str);
                    continue;
                }
            };

            let target_val = if is_client {
                listener.target_address.clone()
            } else {
                "-".to_string()
            };

            map.insert(
                listener.protocol.clone(),
                ActualTunnel {
                    mode,
                    local_ip: ip,
                    port,
                    target: target_val, // 👈 存入完整真实的 Multiaddr 字符串
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