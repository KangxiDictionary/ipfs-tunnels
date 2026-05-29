#[derive(thiserror::Error, Debug)]
pub enum ReconcileError {
    #[error("网络传输故障 (可重试): {0}")]
    Transport(#[from] reqwest::Error),

    #[error("IPFS 节点拒绝配置 (致命错误): {0}")]
    Rejected(String),

    #[error("RPC 接口不可用 (致命错误): {0}")]
    Unavailable(String),

    #[error("灾难性故障：回滚失败，隧道状态已损坏: {0}")]
    RollbackFailed(String),
}

impl ReconcileError {
    pub fn is_retryable(&self) -> bool {
        matches!(self, ReconcileError::Transport(_))
    }
}
