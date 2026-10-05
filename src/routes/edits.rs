use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use axum::extract::multipart::MultipartRejection;
use axum::extract::{Multipart, State};
use axum::Json;
use base64::Engine;
use tracing::{error, info};

use crate::error::AppError;
use crate::openai::responses::{ImageData, ImageResponse};
use crate::AppState;

pub async fn handle_edit_image(
    State(state): State<Arc<AppState>>,
    request: Result<Multipart, MultipartRejection>,
) -> Result<Json<ImageResponse>, AppError> {
    let mut multipart = match request {
        Ok(mp) => mp,
        Err(rejection) => {
            error!("Failed to parse multipart body for image edit: {}", rejection);
            return Err(AppError::BadRequest(format!(
                "Invalid multipart body: {}",
                rejection
            )));
        }
    };

    let mut image_bytes: Option<Vec<u8>> = None;
    let mut image_filename = "input.png".to_string();
    let mut prompt: Option<String> = None;
    let mut _mask_bytes: Option<Vec<u8>> = None;
    let mut _mask_filename = "mask.png".to_string();
    let mut model: Option<String> = None;
    let mut n: u32 = 1;
    let mut size: Option<String> = None;
    let mut response_format = "url".to_string();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to read multipart stream: {e}")))?
    {
        let name = field.name().unwrap_or_default().to_string();
        match name.as_str() {
            "image" => {
                if let Some(fname) = field.file_name() {
                    image_filename = fname.to_string();
                }
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("Failed to read image bytes: {e}")))?;
                image_bytes = Some(bytes.to_vec());
            }
            "mask" => {
                if let Some(fname) = field.file_name() {
                    _mask_filename = fname.to_string();
                }
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("Failed to read mask bytes: {e}")))?;
                _mask_bytes = Some(bytes.to_vec());
            }
            "prompt" => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("Failed to read prompt text: {e}")))?;
                prompt = Some(text);
            }
            "model" => {
                let text = field.text().await.unwrap_or_default();
                if !text.trim().is_empty() {
                    model = Some(text);
                }
            }
            "n" => {
                let text = field.text().await.unwrap_or_default();
                if let Ok(num) = text.parse::<u32>() {
                    n = num.clamp(1, 10);
                }
            }
            "size" => {
                let text = field.text().await.unwrap_or_default();
                if !text.trim().is_empty() {
                    size = Some(text);
                }
            }
            "response_format" => {
                let text = field.text().await.unwrap_or_default();
                if !text.trim().is_empty() {
                    response_format = text;
                }
            }
            _ => {
                // Ignore unrecognized fields
            }
        }
    }

    let image_data = image_bytes
        .ok_or_else(|| AppError::BadRequest("Field 'image' is required in multipart body".to_string()))?;
    let prompt_text = prompt
        .ok_or_else(|| AppError::BadRequest("Field 'prompt' is required in multipart body".to_string()))?;

    if prompt_text.trim().is_empty() {
        return Err(AppError::BadRequest("Prompt cannot be empty".to_string()));
    }

    let checkpoint = model
        .as_deref()
        .or(state.config.default_checkpoint.as_deref());

    info!(
        "Received image edit request: prompt='{}', filename='{}', image_len={}, n={}, format='{}'",
        prompt_text,
        image_filename,
        image_data.len(),
        n,
        response_format
    );

    // Guess mime type of uploaded image
    let mime = mime_guess::from_path(&image_filename)
        .first_or_octet_stream()
        .to_string();

    // 1. Upload input image to Comfy API v2
    let uploaded_asset = state
        .comfy_client
        .upload_asset(&image_filename, image_data, &mime)
        .await?;

    info!("Input image uploaded to v2 asset id: {}", uploaded_asset.id);

    let poll_interval = Duration::from_millis(state.config.poll_interval_ms);
    let timeout = Duration::from_secs(state.config.job_timeout_secs);

    let mut image_datas = Vec::new();

    for i in 0..n {
        info!("Executing edit generation {}/{}", i + 1, n);

        // 2. Prepare img2img workflow with core/ASSET reference
        let workflow = state.workflow_manager.prepare_img2img(
            &uploaded_asset.id,
            &prompt_text,
            size.as_deref(),
            checkpoint,
        )?;

        // 3. Submit workflow to Comfy API v2
        let submitted = state.comfy_client.submit_job(workflow, None).await?;

        // 4. Poll job until completion
        let finished_job = state
            .comfy_client
            .poll_job_until_terminal(&submitted.id, poll_interval, timeout)
            .await?;

        let outputs = finished_job.outputs.ok_or_else(|| {
            AppError::JobFailed(format!(
                "Job {} completed but returned no outputs",
                finished_job.id
            ))
        })?;

        if outputs.is_empty() {
            return Err(AppError::JobFailed(format!(
                "Job {} outputs list is empty",
                finished_job.id
            )));
        }

        let output_url = &outputs[0].url;

        if response_format == "b64_json" {
            let image_bytes = state.comfy_client.fetch_asset_bytes(output_url).await?;
            let b64 = base64::engine::general_purpose::STANDARD.encode(&image_bytes);
            image_datas.push(ImageData {
                url: None,
                b64_json: Some(b64),
                revised_prompt: Some(prompt_text.clone()),
            });
        } else {
            image_datas.push(ImageData {
                url: Some(output_url.clone()),
                b64_json: None,
                revised_prompt: Some(prompt_text.clone()),
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
