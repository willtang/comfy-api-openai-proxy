use std::env;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub comfy_base_url: String,
    pub comfy_api_key: Option<String>,
    pub openwebui_base_url: Option<String>,
    pub host: String,
    pub port: u16,
    pub poll_interval_ms: u64,
    pub job_timeout_secs: u64,
    pub txt2img_template_path: PathBuf,
    pub img2img_template_path: PathBuf,
    pub txt2img_prompt_node_id: Option<String>,
    pub img2img_prompt_node_id: Option<String>,
    pub default_checkpoint: Option<String>,
}

impl AppConfig {
    pub fn from_env() -> Self {
        // Load variables from .env file if present
        dotenvy::dotenv().ok();

        let comfy_base_url = env::var("COMFY_BASE_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8189".to_string())
            .trim_end_matches('/')
            .to_string();

        let comfy_api_key = env::var("COMFY_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty());

        let openwebui_base_url = env::var("OPENWEBUI_BASE_URL")
            .or_else(|_| env::var("WEBUI_BASE_URL"))
            .or_else(|_| env::var("IMAGE_BASE_URL"))
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim_end_matches('/').to_string());

        let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let port = env::var("PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(8190);

        let poll_interval_ms = env::var("POLL_INTERVAL_MS")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(500);

        let job_timeout_secs = env::var("JOB_TIMEOUT_SECS")
            .ok()
            .and_then(|t| t.parse().ok())
            .unwrap_or(180);

        let txt2img_template_path = env::var("TXT2IMG_TEMPLATE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("templates/txt2img.json"));

        let img2img_template_path = env::var("IMG2IMG_TEMPLATE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("templates/img2img.json"));

        let txt2img_prompt_node_id = env::var("TXT2IMG_PROMPT_NODE_ID")
            .or_else(|_| env::var("TXT2IMG_NODE_ID"))
            .or_else(|_| env::var("PROMPT_NODE_ID"))
            .ok()
            .filter(|s| !s.trim().is_empty());

        let img2img_prompt_node_id = env::var("IMG2IMG_PROMPT_NODE_ID")
            .or_else(|_| env::var("IMG2IMG_NODE_ID"))
            .or_else(|_| env::var("PROMPT_NODE_ID"))
            .ok()
            .filter(|s| !s.trim().is_empty());

        let default_checkpoint = env::var("DEFAULT_CHECKPOINT")
            .ok()
            .filter(|s| !s.trim().is_empty());

        Self {
            comfy_base_url,
            comfy_api_key,
            openwebui_base_url,
            host,
            port,
            poll_interval_ms,
            job_timeout_secs,
            txt2img_template_path,
            img2img_template_path,
            txt2img_prompt_node_id,
            img2img_prompt_node_id,
            default_checkpoint,
        }
    }
}
