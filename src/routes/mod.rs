pub mod edits;
pub mod generations;
pub mod videos;

use std::sync::Arc;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::{from_fn, Next};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;
use tracing::{error, info};

use crate::error::{OpenAiErrorDetail, OpenAiErrorResponse};
use crate::AppState;

pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/v1/health", get(health_check))
        .route("/models", get(list_models))
        .route("/v1/models", get(list_models))
        .route("/images/generations", post(generations::handle_generate_image))
        .route("/v1/images/generations", post(generations::handle_generate_image))
        .route("/images/edits", post(edits::handle_edit_image))
        .route("/v1/images/edits", post(edits::handle_edit_image))
        .route("/videos", post(videos::handle_generate_video))
        .route("/v1/videos", post(videos::handle_generate_video))
        .route("/videos/generations", post(videos::handle_generate_video))
        .route("/v1/videos/generations", post(videos::handle_generate_video))
        .route("/videos/:id", get(videos::handle_get_video))
        .route("/v1/videos/:id", get(videos::handle_get_video))
        .route("/videos/:id/content", get(videos::handle_get_video_content))
        .route("/v1/videos/:id/content", get(videos::handle_get_video_content))
        .fallback(handle_404)
        .layer(from_fn(log_request_middleware))
        .with_state(state)
}

pub async fn log_request_middleware(
    req: Request,
    next: Next,
) -> impl IntoResponse {
    let path = req.uri().path();
    let is_health = path == "/health" || path == "/v1/health";

    let method = req.method().clone();
    let uri = req.uri().clone();

    if !is_health {
        info!("--> Incoming HTTP Request: {} {}", method, uri);
    }

    let response = next.run(req).await;

    if !is_health {
        info!(
            "<-- HTTP Response: {} {} => Status {}",
            method,
            uri,
            response.status()
        );
    }

    response
}

pub async fn handle_404(req: Request) -> impl IntoResponse {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let headers = req.headers().clone();

    error!(
        "404 Not Found: Unmatched HTTP request - method={}, uri={}, headers={:?}",
        method, uri, headers
    );

    let body = Json(OpenAiErrorResponse {
        error: OpenAiErrorDetail {
            message: format!(
                "Unmatched route: {} {}. Supported endpoints: /v1/images/generations, /v1/images/edits, /v1/videos, /v1/models, /health",
                method, uri
            ),
            error_type: "invalid_request_error".to_string(),
            param: None,
            code: Some("route_not_found".to_string()),
        },
    });

    (StatusCode::NOT_FOUND, body)
}

async fn health_check() -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "service": "comfy-api-openai-proxy",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

async fn list_models(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let default_model = state
        .config
        .default_checkpoint
        .clone()
        .unwrap_or_else(|| "v1-5-pruned-emaonly.ckpt".to_string());

    let mut models = vec![
        json!({
            "id": "dall-e-3",
            "object": "model",
            "created": 1698785189,
            "owned_by": "system"
        }),
        json!({
            "id": "dall-e-2",
            "object": "model",
            "created": 1698785189,
            "owned_by": "system"
        }),
        json!({
            "id": "sora-2",
            "object": "model",
            "created": 1698785189,
            "owned_by": "system"
        }),
        json!({
            "id": "sora",
            "object": "model",
            "created": 1698785189,
            "owned_by": "system"
        }),
        json!({
            "id": default_model,
            "object": "model",
            "created": 1698785189,
            "owned_by": "comfyui"
        }),
    ];

    if let Some(ref vid_model) = state.config.default_video_checkpoint {
        if vid_model != &default_model {
            models.push(json!({
                "id": vid_model,
                "object": "model",
                "created": 1698785189,
                "owned_by": "comfyui"
            }));
        }
    }

    Json(json!({
        "object": "list",
        "data": models
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;
    use std::path::PathBuf;

    use crate::comfy_v2::client::ComfyV2Client;
    use crate::comfy_v2::workflow::{WorkflowManager, WorkflowNodeConfig};
    use crate::config::AppConfig;

    fn create_test_state() -> Arc<AppState> {
        let config = AppConfig {
            host: "127.0.0.1".into(),
            port: 8190,
            comfy_base_url: "http://127.0.0.1:8189".into(),
            comfy_api_key: None,
            openwebui_base_url: None,
            poll_interval_ms: 100,
            img_timeout_secs: 10,
            vid_timeout_secs: 10,
            default_checkpoint: None,
            default_video_checkpoint: None,
            txt2img_template_path: PathBuf::from("templates/txt2img.json"),
            img2img_template_path: PathBuf::from("templates/img2img.json"),
            txt2vid_template_path: PathBuf::from("templates/txt2vid.json"),
            img2vid_template_path: PathBuf::from("templates/img2vid.json"),
            txt2img_prompt_node_id: None,
            img2img_prompt_node_id: None,
            txt2vid_prompt_node_id: None,
            txt2vid_seconds_node_id: None,
            txt2vid_fps_node_id: None,
            img2vid_prompt_node_id: None,
            img2vid_image_node_id: None,
            img2vid_seconds_node_id: None,
            img2vid_fps_node_id: None,
        };
        let comfy_client = ComfyV2Client::new(config.comfy_base_url.clone(), None);
        let workflow_manager = Arc::new(WorkflowManager::new(
            &config.txt2img_template_path,
            &config.img2img_template_path,
            &config.txt2vid_template_path,
            &config.img2vid_template_path,
            WorkflowNodeConfig::default(),
        ));
        let video_cache = Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new()));
        Arc::new(AppState {
            config,
            comfy_client,
            workflow_manager,
            video_cache,
        })
    }

    #[tokio::test]
    async fn test_unmatched_route_404_handler() {
        let app = create_router(create_test_state());

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/unknown/path")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_health_routes() {
        let app = create_router(create_test_state());

        let res1 = app
            .clone()
            .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res1.status(), StatusCode::OK);

        let res2 = app
            .oneshot(
                Request::builder()
                    .uri("/v1/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res2.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_list_models_contains_sora() {
        let app = create_router(create_test_state());

        let res = app
            .oneshot(
                Request::builder()
                    .uri("/v1/models")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let models = json["data"].as_array().unwrap();
        let ids: Vec<&str> = models.iter().filter_map(|m| m["id"].as_str()).collect();
        assert!(ids.contains(&"sora-2"));
        assert!(ids.contains(&"sora"));
    }

    #[tokio::test]
    async fn test_videos_route_empty_prompt_returns_bad_request() {
        let app = create_router(create_test_state());

        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/videos")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"prompt": "   "}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_videos_route_with_input_image_empty_prompt_returns_bad_request() {
        let app = create_router(create_test_state());

        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/videos")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"prompt": "   ", "input_reference": "foo.png"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }
}
