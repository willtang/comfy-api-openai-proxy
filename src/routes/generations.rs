use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::extract::{Request, State};
use axum::http::HeaderMap;
use axum::Json;
use base64::Engine;
use tracing::{error, info};

use crate::error::AppError;
use crate::image_fetcher::fetch_image_bytes;
use crate::openai::requests::UnifiedImageRequest;
use crate::openai::responses::{ImageData, ImageResponse};
use crate::routes::edits::execute_edit_workflow;
use crate::AppState;

pub async fn handle_generate_image(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    req: Request,
) -> Result<Json<ImageResponse>, AppError> {
    let body_bytes = axum::body::to_bytes(req.into_body(), usize::MAX)
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to read request body: {e}")))?;

    let payload: UnifiedImageRequest = serde_json::from_slice(&body_bytes).map_err(|rejection| {
        error!("Failed to parse JSON body for image generation: {}", rejection);
        AppError::BadRequest(format!("Invalid JSON request body: {}", rejection))
    })?;

    if payload.prompt.trim().is_empty() {
        error!("Bad Request: Field 'prompt' is empty");
        return Err(AppError::BadRequest("Prompt cannot be empty".to_string()));
    }

    // Check if an input image was provided (e.g. Open WebUI image edit request sent to generation endpoint)
    if let Some(input_image_source) = payload.get_input_image() {
        info!("Input image detected in generation payload, executing img2img edit flow");
        let (image_bytes, filename) =
            fetch_image_bytes(&input_image_source, Some(&headers), &state.config).await?;

        return execute_edit_workflow(
            state,
            image_bytes,
            filename,
            payload.prompt,
            payload.model,
            payload.n.unwrap_or(1),
            payload.size,
            payload.response_format.unwrap_or_else(|| "url".to_string()),
        )
        .await;
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
