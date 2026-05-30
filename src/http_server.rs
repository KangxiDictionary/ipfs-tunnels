use axum::{extract::State, routing::get, Router};
use metrics_exporter_prometheus::PrometheusHandle;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Clone)]
pub struct AppCtx {
    pub ipfs_connected: Arc<AtomicBool>,
    // 新增：保存从 main 传过来的 Prometheus 句柄
    pub metrics_handle: PrometheusHandle,
}

pub async fn start_server(port: u16, ctx: AppCtx) -> anyhow::Result<()> {
    let app = Router::new()
        // 暴露给 Prometheus 抓取的接口
        .route("/metrics", get(|State(ctx): State<AppCtx>| async move {
            // 直接调用 handle.render()，它会获取当前全局 Recorder 中最新的数据
            ctx.metrics_handle.render()
        }))
        // 节点健康检查
        .route("/health", get(|State(ctx): State<AppCtx>| async move {
            if ctx.ipfs_connected.load(Ordering::Relaxed) {
                (axum::http::StatusCode::OK, "OK")
            } else {
                (axum::http::StatusCode::SERVICE_UNAVAILABLE, "IPFS Node Offline")
            }
        }))
        // 注入状态
        .with_state(ctx);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    tracing::info!("Metrics/Health server listening on http://{}", listener.local_addr()?);

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    Ok(())
}