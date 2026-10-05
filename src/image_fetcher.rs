use axum::http::HeaderMap;
use base64::Engine;
use reqwest::{Client, Url};
use tracing::{error, info};

use crate::config::AppConfig;
use crate::error::AppError;

/// Fetches image bytes and infers a filename from either:
/// 1. Data URI (`data:image/png;base64,...`)
/// 2. Raw base64 string
/// 3. Absolute HTTP/HTTPS URL
/// 4. Relative URL path (e.g., `/api/v1/files/file_id/content`)
pub async fn fetch_image_bytes(
    input: &str,
    headers: Option<&HeaderMap>,
    config: &AppConfig,
) -> Result<(Vec<u8>, String), AppError> {
    let trimmed = input.trim();

    // 1. Data URI format (e.g. data:image/png;base64,...)
    if trimmed.starts_with("data:") {
        if let Some((mime_part, b64_part)) = trimmed.split_once(',') {
            let mime = mime_part
                .strip_prefix("data:")
                .and_then(|s| s.split(';').next())
                .unwrap_or("image/png")
                .to_string();

            let bytes = base64::engine::general_purpose::STANDARD
                .decode(b64_part.trim())
                .map_err(|e| AppError::BadRequest(format!("Failed to decode base64 image data: {e}")))?;

            let filename = match mime.as_str() {
                "image/jpeg" | "image/jpg" => "input.jpg",
                "image/webp" => "input.webp",
                _ => "input.png",
            }
            .to_string();

            return Ok((bytes, filename));
        }
    }

    // Determine authorization header if available to forward to Open WebUI file endpoint
    let auth_header = headers
        .and_then(|h| h.get("authorization"))
        .and_then(|v| v.to_str().ok());

    // Determine target HTTP URL
    let target_url = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else if trimmed.starts_with('/') {
        let base_url = if let Some(ref env_base) = config.openwebui_base_url {
            env_base.clone()
        } else if let Some(hdrs) = headers {
            if let Some(ref_val) = hdrs.get("referer").and_then(|v| v.to_str().ok()) {
                if let Ok(parsed) = Url::parse(ref_val) {
                    let port_part = parsed
                        .port()
                        .map(|p| format!(":{p}"))
                        .unwrap_or_default();
                    format!(
                        "{}://{}{}",
                        parsed.scheme(),
                        parsed.host_str().unwrap_or("127.0.0.1"),
                        port_part
                    )
                } else {
                    infer_base_from_host(hdrs)
                }
            } else if let Some(orig_val) = hdrs.get("origin").and_then(|v| v.to_str().ok()) {
                orig_val.to_string()
            } else {
                infer_base_from_host(hdrs)
            }
        } else {
            "http://127.0.0.1:3000".to_string()
        };
        format!("{}{}", base_url.trim_end_matches('/'), trimmed)
    } else {
        // Try raw base64 decode if string doesn't look like a URL
        if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(trimmed) {
            return Ok((bytes, "input.png".to_string()));
        }
        return Err(AppError::BadRequest(format!(
            "Invalid image input format or URL: '{trimmed}'"
        )));
    };

    info!("Fetching image bytes from resolved URL: {}", target_url);

    let http_client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to build HTTP client: {e}")))?;

    let mut req = http_client.get(&target_url);
    if let Some(auth) = auth_header {
        req = req.header("authorization", auth);
    }

    let resp = req.send().await.map_err(|e| {
        error!("Failed to send GET request to image URL {target_url}: {e}");
        AppError::BadRequest(format!("Failed to download image from '{target_url}': {e}"))
    })?;

    let status = resp.status();
    if !status.is_success() {
        let err_body = resp.text().await.unwrap_or_default();
        error!("Downloading image from {target_url} failed with status {status}: {err_body}");
        return Err(AppError::BadRequest(format!(
            "Failed to download image from {target_url}: status {status}"
        )));
    }

    // Infer filename from URL path
    let filename = Url::parse(&target_url)
        .ok()
        .and_then(|u| {
            u.path_segments()
                .and_then(|segments| segments.last())
                .map(|s| s.to_string())
        })
        .filter(|s| !s.is_empty() && s.contains('.'))
        .unwrap_or_else(|| "input.png".to_string());

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to read image bytes from {target_url}: {e}")))?
        .to_vec();

    Ok((bytes, filename))
}

fn infer_base_from_host(hdrs: &HeaderMap) -> String {
    if let Some(host) = hdrs
        .get("x-forwarded-host")
        .or_else(|| hdrs.get("host"))
        .and_then(|v| v.to_str().ok())
    {
        let proto = hdrs
            .get("x-forwarded-proto")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("http");
        format!("{proto}://{host}")
    } else {
        "http://127.0.0.1:3000".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_config() -> AppConfig {
        AppConfig {
            comfy_base_url: "http://127.0.0.1:8189".into(),
            comfy_api_key: None,
            openwebui_base_url: Some("http://localhost:3000".into()),
            host: "127.0.0.1".into(),
            port: 8190,
            poll_interval_ms: 100,
            job_timeout_secs: 10,
            txt2img_template_path: PathBuf::from("templates/txt2img.json"),
            img2img_template_path: PathBuf::from("templates/img2img.json"),
            txt2img_prompt_node_id: None,
            img2img_prompt_node_id: None,
            default_checkpoint: None,
        }
    }

    #[tokio::test]
    async fn test_fetch_data_uri() {
        let b64_input = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
        let config = test_config();
        let (bytes, filename) = fetch_image_bytes(b64_input, None, &config)
            .await
            .expect("Data URI should decode successfully");

        assert_eq!(filename, "input.png");
        assert!(!bytes.is_empty());
    }

    #[test]
    fn test_infer_base_from_host() {
        let mut headers = HeaderMap::new();
        headers.insert("host", "mywebui.local:8080".parse().unwrap());
        headers.insert("x-forwarded-proto", "https".parse().unwrap());

        let base = infer_base_from_host(&headers);
        assert_eq!(base, "https://mywebui.local:8080");
    }
}
