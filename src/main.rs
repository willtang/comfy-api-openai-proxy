pub mod comfy_v2;
pub mod config;
pub mod error;
pub mod openai;
pub mod routes;

use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::comfy_v2::client::ComfyV2Client;
use crate::comfy_v2::workflow::WorkflowManager;
use crate::config::AppConfig;

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub comfy_client: ComfyV2Client,
    pub workflow_manager: Arc<WorkflowManager>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Initialize Tracing
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "comfy_api_openai_proxy=debug,tower_http=info,warn".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // 2. Load Configuration
    let config = AppConfig::from_env();
    info!("Starting Comfy API v2 OpenAI Proxy...");
    info!("ComfyUI v2 Base URL: {}", config.comfy_base_url);
    info!(
        "ComfyUI API Key configured: {}",
        if config.comfy_api_key.is_some() { "Yes" } else { "No" }
    );
    info!("Poll Interval: {} ms, Job Timeout: {} s", config.poll_interval_ms, config.job_timeout_secs);

    // 3. Initialize Shared Services
    let comfy_client = ComfyV2Client::new(
        config.comfy_base_url.clone(),
        config.comfy_api_key.clone(),
    );
    let workflow_manager = Arc::new(WorkflowManager::new(
        &config.txt2img_template_path,
        &config.img2img_template_path,
    ));

    let state = Arc::new(AppState {
        config: config.clone(),
        comfy_client,
        workflow_manager,
    });

    // 4. Setup CORS & Middleware
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = routes::create_router(state)
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    // 5. Start Server
    let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;
    info!("Proxy server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
