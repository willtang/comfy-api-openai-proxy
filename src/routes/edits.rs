use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::extract::{FromRequest, Multipart, Request, State};
use axum::http::HeaderMap;
use axum::Json;
use base64::Engine;
use tracing::{error, info};

use crate::error::AppError;
use crate::image_fetcher::fetch_image_bytes;
use crate::openai::requests::UnifiedImageRequest;
use crate::openai::responses::{ImageData, ImageResponse};
use crate::AppState;

pub async fn handle_edit_image(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    req: Request,
) -> Result<Json<ImageResponse>, AppError> {
    let content_type = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if content_type.contains("multipart/form-data") {
        handle_edit_image_multipart(state, headers, req).await
    } else {
        handle_edit_image_json(state, headers, req).await
    }
}

async fn handle_edit_image_json(
    state: Arc<AppState>,
    headers: HeaderMap,
    req: Request,
) -> Result<Json<ImageResponse>, AppError> {
    let body_bytes = axum::body::to_bytes(req.into_body(), usize::MAX)
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to read request body: {e}")))?;

    let payload: UnifiedImageRequest = serde_json::from_slice(&body_bytes).map_err(|e| {
        error!("Failed to parse JSON for image edit request: {}", e);
        AppError::BadRequest(format!("Invalid JSON request body: {e}"))
    })?;

    if payload.prompt.trim().is_empty() {
        return Err(AppError::BadRequest("Prompt cannot be empty".to_string()));
    }

    let input_image_source = payload.get_input_image().ok_or_else(|| {
        AppError::BadRequest(
            "Image edit JSON request requires 'image_urls', 'image_url', 'images', or 'image' field"
                .to_string(),
        )
    })?;

    let (image_bytes, filename) =
        fetch_image_bytes(&input_image_source, Some(&headers), &state.config).await?;

    execute_edit_workflow(
        state,
        image_bytes,
        filename,
        payload.prompt,
        payload.model,
        payload.n.unwrap_or(1),
        payload.size,
        payload.response_format.unwrap_or_else(|| "url".to_string()),
    )
    .await
}

fn extract_url_from_text(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Handle JSON array string e.g. ["/api/v1/files/..."]
    if let Ok(vec) = serde_json::from_str::<Vec<String>>(trimmed) {
        if let Some(first) = vec.first() {
            let first_trimmed = first.trim();
            if !first_trimmed.is_empty() {
                return Some(first_trimmed.to_string());
            }
        }
    }

    // Handle JSON object e.g. {"url": "/api/v1/files/..."}
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(url) = val.get("url").and_then(|u| u.as_str()) {
            let url_trimmed = url.trim();
            if !url_trimmed.is_empty() {
                return Some(url_trimmed.to_string());
            }
        }
    }

    // Handle direct string e.g. "/api/v1/files/..." or "http://..." or "data:..."
    if trimmed.starts_with('/')
        || trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || trimmed.starts_with("data:")
    {
        return Some(trimmed.to_string());
    }

    None
}

async fn handle_edit_image_multipart(
    state: Arc<AppState>,
    headers: HeaderMap,
    req: Request,
) -> Result<Json<ImageResponse>, AppError> {
    let mut multipart = Multipart::from_request(req, &state)
        .await
        .map_err(|rejection| {
            error!("Failed to parse multipart body for image edit: {}", rejection);
            AppError::BadRequest(format!("Invalid multipart body: {}", rejection))
        })?;

    let mut raw_image_bytes: Option<Vec<u8>> = None;
    let mut image_filename = "input.png".to_string();
    let mut image_url_str: Option<String> = None;
    let mut prompt: Option<String> = None;
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
        let clean_name = name.trim_end_matches("[]").to_lowercase();

        info!(
            "Multipart field received: raw_name='{}', clean_name='{}', filename='{:?}'",
            name,
            clean_name,
            field.file_name()
        );

        match clean_name.as_str() {
            "image" | "images" | "image_urls" | "image_url" | "file" | "files" | "input"
            | "input_image" => {
                if let Some(fname) = field.file_name() {
                    if !fname.trim().is_empty() {
                        image_filename = fname.to_string();
                    }
                }
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("Failed to read field '{name}': {e}")))?;

                if !bytes.is_empty() {
                    if let Ok(text) = std::str::from_utf8(&bytes) {
                        if let Some(url) = extract_url_from_text(text) {
                            info!("Extracted image URL from text in field '{}': {}", name, url);
                            image_url_str = Some(url);
                        } else {
                            raw_image_bytes = Some(bytes.to_vec());
                        }
                    } else {
                        raw_image_bytes = Some(bytes.to_vec());
                    }
                }
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
                if clean_name.starts_with("image") || clean_name.starts_with("file") {
                    if let Some(fname) = field.file_name() {
                        if !fname.trim().is_empty() {
                            image_filename = fname.to_string();
                        }
                    }
                    let bytes = field
                        .bytes()
                        .await
                        .map_err(|e| AppError::BadRequest(format!("Failed to read field '{name}': {e}")))?;

                    if !bytes.is_empty() {
                        if let Ok(text) = std::str::from_utf8(&bytes) {
                            if let Some(url) = extract_url_from_text(text) {
                                info!("Extracted image URL from field '{}': {}", name, url);
                                image_url_str = Some(url);
                            } else {
                                raw_image_bytes = Some(bytes.to_vec());
                            }
                        } else {
                            raw_image_bytes = Some(bytes.to_vec());
                        }
                    }
                } else {
                    info!("Ignoring unrecognized multipart field: '{}'", name);
                }
            }
        }
    }

    let prompt_text = prompt
        .ok_or_else(|| AppError::BadRequest("Field 'prompt' is required in multipart body".to_string()))?;

    if prompt_text.trim().is_empty() {
        return Err(AppError::BadRequest("Prompt cannot be empty".to_string()));
    }

    let (image_bytes, filename) = match (raw_image_bytes, image_url_str) {
        (Some(bytes), None) => (bytes, image_filename),
        (_, Some(url_str)) => {
            fetch_image_bytes(&url_str, Some(&headers), &state.config).await?
        }
        (None, None) => {
            return Err(AppError::BadRequest(
                "Field 'image' or 'image_urls' is required in multipart body".to_string(),
            ));
        }
    };

    execute_edit_workflow(
        state,
        image_bytes,
        filename,
        prompt_text,
        model,
        n,
        size,
        response_format,
    )
    .await
}

pub async fn execute_edit_workflow(
    state: Arc<AppState>,
    image_data: Vec<u8>,
    image_filename: String,
    prompt_text: String,
    model: Option<String>,
    n: u32,
    size: Option<String>,
    response_format: String,
) -> Result<Json<ImageResponse>, AppError> {
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

    // Guess mime type of input image
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
        let image_bytes = state.comfy_client.fetch_asset_bytes(output_url).await?;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&image_bytes);
        let data_uri = format!("data:image/png;base64,{b64}");

        let url_val = if output_url.starts_with("http://") || output_url.starts_with("https://") {
            if output_url.contains("host.docker.internal")
                || output_url.contains("127.0.0.1")
                || output_url.contains("localhost")
            {
                data_uri.clone()
            } else {
                output_url.clone()
            }
        } else {
            data_uri.clone()
        };

        image_datas.push(ImageData {
            url: Some(url_val),
            b64_json: Some(b64),
            revised_prompt: Some(prompt_text.clone()),
        });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_url_from_text() {
        assert_eq!(
            extract_url_from_text("[\"/api/v1/files/ab0f71c7/content\"]"),
            Some("/api/v1/files/ab0f71c7/content".to_string())
        );
        assert_eq!(
            extract_url_from_text("/api/v1/files/ab0f71c7/content"),
            Some("/api/v1/files/ab0f71c7/content".to_string())
        );
        assert_eq!(
            extract_url_from_text("{\"url\": \"http://example.com/img.png\"}"),
            Some("http://example.com/img.png".to_string())
        );
        assert_eq!(
            extract_url_from_text("data:image/png;base64,iVBORw0KGgo="),
            Some("data:image/png;base64,iVBORw0KGgo=".to_string())
        );
        assert_eq!(extract_url_from_text("some random prompt text"), None);
    }
}
