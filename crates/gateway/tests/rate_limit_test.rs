use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;
use std::sync::Arc;
use multi_agent_gateway::{GatewayServer, GatewayConfig};
use multi_agent_core::mocks::{MockRouter, MockSemanticCache};

#[tokio::test]
async fn test_rate_limiting() {
    let config = GatewayConfig::default();
    let router = Arc::new(MockRouter::complex_mission("test"));
    let cache = Arc::new(MockSemanticCache::new());
    let server = GatewayServer::new(config, router, cache);
    let app = server.build_router();

    // The rate limit is ~120/min (2/sec) with burst 30.
    // We send 30 requests quickly.
    for i in 0..30 {
        let response = app.clone()
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .extension(axum::extract::ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], 12345))))
                    .body(Body::empty())
                    .unwrap()
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "Request {} should have succeeded", i);
    }

    // The 31st request should be rate limited.
    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .extension(axum::extract::ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], 12345))))
                .body(Body::empty())
                .unwrap()
        )
        .await
        .unwrap();

    
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}
