pub mod anthropic;
pub mod gemini;
pub mod google_free;
pub mod openai_compat;

use crate::models::{ModelProfile, PromptPreset, ProviderKind, language_display_name};
use reqwest::Client;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Clone)]
pub enum TranslateInput {
    Text(String),
    ImagePng {
        bytes: Vec<u8>,
        context_hint: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct TranslateRequest {
    pub input: TranslateInput,
    pub source_lang: String,
    pub target_lang: String,
    pub prompt: PromptPreset,
}

#[derive(Debug, Clone)]
pub struct TranslateResponse {
    pub translated_text: String,
    pub detected_source_lang: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCapabilityProbe {
    pub reachable: bool,
    pub detected_vision: Option<bool>,
    pub detail: String,
}

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("Active model '{model_name}' does not support image/screenshot input")]
    VisionUnsupported { model_name: String },
    #[error("API key is required for model '{model_name}'")]
    MissingApiKey { model_name: String },
    #[error("Rate limited by provider (HTTP 429)")]
    RateLimited,
    #[error("Network request failed: {0}")]
    Network(String),
    #[error("Provider returned HTTP {status}: {message}")]
    ApiError { status: u16, message: String },
    #[error("Unexpected response format: {0}")]
    InvalidResponse(String),
}

#[derive(Clone)]
pub struct TranslatorClient {
    http: Client,
}

impl Default for TranslatorClient {
    fn default() -> Self {
        Self::new()
    }
}

impl TranslatorClient {
    pub fn new() -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(60))
            .user_agent(concat!("Langhian/", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or_else(|_| Client::new());
        Self { http }
    }

    pub async fn translate(
        &self,
        profile: &ModelProfile,
        api_key: Option<&str>,
        req: &TranslateRequest,
    ) -> Result<TranslateResponse, ProviderError> {
        if matches!(req.input, TranslateInput::ImagePng { .. }) && !profile.supports_vision {
            return Err(ProviderError::VisionUnsupported {
                model_name: profile.name.clone(),
            });
        }

        if profile.requires_api_key && api_key.is_none_or(|k| k.trim().is_empty()) {
            return Err(ProviderError::MissingApiKey {
                model_name: profile.name.clone(),
            });
        }

        match profile.provider {
            ProviderKind::GoogleTranslateFree => {
                google_free::translate(&self.http, profile, req).await
            }
            ProviderKind::OpenAiCompatible => {
                openai_compat::translate(&self.http, profile, api_key, req).await
            }
            ProviderKind::Anthropic => {
                anthropic::translate(&self.http, profile, api_key, req).await
            }
            ProviderKind::Gemini => gemini::translate(&self.http, profile, api_key, req).await,
        }
    }

    pub async fn probe_model(
        &self,
        profile: &ModelProfile,
        api_key: Option<&str>,
    ) -> Result<ModelCapabilityProbe, ProviderError> {
        match profile.provider {
            ProviderKind::GoogleTranslateFree => {
                let test_req = TranslateRequest {
                    input: TranslateInput::Text("Hello".to_string()),
                    source_lang: "en".to_string(),
                    target_lang: "es".to_string(),
                    prompt: PromptPreset::defaults().into_iter().next().unwrap(),
                };
                google_free::translate(&self.http, profile, &test_req).await?;
                Ok(ModelCapabilityProbe {
                    reachable: true,
                    detected_vision: Some(false),
                    detail: "Google Translate Free endpoint reachable (text-only)".to_string(),
                })
            }
            ProviderKind::OpenAiCompatible => {
                openai_compat::probe_capabilities(&self.http, profile, api_key).await
            }
            ProviderKind::Anthropic => {
                if api_key.is_none_or(|k| k.trim().is_empty()) {
                    return Err(ProviderError::MissingApiKey {
                        model_name: profile.name.clone(),
                    });
                }
                let vision = infer_vision_capability(ProviderKind::Anthropic, &profile.model_id);
                anthropic::ping(&self.http, profile, api_key).await?;
                Ok(ModelCapabilityProbe {
                    reachable: true,
                    detected_vision: Some(vision),
                    detail: format!("Anthropic connected ({})", profile.model_id),
                })
            }
            ProviderKind::Gemini => {
                if api_key.is_none_or(|k| k.trim().is_empty()) {
                    return Err(ProviderError::MissingApiKey {
                        model_name: profile.name.clone(),
                    });
                }
                let vision = infer_vision_capability(ProviderKind::Gemini, &profile.model_id);
                gemini::ping(&self.http, profile, api_key).await?;
                Ok(ModelCapabilityProbe {
                    reachable: true,
                    detected_vision: Some(vision),
                    detail: format!("Gemini connected ({})", profile.model_id),
                })
            }
        }
    }

    pub async fn fetch_available_models(
        &self,
        profile: &ModelProfile,
        api_key: Option<&str>,
    ) -> Result<Vec<String>, ProviderError> {
        match profile.provider {
            ProviderKind::GoogleTranslateFree => Ok(vec!["gtx-free".to_string()]),
            ProviderKind::OpenAiCompatible => {
                openai_compat::fetch_models(&self.http, profile, api_key).await
            }
            ProviderKind::Anthropic => Ok(vec![
                "claude-sonnet-4-20250514".to_string(),
                "claude-3-7-sonnet-latest".to_string(),
                "claude-3-5-haiku-latest".to_string(),
            ]),
            ProviderKind::Gemini => gemini::fetch_models(&self.http, profile, api_key).await,
        }
    }
}

pub fn build_text_prompts(req: &TranslateRequest) -> (String, String) {
    let source_name = language_display_name(&req.source_lang);
    let target_name = language_display_name(&req.target_lang);
    let system = req
        .prompt
        .system_prompt
        .replace("{source_lang}", source_name)
        .replace("{target_lang}", target_name);
    let text = match &req.input {
        TranslateInput::Text(t) => t.as_str(),
        TranslateInput::ImagePng { .. } => "",
    };
    let user = req
        .prompt
        .render_user_prompt(text, source_name, target_name);
    (system, user)
}

pub fn build_vision_prompts(req: &TranslateRequest) -> (String, String) {
    let source_name = language_display_name(&req.source_lang);
    let target_name = language_display_name(&req.target_lang);
    let system = req
        .prompt
        .system_prompt
        .replace("{source_lang}", source_name)
        .replace("{target_lang}", target_name);
    let mut user = req.prompt.render_vision_prompt(target_name);
    if let TranslateInput::ImagePng {
        context_hint: Some(hint),
        ..
    } = &req.input
        && !hint.trim().is_empty()
    {
        user.push_str("\n\nAdditional context: ");
        user.push_str(hint.trim());
    }
    (system, user)
}

pub fn infer_vision_capability(provider: ProviderKind, model_id: &str) -> bool {
    match provider {
        ProviderKind::GoogleTranslateFree => false,
        ProviderKind::Anthropic => {
            let m = model_id.to_ascii_lowercase();
            m.contains("claude-3")
                || m.contains("claude-4")
                || m.contains("sonnet")
                || m.contains("opus")
                || m.contains("haiku")
        }
        ProviderKind::Gemini => {
            let m = model_id.to_ascii_lowercase();
            m.contains("gemini")
        }
        ProviderKind::OpenAiCompatible => {
            let m = model_id.to_ascii_lowercase();
            let vision_markers = [
                "gpt-4o",
                "gpt-4.1",
                "gpt-4.5",
                "gpt-5",
                "gpt-4-vision",
                "gpt-4-turbo",
                "o1",
                "o3",
                "o4",
                "claude-3",
                "claude-4",
                "gemini",
                "llava",
                "bakllava",
                "moondream",
                "qwen2-vl",
                "qwen2.5-vl",
                "qwen2.5vl",
                "qwen3-vl",
                "qwen-vl",
                "pixtral",
                "llama-3.2-11b-vision",
                "llama-3.2-90b-vision",
                "llama-4",
                "gemma-3",
                "gemma3",
                "minicpm-v",
                "internvl",
                "vision",
                "-vl",
            ];
            vision_markers.iter().any(|marker| m.contains(marker))
        }
    }
}

pub fn looks_like_vision_rejection(status: u16, body: &str) -> bool {
    if status != 400 && status != 422 {
        return false;
    }
    let lower = body.to_ascii_lowercase();
    lower.contains("image")
        && (lower.contains("not support")
            || lower.contains("unsupported")
            || lower.contains("invalid content")
            || lower.contains("multimodal")
            || lower.contains("vision"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn google_free_blocks_image_input_before_network() {
        let client = TranslatorClient::new();
        let profile = ModelProfile::google_translate_free();
        let req = TranslateRequest {
            input: TranslateInput::ImagePng {
                bytes: vec![1, 2, 3],
                context_hint: None,
            },
            source_lang: "auto".to_string(),
            target_lang: "fa".to_string(),
            prompt: PromptPreset::defaults().into_iter().next().unwrap(),
        };

        let res = client.translate(&profile, None, &req).await;
        assert!(matches!(res, Err(ProviderError::VisionUnsupported { .. })));
    }

    #[tokio::test]
    async fn text_only_openai_profile_blocks_image_input() {
        let client = TranslatorClient::new();
        let profile = ModelProfile::preset_ollama(); // supports_vision = false
        let req = TranslateRequest {
            input: TranslateInput::ImagePng {
                bytes: vec![1, 2, 3],
                context_hint: None,
            },
            source_lang: "auto".to_string(),
            target_lang: "en".to_string(),
            prompt: PromptPreset::defaults().into_iter().next().unwrap(),
        };

        let res = client.translate(&profile, None, &req).await;
        assert!(matches!(res, Err(ProviderError::VisionUnsupported { .. })));
    }

    #[test]
    fn infers_vision_models_accurately() {
        assert!(!infer_vision_capability(
            ProviderKind::GoogleTranslateFree,
            "gtx-free"
        ));
        assert!(infer_vision_capability(
            ProviderKind::OpenAiCompatible,
            "openai/gpt-4o-mini"
        ));
        assert!(infer_vision_capability(
            ProviderKind::OpenAiCompatible,
            "qwen2.5vl:7b"
        ));
        assert!(!infer_vision_capability(
            ProviderKind::OpenAiCompatible,
            "llama3.1:8b"
        ));
        assert!(infer_vision_capability(
            ProviderKind::Anthropic,
            "claude-sonnet-4-20250514"
        ));
        assert!(infer_vision_capability(
            ProviderKind::Gemini,
            "gemini-2.5-flash"
        ));
    }
}
