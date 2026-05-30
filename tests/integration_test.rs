use serde_json::json;
use ipfs_tunnels_manager::ipfs::IpfsClient;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_load_actual_state_from_mock_ipfs() {
    // 1. 启动 Mock 服务器
    let mock_server = MockServer::start().await;

    // 2. 配置 Mock 路由 (注意：POST 方法和正确地返回结构)
    // 🌟 关键修复：必须调用 .mount() 来注册 mock 规则！
    Mock::given(method("POST"))
        .and(path("/api/v0/p2p/ls"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "Listeners": [] })))
        .mount(&mock_server)
        .await;

    // 3. 🌟 核心：必须把 mock_server.uri() 喂给客户端
    let client = IpfsClient::new()
        .with_base_url(format!("{}/api/v0", mock_server.uri()));

    // 4. 执行调用
    let result = client.load_actual_state().await;

    // Line 32 的 unwrap() 就在这里
    let actual = result.unwrap();
    assert!(actual.is_empty());
}
