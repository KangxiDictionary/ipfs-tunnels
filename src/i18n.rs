use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Zh,
    En,
}

static CURRENT_LANG: OnceLock<Lang> = OnceLock::new();

/// 初始化语言环境（跨平台获取系统语言）
pub fn init() {
    let lang = sys_locale::get_locale()
        .map(|s| {
            if s.starts_with("zh") {
                Lang::Zh
            } else {
                Lang::En
            }
        })
        .unwrap_or(Lang::En); // 拿不到系统语言时默认英文
    let _ = CURRENT_LANG.set(lang);
}

/// 获取当前激活的语言
pub fn get_lang() -> Lang {
    *CURRENT_LANG.get().unwrap_or(&Lang::En)
}

/// 集中管理所有的日志文本 Key
pub enum LogKey {
    ServiceStarting,
    IpfsConnectError,
    InitialSync,
    ConfigChanged,
    PeriodicCheck,
    ServiceStopped,
    ConfigReadError,
    PortConflict,
    IpfsReadError,
    SyncComplete,
    SyncFailed,
    NetworkRetry,
    TunnelCreating,
    TunnelCreated,
    TunnelUpdating,
    TunnelRollbackAttempt,
    TunnelRollbackFailed,
    TunnelUpdated,
    TunnelDisabling,
    TunnelDisabled,
    TunnelCleaning,
    TunnelCleaned,
    SigtermReceived,
    SigintReceived,
}

/// 翻译函数
pub fn tr(key: LogKey) -> &'static str {
    match get_lang() {
        Lang::Zh => match key {
            LogKey::ServiceStarting => "服务正在启动...",
            LogKey::IpfsConnectError => "无法连接到本地 IPFS 节点，请检查服务是否运行",
            LogKey::InitialSync => "正在执行初始状态同步...",
            LogKey::ConfigChanged => "检测到 tunnels.conf 发生修改，开始同步状态...",
            LogKey::PeriodicCheck => "执行 60 秒定时状态检查...",
            LogKey::ServiceStopped => "服务已安全停止。",
            LogKey::ConfigReadError => "读取配置文件失败，跳过本次同步。",
            LogKey::PortConflict => "配置错误：端口存在冲突！终止状态同步。",
            LogKey::IpfsReadError => "无法从 IPFS 读取运行状态，跳过本次同步。",
            LogKey::SyncComplete => "所有隧道状态同步完成。",
            LogKey::SyncFailed => "部分隧道同步失败",
            LogKey::NetworkRetry => "网络请求失败，正在重试...",
            LogKey::TunnelCreating => "发现新配置，正在创建隧道...",
            LogKey::TunnelCreated => "隧道创建成功",
            LogKey::TunnelUpdating => "检测到实际状态与配置不符，正在更新隧道...",
            LogKey::TunnelRollbackAttempt => "新配置应用失败，正在尝试回滚旧配置...",
            LogKey::TunnelRollbackFailed => "致命错误：旧配置回滚失败！隧道当前状态可能损坏！",
            LogKey::TunnelUpdated => "旧隧道更新成功",
            LogKey::TunnelDisabling => "隧道已在配置中禁用，正在关闭...",
            LogKey::TunnelDisabled => "解绑关闭成功",
            LogKey::TunnelCleaning => "发现配置中未定义的残留隧道，正在清理...",
            LogKey::TunnelCleaned => "残留隧道清理完成",
            LogKey::SigtermReceived => "接收到 SIGTERM 信号，正在准备安全退出...",
            LogKey::SigintReceived => "接收到退出指令 (Ctrl+C)，正在准备安全退出...",
        },
        Lang::En => match key {
            LogKey::ServiceStarting => "Service is starting...",
            LogKey::IpfsConnectError => "Failed to connect to local IPFS node, please check if the service is running",
            LogKey::InitialSync => "Executing initial state synchronization...",
            LogKey::ConfigChanged => "Detected changes in tunnels.conf, starting state synchronization...",
            LogKey::PeriodicCheck => "Executing 60-second periodic state check...",
            LogKey::ServiceStopped => "Service stopped safely.",
            LogKey::ConfigReadError => "Failed to read configuration file, skipping synchronization.",
            LogKey::PortConflict => "Configuration error: Port conflict detected! Terminating synchronization.",
            LogKey::IpfsReadError => "Failed to read running state from IPFS, skipping synchronization.",
            LogKey::SyncComplete => "All tunnel states synchronized successfully.",
            LogKey::SyncFailed => "Some tunnels failed to synchronize",
            LogKey::NetworkRetry => "Network request failed, retrying...",
            LogKey::TunnelCreating => "New configuration found, creating tunnel...",
            LogKey::TunnelCreated => "Tunnel created successfully",
            LogKey::TunnelUpdating => "Actual state mismatches configuration, updating tunnel...",
            LogKey::TunnelRollbackAttempt => "Failed to apply new configuration, attempting to roll back to old configuration...",
            LogKey::TunnelRollbackFailed => "Fatal error: Rollback failed! Current tunnel state may be corrupted!",
            LogKey::TunnelUpdated => "Tunnel updated successfully",
            LogKey::TunnelDisabling => "Tunnel is disabled in configuration, closing...",
            LogKey::TunnelDisabled => "Tunnel closed successfully",
            LogKey::TunnelCleaning => "Found undefined residual tunnel in configuration, cleaning up...",
            LogKey::TunnelCleaned => "Residual tunnel cleaned up successfully",
            LogKey::SigtermReceived => "Received SIGTERM signal, preparing for graceful exit...",
            LogKey::SigintReceived => "Received exit command (Ctrl+C), preparing for graceful exit...",
        }
    }
}
