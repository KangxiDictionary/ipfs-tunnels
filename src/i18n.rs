use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Zh,
    En,
}

static CURRENT_LANG: OnceLock<Lang> = OnceLock::new();

pub fn init() {
    let lang = sys_locale::get_locale()
        .map(|s| if s.starts_with("zh") { Lang::Zh } else { Lang::En })
        .unwrap_or(Lang::En);
    let _ = CURRENT_LANG.set(lang);
}

pub fn get_lang() -> Lang {
    *CURRENT_LANG.get().unwrap_or(&Lang::En)
}

#[derive(Debug, Clone, Copy)]
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
    ProtocolConflictAbort,
    UpdateTeardown,
    TeardownFailed,
    ApplyFailedRollback,
    RollbackSuccess,
    RollbackInconsistent,
    PartialSync,
}

pub fn tr(key: LogKey) -> &'static str {
    match get_lang() {
        Lang::Zh => match key {
            LogKey::ServiceStarting => "IPFS 隧道后台守护进程正在启动...",
            LogKey::IpfsConnectError => "无法连接到本地 IPFS 节点 RPC 接口！",
            LogKey::InitialSync => "正在执行启动期首次状态强制强同步周期...",
            LogKey::ConfigChanged => "检测到配置文件变更，触发热重载调和周期...",
            LogKey::PeriodicCheck => "触发定时状态维持与漂移校准周期...",
            LogKey::ServiceStopped => "守护进程已安全优雅停止下线。",
            LogKey::ConfigReadError => "读取期望配置失败，本次调和终止。",
            LogKey::PortConflict => "本地端口冲突！同一端口不可分配给多个隧道。",
            LogKey::IpfsReadError => "无法从 IPFS 节点读取当前运行状态，跳过本轮同步。",
            LogKey::SyncComplete => "所有隧道状态同步调和成功，系统处于预期稳态。",
            LogKey::SyncFailed => "部分隧道同步调和执行失败。",
            LogKey::NetworkRetry => "网络请求发生网络层异常，正在发起幂等重试...",
            LogKey::TunnelCreating => "检测到新增隧道配置，正在建立协议流转发...",
            LogKey::TunnelCreated => "隧道建立成功。",
            LogKey::TunnelUpdating => "检测到实际运行状态与配置发生漂移，正在启动热更新...",
            LogKey::TunnelRollbackAttempt => "配置项下发失败！尝试激活事务性安全回滚引擎...",
            LogKey::TunnelRollbackFailed => "灾难性错误：回滚操作失败！隧道当前状态可能处于不一致的损坏状态！",
            LogKey::TunnelUpdated => "隧道更新成功。",
            LogKey::TunnelDisabling => "隧道在配置中已被禁用，正在安全关闭下线...",
            LogKey::TunnelDisabled => "隧道安全关闭成功。",
            LogKey::TunnelCleaning => "在节点中检测到未定义的残留冗余隧道，正在强制清洗...",
            LogKey::TunnelCleaned => "残留冗余隧道清洗成功。",
            LogKey::SigtermReceived => "接收到 SIGTERM 终止信号，正在准备优雅退出...",
            LogKey::SigintReceived => "接收到退出指令 (Ctrl+C)，正在准备优雅退出...",
            LogKey::ProtocolConflictAbort => "致命配置错误：发现了重复分配的全局 P2P 协议流主键，调和器紧急终止！",
            LogKey::UpdateTeardown => "检测到隧道配置发生变更，正在安全下线旧协议流...",
            LogKey::TeardownFailed => "解构下线旧隧道流失败，终止后续创建步骤以防拓扑受损！",
            LogKey::ApplyFailedRollback => "配置项下发失败！正在触发事务性回滚...",
            LogKey::RollbackSuccess => "回滚旧配置成功，隧道已恢复到上一个稳定运行状态。",
            LogKey::RollbackInconsistent => "发现回滚操作失败，系统可能处于不一致状态！",
            LogKey::PartialSync => "本轮调和周期出现部分成功，状态未完全对齐。",
        },
        Lang::En => match key {
            LogKey::ServiceStarting => "IPFS Tunnel Daemon is starting...",
            LogKey::IpfsConnectError => "Failed to connect to local IPFS RPC interface!",
            LogKey::InitialSync => "Executing initial enforcement synchronization cycle...",
            LogKey::ConfigChanged => "Configuration change detected, triggering hot-reload reconciliation...",
            LogKey::PeriodicCheck => "Triggering periodic status maintenance and drift calibration cycle...",
            LogKey::ServiceStopped => "Daemon stopped safely and gracefully.",
            LogKey::ConfigReadError => "Failed to read desired configuration. Terminating synchronization.",
            LogKey::PortConflict => "Local port conflict! The same port cannot be assigned to multiple tunnels.",
            LogKey::IpfsReadError => "Failed to read running state from IPFS, skipping synchronization.",
            LogKey::SyncComplete => "All tunnel states synchronized successfully.",
            LogKey::SyncFailed => "Some tunnels failed to synchronize.",
            LogKey::NetworkRetry => "Network request failed, retrying...",
            LogKey::TunnelCreating => "New configuration found, creating tunnel...",
            LogKey::TunnelCreated => "Tunnel created successfully.",
            LogKey::TunnelUpdating => "Actual state mismatches configuration, updating tunnel...",
            LogKey::TunnelRollbackAttempt => "Failed to apply new configuration, attempting to roll back to old configuration...",
            LogKey::TunnelRollbackFailed => "Fatal error: Rollback failed! Current tunnel state may be corrupted!",
            LogKey::TunnelUpdated => "Tunnel updated successfully.",
            LogKey::TunnelDisabling => "Tunnel is disabled in configuration, closing...",
            LogKey::TunnelDisabled => "Tunnel closed successfully.",
            LogKey::TunnelCleaning => "Found undefined residual tunnel in configuration, cleaning up...",
            LogKey::TunnelCleaned => "Residual tunnel cleaned up successfully.",
            LogKey::SigtermReceived => "Received SIGTERM signal, preparing for graceful exit...",
            LogKey::SigintReceived => "Received exit command (Ctrl+C), preparing for graceful exit...",
            LogKey::ProtocolConflictAbort => "Fatal configuration error: Duplicate global protocol identifier detected, reconciler aborted!",
            LogKey::UpdateTeardown => "Tunnel configuration change detected, safely tearing down old protocol stream...",
            LogKey::TeardownFailed => "Failed to tear down old tunnel stream, aborting subsequent steps to prevent topology corruption!",
            LogKey::ApplyFailedRollback => "Configuration deployment failed! Triggering transactional rollback...",
            LogKey::RollbackSuccess => "Old configuration rolled back successfully, tunnel restored to previous stable state.",
            LogKey::RollbackInconsistent => "Rollback failures detected, system may be in an inconsistent state!",
            LogKey::PartialSync => "Reconciliation cycle partially successful.",
        }
    }
}