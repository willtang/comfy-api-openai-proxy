use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use axum::extract::State;
use axum::Json;
use base64::Engine;
use tracing::info;

use crate::error::AppError;
use crate::openai::requests::GenerateImageRequest;
use crate::openai::responses::{ImageData, ImageResponse};
use crate::AppState;

pub async fn handle_generate_image(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<GenerateImageRequest>,
) -> Result<Json<ImageResponse>, AppError> {
    if payload.prompt.trim().is_empty() {
        return Err(AppError::BadRequest("Prompt cannot be empty".to_string()));
    }

    let n = payload.n.unwrap_or(1).clamp(1, 10);
    let response_format = payload.response_format.as_deref().unwrap_or("url");
    let size = payload.size.as_deref();
    let checkpoint = payload
        .model
        .as_deref()
        .or(state.config.default_checkpoint.as_deref());

    info!(
        "Received image generation request: prompt='{}', n={}, format='{}', size={:?}",
        payload.prompt, n, response_format, size
    );

    let poll_interval = Duration::from_millis(state.config.poll_interval_ms);
    let timeout = Duration::from_secs(state.config.job_timeout_secs);

    let mut image_datas = Vec::new();

    for i in 0..n {
        info!("Executing generation {}/{}", i + 1, n);

        // 1. Prepare workflow graph
        let workflow = state
            .workflow_manager
            .prepare_txt2img(&payload.prompt, size, checkpoint)?;

        // 2. Submit workflow to Comfy API v2
        let submitted = state.comfy_client.submit_job(workflow, None).await?;

        // 3. Poll job until completed
        let finished_job = state
            .comfy_client
            .poll_job_until_terminal(&submitted.id, poll_interval, timeout)
            .await?;

        let outputs = finished_job.outputs.ok_or_else(|| {
            AppError::JobFailed(format!("Job {} completed but returned no outputs", finished_job.id))
        })?;

        if outputs.is_empty() {
            return Err(AppError::JobFailed(format!(
                "Job {} outputs list is empty",
                finished_job.id
            )));
        }

        // Output from SaveImage node
        let output_url = &outputs[0].url;

        if response_format == "b64_json" {
            let image_bytes = state.comfy_client.fetch_asset_bytes(output_url).await?;
            let b64 = base64::engine::general_purpose::STANDARD.encode(&image_bytes);
            image_datas.push(ImageData {
                url: None,
                b64_json: Some(b64),
                revised_prompt: Some(payload.prompt.clone()),
            });
        } else {
            image_datas.push(ImageData {
                url: Some(output_url.clone()),
                b64_json: None,
                revised_prompt: Some(payload.prompt.clone()),
            });
        }
    }

    let created = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    Ok(Json(ImageResponse {
        created,
        data: image_datas,
    }))
}
