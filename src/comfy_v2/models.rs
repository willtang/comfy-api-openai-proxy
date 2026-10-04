use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct AssetUploadResponse {
    pub id: String,
    pub hash: Option<String>,
    pub size_bytes: Option<u64>,
    pub content_type: Option<String>,
    pub created_new: Option<bool>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct JobSubmitRequest {
    pub workflow: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JobUrls {
    #[serde(rename = "self")]
    pub self_url: Option<String>,
    pub events: Option<String>,
    pub cancel: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JobOutput {
    pub node_id: Option<String>,
    pub name: Option<String>,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JobStatusResponse {
    pub id: String,
    pub status: String,
    pub urls: Option<JobUrls>,
    pub outputs: Option<Vec<JobOutput>>,
    pub error: Option<serde_json::Value>,
}

impl JobStatusResponse {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status.as_str(),
            "succeeded" | "failed" | "expired" | "canceled"
        )
    }

    pub fn is_success(&self) -> bool {
        self.status == "succeeded"
    }
}
