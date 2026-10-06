use std::fs;
use std::path::Path;
use rand::Rng;
use serde_json::{json, Value};
use tracing::{debug, info};

use crate::error::AppError;

pub const DEFAULT_TXT2IMG_TEMPLATE: &str = include_str!("../../templates/txt2img.json");
pub const DEFAULT_IMG2IMG_TEMPLATE: &str = include_str!("../../templates/img2img.json");
pub const DEFAULT_TXT2VID_TEMPLATE: &str = include_str!("../../templates/txt2vid.json");
pub const DEFAULT_IMG2VID_TEMPLATE: &str = include_str!("../../templates/img2vid.json");

#[derive(Clone, Debug, Default)]
pub struct WorkflowNodeConfig {
    pub txt2img_prompt_node_id: Option<String>,
    pub img2img_prompt_node_id: Option<String>,
    pub txt2vid_prompt_node_id: Option<String>,
    pub txt2vid_seconds_node_id: Option<String>,
    pub txt2vid_fps_node_id: Option<String>,
    pub img2vid_prompt_node_id: Option<String>,
    pub img2vid_image_node_id: Option<String>,
    pub img2vid_seconds_node_id: Option<String>,
    pub img2vid_fps_node_id: Option<String>,
}

pub struct WorkflowManager {
    txt2img_template: Value,
    img2img_template: Value,
    txt2vid_template: Value,
    img2vid_template: Value,
    pub node_config: WorkflowNodeConfig,
}

impl WorkflowManager {
    pub fn new(
        txt2img_path: &Path,
        img2img_path: &Path,
        txt2vid_path: &Path,
        img2vid_path: &Path,
        node_config: WorkflowNodeConfig,
    ) -> Self {
        let txt2img_template = if txt2img_path.exists() {
            info!("Loading txt2img template from {:?}", txt2img_path);
            let content = fs::read_to_string(txt2img_path)
                .expect("Failed to read txt2img template file");
            serde_json::from_str(&content).expect("Invalid JSON in txt2img template")
        } else {
            info!("Using embedded default txt2img template");
            serde_json::from_str(DEFAULT_TXT2IMG_TEMPLATE).unwrap()
        };

        let img2img_template = if img2img_path.exists() {
            info!("Loading img2img template from {:?}", img2img_path);
            let content = fs::read_to_string(img2img_path)
                .expect("Failed to read img2img template file");
            serde_json::from_str(&content).expect("Invalid JSON in img2img template")
        } else {
            info!("Using embedded default img2img template");
            serde_json::from_str(DEFAULT_IMG2IMG_TEMPLATE).unwrap()
        };

        let txt2vid_template = if txt2vid_path.exists() {
            info!("Loading txt2vid template from {:?}", txt2vid_path);
            let content = fs::read_to_string(txt2vid_path)
                .expect("Failed to read txt2vid template file");
            serde_json::from_str(&content).expect("Invalid JSON in txt2vid template")
        } else if Path::new("templates/txt2vid.json").exists() {
            info!("Loading txt2vid template from templates/txt2vid.json");
            let content = fs::read_to_string("templates/txt2vid.json")
                .expect("Failed to read txt2vid template file");
            serde_json::from_str(&content).expect("Invalid JSON in txt2vid template")
        } else {
            info!("Using embedded default txt2vid template");
            serde_json::from_str(DEFAULT_TXT2VID_TEMPLATE).unwrap()
        };

        let img2vid_template = if img2vid_path.exists() {
            info!("Loading img2vid template from {:?}", img2vid_path);
            let content = fs::read_to_string(img2vid_path)
                .expect("Failed to read img2vid template file");
            serde_json::from_str(&content).expect("Invalid JSON in img2vid template")
        } else if Path::new("templates/img2vid.json").exists() {
            info!("Loading img2vid template from templates/img2vid.json");
            let content = fs::read_to_string("templates/img2vid.json")
                .expect("Failed to read img2vid template file");
            serde_json::from_str(&content).expect("Invalid JSON in img2vid template")
        } else {
            info!("Using embedded default img2vid template");
            serde_json::from_str(DEFAULT_IMG2VID_TEMPLATE).unwrap()
        };

        Self {
            txt2img_template,
            img2img_template,
            txt2vid_template,
            img2vid_template,
            node_config,
        }
    }

    pub fn prepare_txt2img(
        &self,
        prompt: &str,
        size_str: Option<&str>,
        checkpoint: Option<&str>,
    ) -> Result<Value, AppError> {
        let mut workflow = self.txt2img_template.clone();
        Self::patch_prompt(&mut workflow, prompt, self.node_config.txt2img_prompt_node_id.as_deref())?;

        if let Some(size) = size_str {
            let (w, h) = Self::parse_size(size)?;
            Self::patch_size(&mut workflow, w, h);
        }

        Self::patch_seed(&mut workflow);

        if let Some(ckpt) = checkpoint {
            Self::patch_checkpoint(&mut workflow, ckpt);
        }

        Ok(workflow)
    }

    pub fn prepare_img2img(
        &self,
        asset_id: &str,
        prompt: &str,
        size_str: Option<&str>,
        checkpoint: Option<&str>,
    ) -> Result<Value, AppError> {
        let mut workflow = self.img2img_template.clone();

        // 1. Inject core/ASSET reference into LoadImage node
        Self::patch_input_asset(&mut workflow, asset_id)?;

        // 2. Patch prompt
        Self::patch_prompt(&mut workflow, prompt, self.node_config.img2img_prompt_node_id.as_deref())?;

        // 3. Patch dimensions if specified
        if let Some(size) = size_str {
            let (w, h) = Self::parse_size(size)?;
            Self::patch_size(&mut workflow, w, h);
        }

        // 4. Randomize seed
        Self::patch_seed(&mut workflow);

        // 5. Patch checkpoint if provided
        if let Some(ckpt) = checkpoint {
            Self::patch_checkpoint(&mut workflow, ckpt);
        }

        Ok(workflow)
    }

    pub fn prepare_txt2vid(
        &self,
        prompt: &str,
        size_str: Option<&str>,
        checkpoint: Option<&str>,
        seconds: Option<u32>,
        fps: Option<u32>,
    ) -> Result<Value, AppError> {
        let mut workflow = self.txt2vid_template.clone();

        // 1. Patch prompt
        Self::patch_prompt(&mut workflow, prompt, self.node_config.txt2vid_prompt_node_id.as_deref())?;

        // 2. Patch dimensions if specified
        if let Some(size) = size_str {
            let (w, h) = Self::parse_size(size)?;
            Self::patch_size(&mut workflow, w, h);
        }

        // 3. Randomize seed
        Self::patch_seed(&mut workflow);

        // 4. Patch checkpoint if provided
        if let Some(ckpt) = checkpoint {
            Self::patch_checkpoint(&mut workflow, ckpt);
        }

        // 5. Patch duration/fps if specified
        if seconds.is_some() || fps.is_some() {
            Self::patch_video_duration(
                &mut workflow,
                seconds,
                fps,
                self.node_config.txt2vid_seconds_node_id.as_deref(),
                self.node_config.txt2vid_fps_node_id.as_deref(),
            )?;
        }

        Ok(workflow)
    }

    pub fn prepare_img2vid(
        &self,
        asset_id: &str,
        prompt: &str,
        size_str: Option<&str>,
        checkpoint: Option<&str>,
        seconds: Option<u32>,
        fps: Option<u32>,
    ) -> Result<Value, AppError> {
        let mut workflow = self.img2vid_template.clone();

        // 1. Inject core/ASSET reference into LoadImage node (or configured image node)
        Self::patch_input_asset_with_node(&mut workflow, asset_id, self.node_config.img2vid_image_node_id.as_deref())?;

        // 2. Patch prompt
        Self::patch_prompt(&mut workflow, prompt, self.node_config.img2vid_prompt_node_id.as_deref())?;

        // 3. Patch dimensions if specified
        if let Some(size) = size_str {
            let (w, h) = Self::parse_size(size)?;
            Self::patch_size(&mut workflow, w, h);
        }

        // 4. Randomize seed
        Self::patch_seed(&mut workflow);

        // 5. Patch checkpoint if provided
        if let Some(ckpt) = checkpoint {
            Self::patch_checkpoint(&mut workflow, ckpt);
        }

        // 6. Patch duration/fps if specified
        if seconds.is_some() || fps.is_some() {
            Self::patch_video_duration(
                &mut workflow,
                seconds,
                fps,
                self.node_config.img2vid_seconds_node_id.as_deref(),
                self.node_config.img2vid_fps_node_id.as_deref(),
            )?;
        }

        Ok(workflow)
    }

    fn set_node_seconds(node: &mut Value, seconds: u32, fps_opt: Option<u32>) -> bool {
        let Some(inputs) = node.get_mut("inputs").and_then(|v| v.as_object_mut()) else {
            return false;
        };

        if inputs.contains_key("value") {
            inputs.insert("value".to_string(), json!(seconds));
            return true;
        }

        if inputs.contains_key("seconds") {
            inputs.insert("seconds".to_string(), json!(seconds));
            return true;
        }

        if inputs.contains_key("duration") {
            inputs.insert("duration".to_string(), json!(seconds));
            return true;
        }

        let fps = fps_opt.unwrap_or(8);
        let total_frames = seconds * fps;

        if inputs.contains_key("length") {
            inputs.insert("length".to_string(), json!(total_frames));
            return true;
        }

        if inputs.contains_key("frames") {
            inputs.insert("frames".to_string(), json!(total_frames));
            return true;
        }

        if inputs.contains_key("num_frames") {
            inputs.insert("num_frames".to_string(), json!(total_frames));
            return true;
        }

        if inputs.contains_key("frame_count") {
            inputs.insert("frame_count".to_string(), json!(total_frames));
            return true;
        }

        if inputs.contains_key("batch_size") {
            inputs.insert("batch_size".to_string(), json!(total_frames));
            return true;
        }

        inputs.insert("value".to_string(), json!(seconds));
        true
    }

    fn set_node_fps(node: &mut Value, fps_val: u32) -> bool {
        let Some(inputs) = node.get_mut("inputs").and_then(|v| v.as_object_mut()) else {
            return false;
        };

        if inputs.contains_key("value") {
            inputs.insert("value".to_string(), json!(fps_val));
            return true;
        }

        if inputs.contains_key("frame_rate") {
            inputs.insert("frame_rate".to_string(), json!(fps_val));
            return true;
        }

        if inputs.contains_key("fps") {
            inputs.insert("fps".to_string(), json!(fps_val));
            return true;
        }

        inputs.insert("value".to_string(), json!(fps_val));
        true
    }

    fn patch_video_duration(
        workflow: &mut Value,
        seconds: Option<u32>,
        fps_opt: Option<u32>,
        configured_seconds_node_id: Option<&str>,
        configured_fps_node_id: Option<&str>,
    ) -> Result<(), AppError> {
        let graph = workflow
            .as_object_mut()
            .ok_or_else(|| AppError::BadRequest("Workflow must be a JSON object".to_string()))?;

        if let Some(node_id) = configured_seconds_node_id {
            if let Some(secs) = seconds {
                let node = graph.get_mut(node_id).ok_or_else(|| {
                    AppError::BadRequest(format!(
                        "Configured seconds node '{node_id}' not found in workflow template"
                    ))
                })?;

                if !Self::set_node_seconds(node, secs, fps_opt) {
                    return Err(AppError::BadRequest(format!(
                        "Failed to set seconds on configured node '{node_id}': missing or invalid 'inputs' object"
                    )));
                }
                debug!("Updated seconds on configured node '{}'", node_id);
            }
        }

        if let Some(node_id) = configured_fps_node_id {
            if let Some(fps_val) = fps_opt {
                let node = graph.get_mut(node_id).ok_or_else(|| {
                    AppError::BadRequest(format!(
                        "Configured fps node '{node_id}' not found in workflow template"
                    ))
                })?;

                if !Self::set_node_fps(node, fps_val) {
                    return Err(AppError::BadRequest(format!(
                        "Failed to set fps on configured node '{node_id}': missing or invalid 'inputs' object"
                    )));
                }
                debug!("Updated fps on configured node '{}'", node_id);
            }
        }

        for (id, node) in graph.iter_mut() {
            if Some(id.as_str()) == configured_seconds_node_id
                || Some(id.as_str()) == configured_fps_node_id
            {
                continue;
            }

            let meta_title = node
                .get("_meta")
                .and_then(|m| m.get("title"))
                .and_then(|t| t.as_str())
                .map(|t| t.to_lowercase());

            if let Some(inputs) = node.get_mut("inputs").and_then(|v| v.as_object_mut()) {
                if configured_fps_node_id.is_none() {
                    if let Some(fps_val) = fps_opt {
                        if inputs.contains_key("frame_rate") {
                            inputs.insert("frame_rate".to_string(), json!(fps_val));
                        }
                        if inputs.contains_key("fps") {
                            inputs.insert("fps".to_string(), json!(fps_val));
                        }
                    }
                }

                if configured_seconds_node_id.is_none() {
                    if let Some(secs) = seconds {
                        let fps = fps_opt.unwrap_or(8);
                        let total_frames = secs * fps;

                        if inputs.contains_key("length") {
                            inputs.insert("length".to_string(), json!(total_frames));
                        }
                        if inputs.contains_key("frames") {
                            inputs.insert("frames".to_string(), json!(total_frames));
                        }
                        if inputs.contains_key("num_frames") {
                            inputs.insert("num_frames".to_string(), json!(total_frames));
                        }
                        if inputs.contains_key("frame_count") {
                            inputs.insert("frame_count".to_string(), json!(total_frames));
                        }
                    }
                }

                if let Some(ref title_lower) = meta_title {
                    if configured_seconds_node_id.is_none() && title_lower == "duration" {
                        if let Some(secs) = seconds {
                            if inputs.contains_key("value") {
                                inputs.insert("value".to_string(), json!(secs));
                            }
                        }
                    } else if configured_fps_node_id.is_none()
                        && (title_lower == "frame rate" || title_lower == "fps")
                    {
                        if let Some(fps_val) = fps_opt {
                            if inputs.contains_key("value") {
                                inputs.insert("value".to_string(), json!(fps_val));
                            }
                        }
                    }
                }
            }
        }

        if configured_seconds_node_id.is_none() {
            if let Some(secs) = seconds {
                let fps = fps_opt.unwrap_or(8);
                let total_frames = secs * fps;
                if let Some(node) = graph.get_mut("5") {
                    if let Some(inputs) = node.get_mut("inputs").and_then(|v| v.as_object_mut()) {
                        if inputs.contains_key("batch_size") {
                            inputs.insert("batch_size".to_string(), json!(total_frames));
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn patch_input_asset(workflow: &mut Value, asset_id: &str) -> Result<(), AppError> {
        Self::patch_input_asset_with_node(workflow, asset_id, None)
    }

    fn patch_input_asset_with_node(
        workflow: &mut Value,
        asset_id: &str,
        configured_node_id: Option<&str>,
    ) -> Result<(), AppError> {
        let graph = workflow
            .as_object_mut()
            .ok_or_else(|| AppError::BadRequest("Workflow must be a JSON object".to_string()))?;

        let asset_ref = json!({
            "__type": "core/ASSET",
            "info": {
                "id": asset_id
            }
        });

        // 0. If a specific image node ID is configured, patch that node directly
        if let Some(node_id) = configured_node_id {
            let node = graph.get_mut(node_id).ok_or_else(|| {
                AppError::BadRequest(format!(
                    "Configured image node '{node_id}' not found in workflow template"
                ))
            })?;

            if let Some(inputs) = node.get_mut("inputs").and_then(|v| v.as_object_mut()) {
                inputs.insert("image".to_string(), asset_ref);
                debug!("Injected core/ASSET reference on configured node '{}'", node_id);
                return Ok(());
            } else {
                return Err(AppError::BadRequest(format!(
                    "Failed to set image asset on configured node '{node_id}': missing or invalid 'inputs' object"
                )));
            }
        }

        // 1. Check node "1" first (standard LoadImage node)
        let mut patched = false;
        if let Some(node) = graph.get_mut("1") {
            if let Some(class_type) = node.get("class_type").and_then(|v| v.as_str()) {
                if class_type == "LoadImage" {
                    if let Some(inputs) = node.get_mut("inputs").and_then(|v| v.as_object_mut()) {
                        inputs.insert("image".to_string(), asset_ref.clone());
                        patched = true;
                    }
                }
            }
        }

        // 2. Check node "395" (common in LTX-Video templates)
        if !patched {
            if let Some(node) = graph.get_mut("395") {
                if let Some(class_type) = node.get("class_type").and_then(|v| v.as_str()) {
                    if class_type == "LoadImage" {
                        if let Some(inputs) = node.get_mut("inputs").and_then(|v| v.as_object_mut()) {
                            inputs.insert("image".to_string(), asset_ref.clone());
                            patched = true;
                        }
                    }
                }
            }
        }

        // 3. Fallback: search for any LoadImage node
        if !patched {
            for (_id, node) in graph.iter_mut() {
                if let Some(class_type) = node.get("class_type").and_then(|v| v.as_str()) {
                    if class_type == "LoadImage" {
                        if let Some(inputs) = node.get_mut("inputs").and_then(|v| v.as_object_mut()) {
                            inputs.insert("image".to_string(), asset_ref.clone());
                            patched = true;
                            break;
                        }
                    }
                }
            }
        }

        if !patched {
            return Err(AppError::BadRequest(
                "Could not find a LoadImage node in workflow template to attach asset".to_string(),
            ));
        }

        debug!("Injected core/ASSET reference for asset_id={}", asset_id);
        Ok(())
    }

    fn set_node_prompt(node: &mut Value, prompt: &str) -> bool {
        let is_clip_text_encode = node
            .get("class_type")
            .and_then(|v| v.as_str()) == Some("CLIPTextEncode");

        let Some(inputs) = node.get_mut("inputs").and_then(|v| v.as_object_mut()) else {
            return false;
        };

        // If inputs has a string "prompt" field, update it
        if let Some(val) = inputs.get("prompt") {
            if val.is_string() {
                inputs.insert("prompt".to_string(), json!(prompt));
                return true;
            }
        }

        // If inputs has a string "text" field, update it
        if let Some(val) = inputs.get("text") {
            if val.is_string() {
                inputs.insert("text".to_string(), json!(prompt));
                return true;
            }
        }

        // If inputs has a string "value" field, update it (e.g. PrimitiveStringMultiline)
        if let Some(val) = inputs.get("value") {
            if val.is_string() {
                inputs.insert("value".to_string(), json!(prompt));
                return true;
            }
        }

        // If inputs has a "prompt" key (e.g. placeholder value)
        if inputs.contains_key("prompt") {
            inputs.insert("prompt".to_string(), json!(prompt));
            return true;
        }

        // If inputs has a "text" key
        if inputs.contains_key("text") {
            inputs.insert("text".to_string(), json!(prompt));
            return true;
        }

        // If inputs has a "value" key
        if inputs.contains_key("value") {
            inputs.insert("value".to_string(), json!(prompt));
            return true;
        }

        // Fallback based on class_type
        if is_clip_text_encode {
            inputs.insert("text".to_string(), json!(prompt));
            return true;
        }

        // Default to "prompt"
        inputs.insert("prompt".to_string(), json!(prompt));
        true
    }

    fn patch_prompt(
        workflow: &mut Value,
        prompt: &str,
        configured_node_id: Option<&str>,
    ) -> Result<(), AppError> {
        let graph = workflow
            .as_object_mut()
            .ok_or_else(|| AppError::BadRequest("Workflow must be a JSON object".to_string()))?;

        // If a specific prompt node ID is configured, patch that node directly
        if let Some(node_id) = configured_node_id {
            let node = graph.get_mut(node_id).ok_or_else(|| {
                AppError::BadRequest(format!(
                    "Configured prompt node '{node_id}' not found in workflow template"
                ))
            })?;

            if !Self::set_node_prompt(node, prompt) {
                return Err(AppError::BadRequest(format!(
                    "Failed to set prompt on configured node '{node_id}': missing or invalid 'inputs' object"
                )));
            }
            debug!("Updated prompt on configured node '{}'", node_id);
            return Ok(());
        }

        let mut patched = false;

        // 1. Try node "6" first (standard positive prompt node) if it's a prompt node
        if let Some(node) = graph.get_mut("6") {
            let is_prompt_node = (node
                .get("class_type")
                .and_then(|v| v.as_str()) == Some("CLIPTextEncode"))
                || node
                    .get("inputs")
                    .and_then(|i| i.get("text").or_else(|| i.get("prompt")).or_else(|| i.get("value")))
                    .is_some_and(|v| v.is_string());

            if is_prompt_node {
                patched = Self::set_node_prompt(node, prompt);
            }
        }

        // 2. Fallback: search for first CLIPTextEncode node that has string text and is not negative
        if !patched {
            for (id, node) in graph.iter_mut() {
                if let Some(class_type) = node.get("class_type").and_then(|v| v.as_str()) {
                    if class_type == "CLIPTextEncode" && id != "7" {
                        let has_string_text = node
                            .get("inputs")
                            .and_then(|i| i.get("text"))
                            .is_some_and(|v| v.is_string());

                        if has_string_text && Self::set_node_prompt(node, prompt) {
                            patched = true;
                            break;
                        }
                    }
                }
            }
        }

        // 3. Fallback: search for any non-negative node with string prompt/text/value input
        if !patched {
            for (id, node) in graph.iter_mut() {
                if id == "7" {
                    continue;
                }

                let is_negative = node
                    .get("_meta")
                    .and_then(|m| m.get("title"))
                    .and_then(|t| t.as_str())
                    .is_some_and(|t| t.to_lowercase().contains("negative"));

                if is_negative {
                    continue;
                }

                let has_string_prompt = node
                    .get("inputs")
                    .and_then(|i| i.get("prompt").or_else(|| i.get("text")).or_else(|| i.get("value")))
                    .is_some_and(|v| v.is_string());

                if has_string_prompt && Self::set_node_prompt(node, prompt) {
                    patched = true;
                    break;
                }
            }
        }

        if !patched {
            return Err(AppError::BadRequest(
                "Could not find a prompt node for positive prompt".to_string(),
            ));
        }

        Ok(())
    }

    fn patch_size(workflow: &mut Value, width: u32, height: u32) {
        if let Some(graph) = workflow.as_object_mut() {
            // Check node "5" or search for EmptyLatentImage
            if let Some(node) = graph.get_mut("5") {
                if let Some(inputs) = node.get_mut("inputs").and_then(|v| v.as_object_mut()) {
                    inputs.insert("width".to_string(), json!(width));
                    inputs.insert("height".to_string(), json!(height));
                    return;
                }
            }

            for (_id, node) in graph.iter_mut() {
                if let Some(class_type) = node.get("class_type").and_then(|v| v.as_str()) {
                    if class_type == "EmptyLatentImage" {
                        if let Some(inputs) = node.get_mut("inputs").and_then(|v| v.as_object_mut()) {
                            inputs.insert("width".to_string(), json!(width));
                            inputs.insert("height".to_string(), json!(height));
                            return;
                        }
                    }
                }
            }
        }
    }

    fn patch_seed(workflow: &mut Value) {
        let mut rng = rand::thread_rng();
        let random_seed: u64 = rng.gen_range(1..999_999_999_999_999);

        if let Some(graph) = workflow.as_object_mut() {
            for (_id, node) in graph.iter_mut() {
                if let Some(class_type) = node.get("class_type").and_then(|v| v.as_str()) {
                    if class_type.contains("KSampler") || class_type == "RandomNoise" {
                        if let Some(inputs) = node.get_mut("inputs").and_then(|v| v.as_object_mut()) {
                            if inputs.contains_key("seed") {
                                inputs.insert("seed".to_string(), json!(random_seed));
                            }
                            if inputs.contains_key("noise_seed") {
                                inputs.insert("noise_seed".to_string(), json!(random_seed));
                            }
                        }
                    }
                }
            }
        }
    }

    fn patch_checkpoint(workflow: &mut Value, checkpoint_name: &str) {
        if let Some(graph) = workflow.as_object_mut() {
            for (_id, node) in graph.iter_mut() {
                if let Some(class_type) = node.get("class_type").and_then(|v| v.as_str()) {
                    if class_type == "CheckpointLoaderSimple" {
                        if let Some(inputs) = node.get_mut("inputs").and_then(|v| v.as_object_mut()) {
                            inputs.insert("ckpt_name".to_string(), json!(checkpoint_name));
                        }
                    }
                }
            }
        }
    }

    fn parse_size(size_str: &str) -> Result<(u32, u32), AppError> {
        let parts: Vec<&str> = size_str.split('x').collect();
        if parts.len() != 2 {
            return Err(AppError::BadRequest(format!(
                "Invalid size format '{}'. Expected format 'WIDTHxHEIGHT', e.g. '1024x1024'",
                size_str
            )));
        }

        let width = parts[0]
            .parse::<u32>()
            .map_err(|_| AppError::BadRequest(format!("Invalid width in size '{size_str}'")))?;
        let height = parts[1]
            .parse::<u32>()
            .map_err(|_| AppError::BadRequest(format!("Invalid height in size '{size_str}'")))?;

        Ok((width, height))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_default_txt2img_prompt_patching() {
        let manager = WorkflowManager::new(
            &PathBuf::from("non_existent_file.json"),
            &PathBuf::from("non_existent_file.json"),
            &PathBuf::from("non_existent_file.json"),
            &PathBuf::from("non_existent_file.json"),
            WorkflowNodeConfig::default(),
        );

        let workflow = manager
            .prepare_txt2img("a scenic landscape", None, None)
            .expect("prepare_txt2img should succeed");

        assert_eq!(
            workflow["6"]["inputs"]["text"],
            "a scenic landscape"
        );
    }

    #[test]
    fn test_configured_node_id_with_prompt_field() {
        let custom_template = json!({
            "459:471": {
                "class_type": "TextGenerate",
                "inputs": {
                    "prompt": "OLD PROMPT",
                    "max_length": 16256
                }
            },
            "other_node": {
                "class_type": "Other",
                "inputs": {}
            }
        });

        let mut manager = WorkflowManager::new(
            &PathBuf::from("non_existent.json"),
            &PathBuf::from("non_existent.json"),
            &PathBuf::from("non_existent.json"),
            &PathBuf::from("non_existent.json"),
            WorkflowNodeConfig {
                txt2img_prompt_node_id: Some("459:471".to_string()),
                ..Default::default()
            },
        );
        manager.txt2img_template = custom_template;

        let workflow = manager
            .prepare_txt2img("brand new prompt", None, None)
            .expect("prepare_txt2img should succeed");

        assert_eq!(
            workflow["459:471"]["inputs"]["prompt"],
            "brand new prompt"
        );
        assert_eq!(
            workflow["459:471"]["inputs"]["max_length"],
            16256
        );
    }

    #[test]
    fn test_configured_node_id_not_found() {
        let custom_template = json!({
            "1": {
                "class_type": "SomeNode",
                "inputs": {}
            }
        });

        let mut manager = WorkflowManager::new(
            &PathBuf::from("non_existent.json"),
            &PathBuf::from("non_existent.json"),
            &PathBuf::from("non_existent.json"),
            &PathBuf::from("non_existent.json"),
            WorkflowNodeConfig {
                txt2img_prompt_node_id: Some("non_existent_node".to_string()),
                ..Default::default()
            },
        );
        manager.txt2img_template = custom_template;

        let err = manager
            .prepare_txt2img("prompt", None, None)
            .unwrap_err();

        match err {
            AppError::BadRequest(msg) => {
                assert!(msg.contains("Configured prompt node 'non_existent_node' not found"));
            }
            _ => panic!("Expected BadRequest error"),
        }
    }

    #[test]
    fn test_fallback_node_with_prompt_field() {
        let custom_template = json!({
            "459:452": {
                "class_type": "TextEncodeQwenImage21",
                "inputs": {
                    "prompt": ["459:472", 0]
                }
            },
            "459:471": {
                "class_type": "TextGenerate",
                "inputs": {
                    "prompt": "OLD PROMPT",
                    "max_length": 16256
                }
            }
        });

        let mut manager = WorkflowManager::new(
            &PathBuf::from("non_existent.json"),
            &PathBuf::from("non_existent.json"),
            &PathBuf::from("non_existent.json"),
            &PathBuf::from("non_existent.json"),
            WorkflowNodeConfig::default(),
        );
        manager.txt2img_template = custom_template;

        let workflow = manager
            .prepare_txt2img("fallback detected prompt", None, None)
            .expect("prepare_txt2img should succeed");

        assert_eq!(
            workflow["459:471"]["inputs"]["prompt"],
            "fallback detected prompt"
        );
    }

    #[test]
    fn test_qwen_template_file_with_node_id() {
        let qwen_path = PathBuf::from("templates/image_qwen_image_2_1_t2i.json");
        if qwen_path.exists() {
            let manager = WorkflowManager::new(
                &qwen_path,
                &PathBuf::from("templates/img2img.json"),
                &PathBuf::from("templates/txt2vid.json"),
                &PathBuf::from("templates/img2vid.json"),
                WorkflowNodeConfig {
                    txt2img_prompt_node_id: Some("459:471".to_string()),
                    ..Default::default()
                },
            );

            let workflow = manager
                .prepare_txt2img("cinematic photo of a cyberpunk street", None, None)
                .expect("prepare_txt2img should succeed");

            assert_eq!(
                workflow["459:471"]["inputs"]["prompt"],
                "cinematic photo of a cyberpunk street"
            );
        }
    }

    #[test]
    fn test_default_txt2vid_preparation() {
        let manager = WorkflowManager::new(
            &PathBuf::from("non_existent.json"),
            &PathBuf::from("non_existent.json"),
            &PathBuf::from("templates/txt2vid.json"),
            &PathBuf::from("templates/img2vid.json"),
            WorkflowNodeConfig::default(),
        );

        let workflow = manager
            .prepare_txt2vid("A drone flying through a city", Some("720x480"), Some("custom-video-model.safetensors"), Some(4), Some(16))
            .expect("prepare_txt2vid should succeed");

        assert_eq!(
            workflow["6"]["inputs"]["text"],
            "A drone flying through a city"
        );
        assert_eq!(workflow["5"]["inputs"]["width"], 720);
        assert_eq!(workflow["5"]["inputs"]["height"], 480);
        assert_eq!(workflow["5"]["inputs"]["batch_size"], 64); // 4 secs * 16 fps
        assert_eq!(workflow["9"]["inputs"]["frame_rate"], 16);
        assert_eq!(workflow["4"]["inputs"]["ckpt_name"], "custom-video-model.safetensors");
    }

    #[test]
    fn test_txt2vid_custom_prompt_node() {
        let manager = WorkflowManager::new(
            &PathBuf::from("non_existent.json"),
            &PathBuf::from("non_existent.json"),
            &PathBuf::from("templates/txt2vid.json"),
            &PathBuf::from("templates/img2vid.json"),
            WorkflowNodeConfig {
                txt2vid_prompt_node_id: Some("6".to_string()),
                ..Default::default()
            },
        );

        let workflow = manager
            .prepare_txt2vid("custom node prompt test", None, None, None, None)
            .expect("prepare_txt2vid should succeed");

        assert_eq!(workflow["6"]["inputs"]["text"], "custom node prompt test");
    }

    #[test]
    fn test_txt2vid_seconds_and_fps_configured_nodes() {
        let custom_template = json!({
            "prompt_node": {
                "class_type": "CLIPTextEncode",
                "inputs": { "text": "placeholder" }
            },
            "sec_node": {
                "class_type": "PrimitiveInt",
                "inputs": { "value": 1 }
            },
            "fps_node": {
                "class_type": "PrimitiveInt",
                "inputs": { "value": 8 }
            }
        });

        let mut manager = WorkflowManager::new(
            &PathBuf::from("non_existent.json"),
            &PathBuf::from("non_existent.json"),
            &PathBuf::from("templates/txt2vid.json"),
            &PathBuf::from("templates/img2vid.json"),
            WorkflowNodeConfig {
                txt2vid_prompt_node_id: Some("prompt_node".to_string()),
                txt2vid_seconds_node_id: Some("sec_node".to_string()),
                txt2vid_fps_node_id: Some("fps_node".to_string()),
                ..Default::default()
            },
        );
        manager.txt2vid_template = custom_template;

        let workflow = manager
            .prepare_txt2vid("flying bird", None, None, Some(7), Some(30))
            .expect("prepare_txt2vid should succeed");

        assert_eq!(workflow["sec_node"]["inputs"]["value"], 7);
        assert_eq!(workflow["fps_node"]["inputs"]["value"], 30);
    }

    #[test]
    fn test_default_img2vid_preparation() {
        let manager = WorkflowManager::new(
            &PathBuf::from("non_existent.json"),
            &PathBuf::from("non_existent.json"),
            &PathBuf::from("templates/txt2vid.json"),
            &PathBuf::from("templates/img2vid.json"),
            WorkflowNodeConfig::default(),
        );

        let workflow = manager
            .prepare_img2vid("asset_abc123", "Robot walking down neon street", None, Some("model.ckpt"), Some(3), Some(12))
            .expect("prepare_img2vid should succeed");

        assert_eq!(
            workflow["1"]["inputs"]["image"]["__type"],
            "core/ASSET"
        );
        assert_eq!(
            workflow["1"]["inputs"]["image"]["info"]["id"],
            "asset_abc123"
        );
        assert_eq!(
            workflow["6"]["inputs"]["text"],
            "Robot walking down neon street"
        );
        assert_eq!(
            workflow["4"]["inputs"]["ckpt_name"],
            "model.ckpt"
        );
        assert_eq!(
            workflow["9"]["inputs"]["frame_rate"],
            12
        );
    }

    #[test]
    fn test_img2vid_custom_nodes() {
        let custom_template = json!({
            "image_in": {
                "class_type": "LoadImage",
                "inputs": {
                    "image": "foo.png"
                }
            },
            "prompt_in": {
                "class_type": "CLIPTextEncode",
                "inputs": {
                    "text": "old prompt"
                }
            },
            "dur_in": {
                "class_type": "PrimitiveInt",
                "inputs": {
                    "value": 2
                }
            },
            "fps_in": {
                "class_type": "PrimitiveInt",
                "inputs": {
                    "value": 10
                }
            }
        });

        let mut manager = WorkflowManager::new(
            &PathBuf::from("non_existent.json"),
            &PathBuf::from("non_existent.json"),
            &PathBuf::from("templates/txt2vid.json"),
            &PathBuf::from("non_existent.json"),
            WorkflowNodeConfig {
                img2vid_prompt_node_id: Some("prompt_in".to_string()),
                img2vid_image_node_id: Some("image_in".to_string()),
                img2vid_seconds_node_id: Some("dur_in".to_string()),
                img2vid_fps_node_id: Some("fps_in".to_string()),
                ..Default::default()
            },
        );
        manager.img2vid_template = custom_template;

        let workflow = manager
            .prepare_img2vid("asset_999", "new futuristic prompt", None, None, Some(6), Some(25))
            .expect("prepare_img2vid should succeed");

        assert_eq!(
            workflow["image_in"]["inputs"]["image"]["info"]["id"],
            "asset_999"
        );
        assert_eq!(
            workflow["prompt_in"]["inputs"]["text"],
            "new futuristic prompt"
        );
        assert_eq!(workflow["dur_in"]["inputs"]["value"], 6);
        assert_eq!(workflow["fps_in"]["inputs"]["value"], 25);
    }

    #[test]
    fn test_ltx2_5_i2v_template_file() {
        let ltx_path = PathBuf::from("templates/video_ltx2_5_i2v.json");
        if ltx_path.exists() {
            let manager = WorkflowManager::new(
                &PathBuf::from("templates/txt2img.json"),
                &PathBuf::from("templates/img2img.json"),
                &PathBuf::from("templates/txt2vid.json"),
                &ltx_path,
                WorkflowNodeConfig {
                    img2vid_prompt_node_id: Some("398:376".to_string()),
                    img2vid_image_node_id: Some("395".to_string()),
                    img2vid_seconds_node_id: Some("398:362".to_string()),
                    img2vid_fps_node_id: Some("398:361".to_string()),
                    ..Default::default()
                },
            );

            let workflow = manager
                .prepare_img2vid("asset_ltx_test", "Futuristic cyberpunk portrait animation", None, None, Some(5), Some(24))
                .expect("prepare_img2vid should succeed for ltx2_5");

            assert_eq!(
                workflow["395"]["inputs"]["image"]["info"]["id"],
                "asset_ltx_test"
            );
            assert_eq!(
                workflow["398:376"]["inputs"]["value"],
                "Futuristic cyberpunk portrait animation"
            );
            assert_eq!(
                workflow["398:362"]["inputs"]["value"],
                5
            );
            assert_eq!(
                workflow["398:361"]["inputs"]["value"],
                24
            );
        }
    }
}


