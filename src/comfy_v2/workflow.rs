use std::fs;
use std::path::Path;
use rand::Rng;
use serde_json::{json, Value};
use tracing::{debug, info};

use crate::error::AppError;

pub const DEFAULT_TXT2IMG_TEMPLATE: &str = include_str!("../../templates/txt2img.json");
pub const DEFAULT_IMG2IMG_TEMPLATE: &str = include_str!("../../templates/img2img.json");

pub struct WorkflowManager {
    txt2img_template: Value,
    img2img_template: Value,
    txt2img_prompt_node_id: Option<String>,
    img2img_prompt_node_id: Option<String>,
}

impl WorkflowManager {
    pub fn new(
        txt2img_path: &Path,
        img2img_path: &Path,
        txt2img_prompt_node_id: Option<String>,
        img2img_prompt_node_id: Option<String>,
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

        Self {
            txt2img_template,
            img2img_template,
            txt2img_prompt_node_id,
            img2img_prompt_node_id,
        }
    }

    pub fn prepare_txt2img(
        &self,
        prompt: &str,
        size_str: Option<&str>,
        checkpoint: Option<&str>,
    ) -> Result<Value, AppError> {
        let mut workflow = self.txt2img_template.clone();
        Self::patch_prompt(&mut workflow, prompt, self.txt2img_prompt_node_id.as_deref())?;

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
        Self::patch_prompt(&mut workflow, prompt, self.img2img_prompt_node_id.as_deref())?;

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

    fn patch_input_asset(workflow: &mut Value, asset_id: &str) -> Result<(), AppError> {
        let graph = workflow
            .as_object_mut()
            .ok_or_else(|| AppError::BadRequest("Workflow must be a JSON object".to_string()))?;

        let asset_ref = json!({
            "__type": "core/ASSET",
            "info": {
                "id": asset_id
            }
        });

        // Check node "1" first, or any node with class_type LoadImage
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
            .and_then(|v| v.as_str())
            .map_or(false, |s| s == "CLIPTextEncode");

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
            let is_prompt_node = node
                .get("class_type")
                .and_then(|v| v.as_str())
                .map_or(false, |c| c == "CLIPTextEncode")
                || node
                    .get("inputs")
                    .and_then(|i| i.get("text").or_else(|| i.get("prompt")))
                    .map_or(false, |v| v.is_string());

            if is_prompt_node {
                patched = Self::set_node_prompt(node, prompt);
            }
        }

        // 2. Fallback: search for first CLIPTextEncode node that is not negative
        if !patched {
            for (id, node) in graph.iter_mut() {
                if let Some(class_type) = node.get("class_type").and_then(|v| v.as_str()) {
                    if class_type == "CLIPTextEncode" && id != "7" {
                        if Self::set_node_prompt(node, prompt) {
                            patched = true;
                            break;
                        }
                    }
                }
            }
        }

        // 3. Fallback: search for any non-negative node with string prompt/text input
        if !patched {
            for (id, node) in graph.iter_mut() {
                if id == "7" {
                    continue;
                }

                let is_negative = node
                    .get("_meta")
                    .and_then(|m| m.get("title"))
                    .and_then(|t| t.as_str())
                    .map_or(false, |t| t.to_lowercase().contains("negative"));

                if is_negative {
                    continue;
                }

                let has_string_prompt = node
                    .get("inputs")
                    .and_then(|i| i.get("prompt").or_else(|| i.get("text")))
                    .map_or(false, |v| v.is_string());

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
                    if class_type.contains("KSampler") {
                        if let Some(inputs) = node.get_mut("inputs").and_then(|v| v.as_object_mut()) {
                            inputs.insert("seed".to_string(), json!(random_seed));
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
            None,
            None,
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
            Some("459:471".to_string()),
            None,
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
            Some("non_existent_node".to_string()),
            None,
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
            None,
            None,
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
                Some("459:471".to_string()),
                None,
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
}

