use serde::{Deserialize, Deserializer};
use serde_json::Value;

#[derive(Debug, Clone)]
pub enum ImageInput {
    String(String),
    UrlObject { url: String },
    B64Object { b64_json: String },
}

impl ImageInput {
    pub fn to_url_or_b64(&self) -> String {
        match self {
            ImageInput::String(s) => s.clone(),
            ImageInput::UrlObject { url } => url.clone(),
            ImageInput::B64Object { b64_json } => format!("data:image/png;base64,{}", b64_json),
        }
    }
}

impl<'de> Deserialize<'de> for ImageInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = Value::deserialize(deserializer)?;
        if let Some(s) = v.as_str() {
            Ok(ImageInput::String(s.to_string()))
        } else if let Some(obj) = v.as_object() {
            if let Some(url) = obj.get("url").and_then(|u| u.as_str()) {
                Ok(ImageInput::UrlObject {
                    url: url.to_string(),
                })
            } else if let Some(b64) = obj.get("b64_json").and_then(|b| b.as_str()) {
                Ok(ImageInput::B64Object {
                    b64_json: b64.to_string(),
                })
            } else {
                Err(serde::de::Error::custom(
                    "Expected object with 'url' or 'b64_json' key",
                ))
            }
        } else {
            Err(serde::de::Error::custom(
                "Expected string or object for image input",
            ))
        }
    }
}

#[derive(Debug, Clone)]
pub enum FlexibleStringOrVec {
    Single(ImageInput),
    Vec(Vec<ImageInput>),
}

impl FlexibleStringOrVec {
    pub fn first(&self) -> Option<String> {
        match self {
            FlexibleStringOrVec::Single(input) => Some(input.to_url_or_b64()),
            FlexibleStringOrVec::Vec(v) => v.first().map(|input| input.to_url_or_b64()),
        }
    }
}

impl<'de> Deserialize<'de> for FlexibleStringOrVec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = Value::deserialize(deserializer)?;
        if let Ok(single) = serde_json::from_value::<ImageInput>(v.clone()) {
            Ok(FlexibleStringOrVec::Single(single))
        } else if let Ok(vec) = serde_json::from_value::<Vec<ImageInput>>(v) {
            Ok(FlexibleStringOrVec::Vec(vec))
        } else {
            Err(serde::de::Error::custom(
                "Expected string, image object, or array of strings/objects",
            ))
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct UnifiedImageRequest {
    pub prompt: String,
    pub model: Option<String>,
    pub n: Option<u32>,
    pub quality: Option<String>,
    pub response_format: Option<String>,
    pub size: Option<String>,
    pub style: Option<String>,
    pub user: Option<String>,

    // Support image_urls, image_url, images, image as passed by Open WebUI and various OpenAI client apps
    pub image_urls: Option<FlexibleStringOrVec>,
    pub image_url: Option<FlexibleStringOrVec>,
    pub images: Option<FlexibleStringOrVec>,
    pub image: Option<FlexibleStringOrVec>,

    pub mask: Option<FlexibleStringOrVec>,
    pub mask_url: Option<FlexibleStringOrVec>,
}

impl UnifiedImageRequest {
    pub fn get_input_image(&self) -> Option<String> {
        self.image_urls
            .as_ref()
            .and_then(|v| v.first())
            .or_else(|| self.image_url.as_ref().and_then(|v| v.first()))
            .or_else(|| self.images.as_ref().and_then(|v| v.first()))
            .or_else(|| self.image.as_ref().and_then(|v| v.first()))
            .filter(|s| !s.trim().is_empty())
    }
}

pub type GenerateImageRequest = UnifiedImageRequest;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_openwebui_image_urls() {
        let json_str = r#"{
            "image_urls": ["/api/v1/files/ab0f71c7-ab09-4a5d-a41c-edb5b704ac0c/content"],
            "prompt": "Remove the beach ball from the image entirely."
        }"#;

        let req: UnifiedImageRequest =
            serde_json::from_str(json_str).expect("Should deserialize Open WebUI image_urls format");
        assert_eq!(req.prompt, "Remove the beach ball from the image entirely.");
        assert_eq!(
            req.get_input_image(),
            Some("/api/v1/files/ab0f71c7-ab09-4a5d-a41c-edb5b704ac0c/content".to_string())
        );
    }

    #[test]
    fn test_deserialize_single_image_url_and_objects() {
        let json_str1 = r#"{"prompt": "test", "image_url": "/path/to/img.png"}"#;
        let req1: UnifiedImageRequest = serde_json::from_str(json_str1).unwrap();
        assert_eq!(req1.get_input_image(), Some("/path/to/img.png".to_string()));

        let json_str2 = r#"{"prompt": "test", "images": [{"url": "http://example.com/a.jpg"}]}"#;
        let req2: UnifiedImageRequest = serde_json::from_str(json_str2).unwrap();
        assert_eq!(req2.get_input_image(), Some("http://example.com/a.jpg".to_string()));
    }
}
