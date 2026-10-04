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
}

impl WorkflowManager {
    pub fn new(txt2img_path: &Path, img2img_path: &Path) -> Self {
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
        }
    }

    pub fn prepare_txt2img(
        &self,
        prompt: &str,
        size_str: Option<&str>,
        checkpoint: Option<&str>,
    ) -> Result<Value, AppError> {
        let mut workflow = self.txt2img_template.clone();
        Self::patch_prompt(&mut workflow, prompt)?;

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
        Self::patch_prompt(&mut workflow, prompt)?;

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

    fn patch_prompt(workflow: &mut Value, prompt: &str) -> Result<(), AppError> {
        let graph = workflow
            .as_object_mut()
            .ok_or_else(|| AppError::BadRequest("Workflow must be a JSON object".to_string()))?;

        let mut patched = false;

        // Try node "6" first (standard positive prompt node)
        if let Some(node) = graph.get_mut("6") {
            if let Some(inputs) = node.get_mut("inputs").and_then(|v| v.as_object_mut()) {
                inputs.insert("text".to_string(), json!(prompt));
                patched = true;
            }
        }

        // Fallback: search for first CLIPTextEncode node that is not negative
        if !patched {
            for (id, node) in graph.iter_mut() {
                if let Some(class_type) = node.get("class_type").and_then(|v| v.as_str()) {
                    if class_type == "CLIPTextEncode" && id != "7" {
                        if let Some(inputs) = node.get_mut("inputs").and_then(|v| v.as_object_mut()) {
                            inputs.insert("text".to_string(), json!(prompt));
                            patched = true;
                            break;
                        }
                    }
                }
            }
        }

        if !patched {
            return Err(AppError::BadRequest(
                "Could not find a CLIPTextEncode node for positive prompt".to_string(),
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
