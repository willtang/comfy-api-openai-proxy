use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::body::Body;
use axum::extract::{FromRequest, Multipart, Path, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::Response;
use axum::Json;
use base64::Engine;
use tracing::{error, info};

use crate::error::AppError;
use crate::image_fetcher::fetch_image_bytes;
use crate::openai::requests::CreateVideoRequest;
use crate::openai::responses::{VideoData, VideoOutput, VideoResponse};
use crate::{AppState, CachedVideo};

pub async fn handle_generate_video(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    req: Request,
) -> Result<Json<VideoResponse>, AppError> {
    let content_type = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if content_type.contains("multipart/form-data") {
        handle_generate_video_multipart(state, headers, req).await
    } else {
        handle_generate_video_json(state, headers, req).await
    }
}

async fn handle_generate_video_json(
    state: Arc<AppState>,
    headers: HeaderMap,
    req: Request,
) -> Result<Json<VideoResponse>, AppError> {
    let body_bytes = axum::body::to_bytes(req.into_body(), usize::MAX)
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to read request body: {e}")))?;

    let payload: CreateVideoRequest = serde_json::from_slice(&body_bytes).map_err(|rejection| {
        error!("Failed to parse JSON body for video generation: {}", rejection);
        AppError::BadRequest(format!("Invalid JSON request body: {}", rejection))
    })?;

    if payload.prompt.trim().is_empty() {
        error!("Bad Request: Field 'prompt' is empty");
        return Err(AppError::BadRequest("Prompt cannot be empty".to_string()));
    }

    let input_image = if let Some(img_source) = payload.get_input_image() {
        info!("Found input image reference in video request: {}", img_source);
        let (bytes, filename) = fetch_image_bytes(&img_source, Some(&headers), &state.config).await?;
        Some((bytes, filename))
    } else {
        None
    };

    let size = payload.size.or_else(|| match payload.aspect_ratio.as_deref() {
        Some("16:9") => Some("1280x720".to_string()),
        Some("9:16") => Some("720x1280".to_string()),
        Some("1:1") => Some("512x512".to_string()),
        Some("4:3") => Some("768x576".to_string()),
        Some("3:4") => Some("576x768".to_string()),
        _ => None,
    });

    let seconds = payload.seconds.or(payload.duration);

    execute_video_generation(
        state,
        payload.prompt,
        payload.model,
        size,
        seconds,
        payload.fps,
        input_image,
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

async fn handle_generate_video_multipart(
    state: Arc<AppState>,
    headers: HeaderMap,
    req: Request,
) -> Result<Json<VideoResponse>, AppError> {
    let mut multipart = Multipart::from_request(req, &state)
        .await
        .map_err(|rejection| {
            error!("Failed to parse multipart body for video generation: {}", rejection);
            AppError::BadRequest(format!("Invalid multipart body: {}", rejection))
        })?;

    let mut raw_image_bytes: Option<Vec<u8>> = None;
    let mut image_filename = "first_frame.png".to_string();
    let mut image_url_str: Option<String> = None;
    let mut prompt: Option<String> = None;
    let mut model: Option<String> = None;
    let mut size: Option<String> = None;
    let mut aspect_ratio: Option<String> = None;
    let mut seconds: Option<u32> = None;
    let mut fps: Option<u32> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to read multipart stream: {e}")))?
    {
        let name = field.name().unwrap_or_default().to_string();
        let clean_name = name.trim_end_matches("[]").to_lowercase();

        info!(
            "Multipart video field received: raw_name='{}', clean_name='{}', filename='{:?}'",
            name,
            clean_name,
            field.file_name()
        );

        match clean_name.as_str() {
            "image" | "images" | "image_urls" | "image_url" | "input_reference" | "first_frame"
            | "first_frame_image" | "file" | "files" | "input" | "input_image" => {
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
            "size" => {
                let text = field.text().await.unwrap_or_default();
                if !text.trim().is_empty() {
                    size = Some(text);
                }
            }
            "aspect_ratio" => {
                let text = field.text().await.unwrap_or_default();
                if !text.trim().is_empty() {
                    aspect_ratio = Some(text);
                }
            }
            "seconds" | "duration" => {
                let text = field.text().await.unwrap_or_default();
                if let Ok(num) = text.parse::<u32>() {
                    seconds = Some(num);
                }
            }
            "fps" => {
                let text = field.text().await.unwrap_or_default();
                if let Ok(num) = text.parse::<u32>() {
                    fps = Some(num);
                }
            }
            _ => {
                if clean_name.starts_with("image")
                    || clean_name.starts_with("file")
                    || clean_name.contains("frame")
                    || clean_name.contains("reference")
                {
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
                                image_url_str = Some(url);
                            } else {
                                raw_image_bytes = Some(bytes.to_vec());
                            }
                        } else {
                            raw_image_bytes = Some(bytes.to_vec());
                        }
                    }
                }
            }
        }
    }

    let prompt_text = prompt
        .ok_or_else(|| AppError::BadRequest("Field 'prompt' is required in request".to_string()))?;

    if prompt_text.trim().is_empty() {
        return Err(AppError::BadRequest("Prompt cannot be empty".to_string()));
    }

    let input_image = match (raw_image_bytes, image_url_str) {
        (Some(bytes), _) => Some((bytes, image_filename)),
        (None, Some(url_str)) => {
            let (bytes, fname) = fetch_image_bytes(&url_str, Some(&headers), &state.config).await?;
            Some((bytes, fname))
        }
        (None, None) => None,
    };

    let resolved_size = size.or_else(|| match aspect_ratio.as_deref() {
        Some("16:9") => Some("1280x720".to_string()),
        Some("9:16") => Some("720x1280".to_string()),
        Some("1:1") => Some("512x512".to_string()),
        Some("4:3") => Some("768x576".to_string()),
        Some("3:4") => Some("576x768".to_string()),
        _ => None,
    });

    execute_video_generation(
        state,
        prompt_text,
        model,
        resolved_size,
        seconds,
        fps,
        input_image,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn execute_video_generation(
    state: Arc<AppState>,
    prompt: String,
    model: Option<String>,
    size: Option<String>,
    seconds: Option<u32>,
    fps: Option<u32>,
    input_image: Option<(Vec<u8>, String)>,
) -> Result<Json<VideoResponse>, AppError> {
    let checkpoint = model
        .as_deref()
        .or(state.config.default_video_checkpoint.as_deref());

    info!(
        "Executing video generation: prompt='{}', model={:?}, size={:?}, seconds={:?}, fps={:?}, has_input_image={}",
        prompt, checkpoint, size, seconds, fps, input_image.is_some()
    );

    let poll_interval = Duration::from_millis(state.config.poll_interval_ms);
    let timeout = Duration::from_secs(state.config.vid_timeout_secs);

    // 1. Prepare workflow graph
    let workflow = if let Some((img_bytes, filename)) = input_image {
        let mime = mime_guess::from_path(&filename)
            .first_or_octet_stream()
            .to_string();

        let uploaded_asset = state
            .comfy_client
            .upload_asset(&filename, img_bytes, &mime)
            .await?;
        info!("Uploaded input image asset id={} for video", uploaded_asset.id);

        state.workflow_manager.prepare_img2vid(
            &uploaded_asset.id,
            &prompt,
            size.as_deref(),
            checkpoint,
            seconds,
            fps,
        )?
    } else {
        state.workflow_manager.prepare_txt2vid(
            &prompt,
            size.as_deref(),
            checkpoint,
            seconds,
            fps,
        )?
    };

    // 2. Submit workflow to Comfy API v2
    let submitted = state.comfy_client.submit_job(workflow, None).await?;
    info!("Submitted video job id={}", submitted.id);

    // 3. Poll job until completed
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

    // Output from Video node (e.g. VHS_VideoCombine or SaveAnimatedWEBP or SaveVideo)
    let output_url = &outputs[0].url;
    let video_bytes = state.comfy_client.fetch_asset_bytes(output_url).await?;

    let mime_type = if video_bytes.starts_with(&[0x1a, 0x45, 0xdf, 0xa3]) {
        "video/webm".to_string()
    } else if video_bytes.len() > 12 && &video_bytes[0..4] == b"RIFF" && &video_bytes[8..12] == b"WEBP" {
        "image/webp".to_string()
    } else {
        "video/mp4".to_string()
    };

    let b64 = base64::engine::general_purpose::STANDARD.encode(&video_bytes);
    let data_uri = format!("data:{mime_type};base64,{b64}");

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

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let video_id = format!("video_{}", submitted.id);

    // Cache completed video for status checking and binary retrieval
    let cached = CachedVideo {
        id: video_id.clone(),
        model: model.clone().or_else(|| state.config.default_video_checkpoint.clone()),
        prompt: prompt.clone(),
        created_at: now,
        completed_at: now,
        url: url_val.clone(),
        b64_json: Some(b64.clone()),
        bytes: video_bytes,
        mime_type,
    };

    {
        let mut cache = state.video_cache.write().await;
        if cache.len() > 100 {
            cache.clear();
        }
        cache.insert(video_id.clone(), cached.clone());
        cache.insert(submitted.id.clone(), cached);
    }

    Ok(Json(VideoResponse {
        id: video_id,
        object: "video".to_string(),
        status: "completed".to_string(),
        created_at: now,
        completed_at: Some(now),
        model: model.or_else(|| state.config.default_video_checkpoint.clone()),
        prompt: Some(prompt.clone()),
        error: None,
        output: Some(VideoOutput {
            url: Some(url_val.clone()),
        }),
        data: vec![VideoData {
            url: Some(url_val.clone()),
            b64_json: Some(b64),
            revised_prompt: Some(prompt),
        }],
        url: Some(url_val),
    }))
}

pub async fn handle_get_video(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<VideoResponse>, AppError> {
    // 1. Check local in-memory cache
    {
        let cache = state.video_cache.read().await;
        if let Some(cached) = cache.get(&id) {
            return Ok(Json(VideoResponse {
                id: cached.id.clone(),
                object: "video".to_string(),
                status: "completed".to_string(),
                created_at: cached.created_at,
                completed_at: Some(cached.completed_at),
                model: cached.model.clone(),
                prompt: Some(cached.prompt.clone()),
                error: None,
                output: Some(VideoOutput {
                    url: Some(cached.url.clone()),
                }),
                data: vec![VideoData {
                    url: Some(cached.url.clone()),
                    b64_json: cached.b64_json.clone(),
                    revised_prompt: Some(cached.prompt.clone()),
                }],
                url: Some(cached.url.clone()),
            }));
        }
    }

    // 2. Fallback: Query upstream ComfyUI job status
    let clean_id = id.strip_prefix("video_").unwrap_or(&id);
    let job = state.comfy_client.get_job(clean_id).await?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let status = if job.is_success() {
        "completed"
    } else if job.status == "failed" {
        "failed"
    } else {
        "in_progress"
    };

    let output_url = job
        .outputs
        .as_ref()
        .and_then(|outputs| outputs.first())
        .map(|out| out.url.clone());

    Ok(Json(VideoResponse {
        id: format!("video_{clean_id}"),
        object: "video".to_string(),
        status: status.to_string(),
        created_at: now,
        completed_at: if job.is_terminal() { Some(now) } else { None },
        model: state.config.default_video_checkpoint.clone(),
        prompt: None,
        error: job.error,
        output: output_url.as_ref().map(|u| VideoOutput {
            url: Some(u.clone()),
        }),
        data: output_url
            .as_ref()
            .map(|u| {
                vec![VideoData {
                    url: Some(u.clone()),
                    b64_json: None,
                    revised_prompt: None,
                }]
            })
            .unwrap_or_default(),
        url: output_url,
    }))
}

pub async fn handle_get_video_content(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    // 1. Check local cache for binary bytes
    {
        let cache = state.video_cache.read().await;
        if let Some(cached) = cache.get(&id) {
            return Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, cached.mime_type.as_str())
                .body(Body::from(cached.bytes.clone()))
                .map_err(|e| AppError::Internal(anyhow::anyhow!(e)));
        }
    }

    // 2. Fallback: Query upstream ComfyUI job output
    let clean_id = id.strip_prefix("video_").unwrap_or(&id);
    let job = state.comfy_client.get_job(clean_id).await?;

    let output_url = job
        .outputs
        .as_ref()
        .and_then(|outputs| outputs.first())
        .map(|out| out.url.as_str())
        .ok_or_else(|| AppError::BadRequest(format!("Video content for '{id}' not found or not ready")))?;

    let bytes = state.comfy_client.fetch_asset_bytes(output_url).await?;

    let mime_type = if bytes.starts_with(&[0x1a, 0x45, 0xdf, 0xa3]) {
        "video/webm"
    } else if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else {
        "video/mp4"
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime_type)
        .body(Body::from(bytes))
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))
}
