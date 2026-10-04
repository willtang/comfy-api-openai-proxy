use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct GenerateImageRequest {
    pub prompt: String,
    pub model: Option<String>,
    pub n: Option<u32>,
    pub quality: Option<String>,
    pub response_format: Option<String>,
    pub size: Option<String>,
    pub style: Option<String>,
    pub user: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EditImageRequest {
    pub image_bytes: Vec<u8>,
    pub image_filename: String,
    pub prompt: String,
    pub mask_bytes: Option<Vec<u8>>,
    pub mask_filename: Option<String>,
    pub model: Option<String>,
    pub n: Option<u32>,
    pub size: Option<String>,
    pub response_format: Option<String>,
    pub user: Option<String>,
}
