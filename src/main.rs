pub mod comfy_v2;
pub mod config;
pub mod error;
pub mod image_fetcher;
pub mod openai;
pub mod routes;

use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::comfy_v2::client::ComfyV2Client;
use crate::comfy_v2::workflow::{WorkflowManager, WorkflowNodeConfig};
use crate::config::AppConfig;

#[derive(Clone, Debug)]
pub struct CachedVideo {
    pub id: String,
    pub model: Option<String>,
    pub prompt: String,
    pub created_at: u64,
    pub completed_at: u64,
    pub url: String,
    pub b64_json: Option<String>,
    pub bytes: Vec<u8>,
    pub mime_type: String,
}

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub comfy_client: ComfyV2Client,
    pub workflow_manager: Arc<WorkflowManager>,
    pub video_cache: Arc<tokio::sync::RwLock<std::collections::HashMap<String, CachedVideo>>>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Initialize Tracing
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "comfy_api_openai_proxy=debug,tower_http=debug,info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // 2. Load Configuration
    let config = AppConfig::from_env();
    info!("Starting Comfy API v2 OpenAI Proxy...");
    info!("ComfyUI v2 Base URL: {}", config.comfy_base_url);
    if let Some(ref base) = config.openwebui_base_url {
        info!("Open WebUI Base URL: {}", base);
    }
    info!(
        "ComfyUI API Key configured: {}",
        if config.comfy_api_key.is_some() { "Yes" } else { "No" }
    );
    info!("Poll Interval: {} ms, Image Timeout: {} s, Video Timeout: {} s", config.poll_interval_ms, config.img_timeout_secs, config.vid_timeout_secs);
    info!("txt2img template path: {:?}", config.txt2img_template_path);
    info!("img2img template path: {:?}", config.img2img_template_path);
    info!("txt2vid template path: {:?}", config.txt2vid_template_path);
    info!("img2vid template path: {:?}", config.img2vid_template_path);
    if let Some(ref node_id) = config.txt2img_prompt_node_id {
        info!("txt2img prompt node ID: {}", node_id);
    }
    if let Some(ref node_id) = config.img2img_prompt_node_id {
        info!("img2img prompt node ID: {}", node_id);
    }
    if let Some(ref node_id) = config.txt2vid_prompt_node_id {
        info!("txt2vid prompt node ID: {}", node_id);
    }
    if let Some(ref node_id) = config.txt2vid_seconds_node_id {
        info!("txt2vid seconds node ID: {}", node_id);
    }
    if let Some(ref node_id) = config.txt2vid_fps_node_id {
        info!("txt2vid fps node ID: {}", node_id);
    }
    if let Some(ref node_id) = config.img2vid_prompt_node_id {
        info!("img2vid prompt node ID: {}", node_id);
    }
    if let Some(ref node_id) = config.img2vid_image_node_id {
        info!("img2vid image node ID: {}", node_id);
    }
    if let Some(ref node_id) = config.img2vid_seconds_node_id {
        info!("img2vid seconds node ID: {}", node_id);
    }
    if let Some(ref node_id) = config.img2vid_fps_node_id {
        info!("img2vid fps node ID: {}", node_id);
    }

    // 3. Initialize Shared Services
    let comfy_client = ComfyV2Client::new(
        config.comfy_base_url.clone(),
        config.comfy_api_key.clone(),
    );
    let node_config = WorkflowNodeConfig {
        txt2img_prompt_node_id: config.txt2img_prompt_node_id.clone(),
        img2img_prompt_node_id: config.img2img_prompt_node_id.clone(),
        txt2vid_prompt_node_id: config.txt2vid_prompt_node_id.clone(),
        txt2vid_seconds_node_id: config.txt2vid_seconds_node_id.clone(),
        txt2vid_fps_node_id: config.txt2vid_fps_node_id.clone(),
        img2vid_prompt_node_id: config.img2vid_prompt_node_id.clone(),
        img2vid_image_node_id: config.img2vid_image_node_id.clone(),
        img2vid_seconds_node_id: config.img2vid_seconds_node_id.clone(),
        img2vid_fps_node_id: config.img2vid_fps_node_id.clone(),
    };
    let workflow_manager = Arc::new(WorkflowManager::new(
        &config.txt2img_template_path,
        &config.img2img_template_path,
        &config.txt2vid_template_path,
        &config.img2vid_template_path,
        node_config,
    ));

    let video_cache = Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new()));

    let state = Arc::new(AppState {
        config: config.clone(),
        comfy_client,
        workflow_manager,
        video_cache,
    });

    // 4. Setup CORS & Middleware
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = routes::create_router(state)
        .layer(cors)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &axum::http::Request<_>| {
                    let path = request.uri().path();
                    if path == "/health" || path == "/v1/health" {
                        tracing::Span::none()
                    } else {
                        tracing::debug_span!(
                            "http_request",
                            method = %request.method(),
                            uri = %request.uri(),
                        )
                    }
                })
                .on_request(|_request: &axum::http::Request<_>, span: &tracing::Span| {
                    if !span.is_none() {
                        tracing::debug!("started processing request");
                    }
                })
                .on_response(
                    |response: &axum::http::Response<_>, latency: std::time::Duration, span: &tracing::Span| {
                        if !span.is_none() {
                            tracing::debug!(
                                latency = %format_args!("{} ms", latency.as_millis()),
                                status = %response.status().as_u16(),
                                "finished processing request"
                            );
                        }
                    },
                ),
        );

    // 5. Start Server
    let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;
    info!("Proxy server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
