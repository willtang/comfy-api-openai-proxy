pub mod edits;
pub mod generations;

use std::sync::Arc;
use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;

use crate::AppState;

pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/v1/models", get(list_models))
        .route("/v1/images/generations", post(generations::handle_generate_image))
        .route("/v1/images/edits", post(edits::handle_edit_image))
        .with_state(state)
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

    Json(json!({
        "object": "list",
        "data": [
            {
                "id": "dall-e-3",
                "object": "model",
                "created": 1698785189,
                "owned_by": "system"
            },
            {
                "id": "dall-e-2",
                "object": "model",
                "created": 1698785189,
                "owned_by": "system"
            },
            {
                "id": default_model,
                "object": "model",
                "created": 1698785189,
                "owned_by": "comfyui"
            }
        ]
    }))
}
