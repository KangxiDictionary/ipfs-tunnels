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

#[cfg(test)]
mod tests {
    use super::*;

    // 👈 换成 tokio::test 宏，允许函数使用 async
    #[tokio::test]
    async fn test_reconcile_error_retryable() {
        // 构造一个必定失败的请求来获取 reqwest::Error
        let reqwest_err = reqwest::get("http://127.0.0.1:0").await.unwrap_err();
        let err = ReconcileError::Transport(reqwest_err);
        assert!(err.is_retryable());

        // 致命错误不可重试
        let fatal = ReconcileError::Rejected("Bad config".to_string());
        assert!(!fatal.is_retryable());

        let rollback = ReconcileError::RollbackFailed("Crash".to_string());
        assert!(!rollback.is_retryable());
    }
}
