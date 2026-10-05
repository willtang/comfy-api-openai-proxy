use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use thiserror::Error;
use tracing::{debug, error};

#[derive(Debug, Serialize)]
pub struct OpenAiErrorResponse {
    pub error: OpenAiErrorDetail,
}

#[derive(Debug, Serialize)]
pub struct OpenAiErrorDetail {
    pub message: String,
    #[serde(rename = "type")]
    pub error_type: String,
    pub param: Option<String>,
    pub code: Option<String>,
}

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Comfy API error: {0}")]
    ComfyApiError(String),

    #[error("Job failed: {0}")]
    JobFailed(String),

    #[error("Job timed out")]
    Timeout,

    #[error("Internal server error: {0}")]
    Internal(#[from] anyhow::Error),

    #[error("HTTP client error: {0}")]
    Reqwest(#[from] reqwest::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, err_type, message) = match &self {
            AppError::BadRequest(msg) => {
                debug!("Bad request error: {}", msg);
                (
                    StatusCode::BAD_REQUEST,
                    "invalid_request_error".to_string(),
                    msg.clone(),
                )
            }
            AppError::ComfyApiError(msg) => {
                error!("Comfy API error: {}", msg);
                (
                    StatusCode::BAD_GATEWAY,
                    "comfy_api_error".to_string(),
                    msg.clone(),
                )
            }
            AppError::JobFailed(msg) => {
                error!("Job failed: {}", msg);
                (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "job_execution_error".to_string(),
                    msg.clone(),
                )
            }
            AppError::Timeout => {
                error!("Job execution timed out");
                (
                    StatusCode::GATEWAY_TIMEOUT,
                    "timeout_error".to_string(),
                    "Image generation job timed out before completion".to_string(),
                )
            }
            AppError::Internal(err) => {
                error!("Internal server error: {:#}", err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "server_error".to_string(),
                    err.to_string(),
                )
            }
            AppError::Reqwest(err) => {
                let url_str = err.url().map(|u| u.as_str()).unwrap_or("unknown URL");
                debug!("HTTP client (reqwest) error for URL {}: {}", url_str, err);
                error!("HTTP client (reqwest) error for URL {}: {}", url_str, err);
                (
                    StatusCode::BAD_GATEWAY,
                    "upstream_connection_error".to_string(),
                    format!("{} [url: {}]", err, url_str),
                )
            }
            AppError::Json(err) => {
                error!("JSON error: {}", err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "json_error".to_string(),
                    err.to_string(),
                )
            }
        };

        let body = Json(OpenAiErrorResponse {
            error: OpenAiErrorDetail {
                message,
                error_type: err_type,
                param: None,
                code: None,
            },
        });

        (status, body).into_response()
    }
}
