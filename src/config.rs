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
    pub img_timeout_secs: u64,
    pub vid_timeout_secs: u64,
    pub txt2img_template_path: PathBuf,
    pub img2img_template_path: PathBuf,
    pub txt2vid_template_path: PathBuf,
    pub img2vid_template_path: PathBuf,
    pub txt2img_prompt_node_id: Option<String>,
    pub img2img_prompt_node_id: Option<String>,
    pub txt2vid_prompt_node_id: Option<String>,
    pub txt2vid_seconds_node_id: Option<String>,
    pub txt2vid_fps_node_id: Option<String>,
    pub img2vid_prompt_node_id: Option<String>,
    pub img2vid_image_node_id: Option<String>,
    pub img2vid_seconds_node_id: Option<String>,
    pub img2vid_fps_node_id: Option<String>,
    pub default_checkpoint: Option<String>,
    pub default_video_checkpoint: Option<String>,
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

        let (img_timeout_secs, vid_timeout_secs) =
            resolve_timeouts_from(|k| env::var(k).ok());

        let txt2img_template_path = env::var("TXT2IMG_TEMPLATE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("templates/txt2img.json"));

        let img2img_template_path = env::var("IMG2IMG_TEMPLATE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("templates/img2img.json"));

        let txt2vid_template_path = env::var("TXT2VID_TEMPLATE_PATH")
            .or_else(|_| env::var("TXT2VIDEO_TEMPLATE_PATH"))
            .or_else(|_| env::var("VID_TEMPLATE_PATH"))
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("templates/txt2vid.json"));

        let img2vid_template_path = env::var("IMG2VID_TEMPLATE_PATH")
            .or_else(|_| env::var("I2V_TEMPLATE_PATH"))
            .or_else(|_| env::var("IMAGE2VIDEO_TEMPLATE_PATH"))
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("templates/img2vid.json"));

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

        let txt2vid_prompt_node_id = env::var("TXT2VID_PROMPT_NODE_ID")
            .or_else(|_| env::var("TXT2VID_NODE_ID"))
            .or_else(|_| env::var("VID_PROMPT_NODE_ID"))
            .or_else(|_| env::var("VIDEO_PROMPT_NODE_ID"))
            .or_else(|_| env::var("PROMPT_NODE_ID"))
            .ok()
            .filter(|s| !s.trim().is_empty());

        let img2vid_prompt_node_id = env::var("IMG2VID_PROMPT_NODE_ID")
            .or_else(|_| env::var("IMG2VID_NODE_ID"))
            .or_else(|_| env::var("I2V_PROMPT_NODE_ID"))
            .or_else(|_| env::var("PROMPT_NODE_ID"))
            .ok()
            .filter(|s| !s.trim().is_empty());

        let img2vid_image_node_id = env::var("IMG2VID_IMAGE_NODE_ID")
            .or_else(|_| env::var("I2V_IMAGE_NODE_ID"))
            .or_else(|_| env::var("IMAGE_NODE_ID"))
            .ok()
            .filter(|s| !s.trim().is_empty());

        let global_seconds_node_id = env::var("SECONDS_NODE_ID")
            .or_else(|_| env::var("DURATION_NODE_ID"))
            .ok()
            .filter(|s| !s.trim().is_empty());

        let global_fps_node_id = env::var("FPS_NODE_ID")
            .or_else(|_| env::var("FRAME_RATE_NODE_ID"))
            .ok()
            .filter(|s| !s.trim().is_empty());

        let txt2vid_seconds_node_id = env::var("TXT2VID_SECONDS_NODE_ID")
            .or_else(|_| env::var("TXT2VID_DURATION_NODE_ID"))
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| global_seconds_node_id.clone());

        let txt2vid_fps_node_id = env::var("TXT2VID_FPS_NODE_ID")
            .or_else(|_| env::var("TXT2VID_FRAME_RATE_NODE_ID"))
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| global_fps_node_id.clone());

        let img2vid_seconds_node_id = env::var("IMG2VID_SECONDS_NODE_ID")
            .or_else(|_| env::var("I2V_SECONDS_NODE_ID"))
            .or_else(|_| env::var("IMG2VID_DURATION_NODE_ID"))
            .or_else(|_| env::var("I2V_DURATION_NODE_ID"))
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| global_seconds_node_id.clone());

        let img2vid_fps_node_id = env::var("IMG2VID_FPS_NODE_ID")
            .or_else(|_| env::var("I2V_FPS_NODE_ID"))
            .or_else(|_| env::var("IMG2VID_FRAME_RATE_NODE_ID"))
            .or_else(|_| env::var("I2V_FRAME_RATE_NODE_ID"))
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| global_fps_node_id.clone());

        let default_checkpoint = env::var("DEFAULT_CHECKPOINT")
            .ok()
            .filter(|s| !s.trim().is_empty());

        let default_video_checkpoint = env::var("DEFAULT_VIDEO_CHECKPOINT")
            .or_else(|_| env::var("DEFAULT_VID_CHECKPOINT"))
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| default_checkpoint.clone());

        Self {
            comfy_base_url,
            comfy_api_key,
            openwebui_base_url,
            host,
            port,
            poll_interval_ms,
            img_timeout_secs,
            vid_timeout_secs,
            txt2img_template_path,
            img2img_template_path,
            txt2vid_template_path,
            img2vid_template_path,
            txt2img_prompt_node_id,
            img2img_prompt_node_id,
            txt2vid_prompt_node_id,
            txt2vid_seconds_node_id,
            txt2vid_fps_node_id,
            img2vid_prompt_node_id,
            img2vid_image_node_id,
            img2vid_seconds_node_id,
            img2vid_fps_node_id,
            default_checkpoint,
            default_video_checkpoint,
        }
    }
}

fn resolve_timeouts_from<F>(get_var: F) -> (u64, u64)
where
    F: Fn(&str) -> Option<String>,
{
    let img_timeout_secs = get_var("IMG_TIMEOUT_SECS")
        .or_else(|| get_var("IMAGE_TIMEOUT_SECS"))
        .and_then(|t| t.parse().ok())
        .unwrap_or(180);

    let vid_timeout_secs = get_var("VID_TIMEOUT_SECS")
        .or_else(|| get_var("VIDEO_TIMEOUT_SECS"))
        .and_then(|t| t.parse().ok())
        .unwrap_or(600);

    (img_timeout_secs, vid_timeout_secs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_timeout_defaults() {
        let env_map = HashMap::<&str, &str>::new();
        let (img, vid) = resolve_timeouts_from(|k| env_map.get(k).map(|s| s.to_string()));
        assert_eq!(img, 180);
        assert_eq!(vid, 600);
    }

    #[test]
    fn test_timeout_specific_override() {
        let mut env_map = HashMap::<&str, &str>::new();
        env_map.insert("IMG_TIMEOUT_SECS", "120");
        env_map.insert("VID_TIMEOUT_SECS", "900");
        let (img, vid) = resolve_timeouts_from(|k| env_map.get(k).map(|s| s.to_string()));
        assert_eq!(img, 120);
        assert_eq!(vid, 900);
    }

    #[test]
    fn test_timeout_aliases() {
        let mut env_map = HashMap::<&str, &str>::new();
        env_map.insert("IMAGE_TIMEOUT_SECS", "150");
        env_map.insert("VIDEO_TIMEOUT_SECS", "450");
        let (img, vid) = resolve_timeouts_from(|k| env_map.get(k).map(|s| s.to_string()));
        assert_eq!(img, 150);
        assert_eq!(vid, 450);
    }
}
