use std::time::Duration;
use reqwest::multipart::{Form, Part};
use reqwest::Client;
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::comfy_v2::models::{AssetUploadResponse, JobStatusResponse, JobSubmitRequest};
use crate::error::AppError;

#[derive(Clone)]
pub struct ComfyV2Client {
    client: Client,
    base_url: String,
    api_key: Option<String>,
}

impl ComfyV2Client {
    pub fn new(base_url: String, api_key: Option<String>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .expect("Failed to initialize reqwest client");

        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
        }
    }

    fn apply_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(key) = &self.api_key {
            req.bearer_auth(key)
        } else {
            req
        }
    }

    /// Upload an input image/asset via POST /api/v2/assets
    pub async fn upload_asset(
        &self,
        filename: &str,
        data: Vec<u8>,
        mime_type: &str,
    ) -> Result<AssetUploadResponse, AppError> {
        let url = format!("{}/api/v2/assets", self.base_url);
        debug!("Uploading asset to {} with filename={}", url, filename);

        let file_part_filepath = Part::bytes(data.clone())
            .file_name(filename.to_string())
            .mime_str(mime_type)
            .map_err(|e| AppError::BadRequest(format!("Invalid mime type: {e}")))?;

        let file_part_file = Part::bytes(data)
            .file_name(filename.to_string())
            .mime_str(mime_type)
            .map_err(|e| AppError::BadRequest(format!("Invalid mime type: {e}")))?;

        let form = Form::new()
            .part("file_path", file_part_filepath)
            .part("file", file_part_file)
            .text("tags", "input");

        let req = self.apply_auth(self.client.post(&url)).multipart(form);
        let resp = req.send().await?;

        let status = resp.status();
        if !status.is_success() {
            let error_text = resp.text().await.unwrap_or_default();
            debug!("HTTP request error: url={}, status={}, body={}", url, status, error_text);
            error!("Upload asset failed for url {}: status {} - {}", url, status, error_text);
            return Err(AppError::ComfyApiError(format!(
                "Asset upload failed ({}): {} [url: {}]",
                status, error_text, url
            )));
        }

        let asset: AssetUploadResponse = resp.json().await?;
        info!("Successfully uploaded asset: id={}", asset.id);
        Ok(asset)
    }

    /// Submit a workflow via POST /api/v2/jobs
    pub async fn submit_job(
        &self,
        workflow: serde_json::Value,
        extra_data: Option<serde_json::Value>,
    ) -> Result<JobStatusResponse, AppError> {
        let url = format!("{}/api/v2/jobs", self.base_url);
        let idempotency_key = Uuid::new_v4().to_string();

        debug!(
            "Submitting job to {} with Idempotency-Key: {}",
            url, idempotency_key
        );

        debug!(
            "Workflow JSON:\n{}",
            serde_json::to_string_pretty(&workflow).unwrap_or_else(|_| workflow.to_string())
        );

        let payload = JobSubmitRequest {
            workflow,
            extra_data,
        };

        let req = self
            .apply_auth(self.client.post(&url))
            .header("Idempotency-Key", idempotency_key)
            .json(&payload);

        let resp = req.send().await?;
        let status = resp.status();

        if !status.is_success() {
            let error_text = resp.text().await.unwrap_or_default();
            debug!("HTTP request error: url={}, status={}, body={}", url, status, error_text);
            error!("Job submission failed for url {}: status {} - {}", url, status, error_text);
            return Err(AppError::ComfyApiError(format!(
                "Job submission failed ({}): {} [url: {}]",
                status, error_text, url
            )));
        }

        let job: JobStatusResponse = resp.json().await?;
        info!("Submitted job id={}, status={}", job.id, job.status);
        Ok(job)
    }

    /// Retrieve job status via GET /api/v2/jobs/{id}
    pub async fn get_job(&self, job_id: &str) -> Result<JobStatusResponse, AppError> {
        let url = format!("{}/api/v2/jobs/{}", self.base_url, job_id);
        let req = self.apply_auth(self.client.get(&url));
        let resp = req.send().await?;

        let status = resp.status();
        if !status.is_success() {
            let error_text = resp.text().await.unwrap_or_default();
            debug!("HTTP request error: url={}, status={}, body={}", url, status, error_text);
            error!("Failed to get job {job_id} for url {}: status {} - {}", url, status, error_text);
            return Err(AppError::ComfyApiError(format!(
                "Failed to get job {job_id} ({}): {} [url: {}]",
                status, error_text, url
            )));
        }

        let job: JobStatusResponse = resp.json().await?;
        Ok(job)
    }

    /// Poll until job reaches terminal state
    pub async fn poll_job_until_terminal(
        &self,
        job_id: &str,
        poll_interval: Duration,
        timeout: Duration,
    ) -> Result<JobStatusResponse, AppError> {
        let start = tokio::time::Instant::now();

        loop {
            if start.elapsed() >= timeout {
                error!("Job {} timed out after {:?}", job_id, timeout);
                return Err(AppError::Timeout);
            }

            let job = self.get_job(job_id).await?;
            debug!("Polled job {}: status={}", job.id, job.status);

            if job.is_terminal() {
                if job.is_success() {
                    info!("Job {} completed successfully", job_id);
                    return Ok(job);
                } else {
                    let err_msg = job
                        .error
                        .as_ref()
                        .map(|e| e.to_string())
                        .unwrap_or_else(|| format!("Job ended with status: {}", job.status));
                    error!("Job {} failed: {}", job_id, err_msg);
                    return Err(AppError::JobFailed(err_msg));
                }
            }

            tokio::time::sleep(poll_interval).await;
        }
    }

    /// Fetch image bytes given an output URL or asset endpoint
    pub async fn fetch_asset_bytes(&self, target_url: &str) -> Result<Vec<u8>, AppError> {
        let full_url = if target_url.starts_with("http://") || target_url.starts_with("https://") {
            target_url.to_string()
        } else {
            format!("{}{}", self.base_url, target_url)
        };

        debug!("Fetching asset content from {}", full_url);
        let req = self.apply_auth(self.client.get(&full_url));
        let resp = req.send().await?;

        let status = resp.status();
        if !status.is_success() {
            let error_text = resp.text().await.unwrap_or_default();
            debug!("HTTP request error: url={}, status={}, body={}", full_url, status, error_text);
            error!("Failed to download asset from url {}: status {} - {}", full_url, status, error_text);
            return Err(AppError::ComfyApiError(format!(
                "Failed to download asset from {full_url} ({}): {error_text}",
                status
            )));
        }

        let bytes = resp.bytes().await?;
        Ok(bytes.to_vec())
    }
}
