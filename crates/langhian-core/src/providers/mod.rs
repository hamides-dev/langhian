pub mod anthropic;
pub mod gemini;
pub mod google_free;
pub mod openai_compat;

use crate::models::{
    ModelCatalogEntry, ModelProfile, PromptPreset, ProviderKind, language_display_name,
};
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
    pub preserve_markdown: bool,
}

#[derive(Debug, Clone)]
pub struct TranslateResponse {
    pub translated_text: String,
    pub detected_source_lang: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioFormat {
    Wav,
    Mp3,
}

#[derive(Debug, Clone)]
pub struct SpeechAudio {
    pub bytes: Vec<u8>,
    pub format: AudioFormat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceOption {
    pub id: &'static str,
    pub label: &'static str,
}

#[derive(Debug, Clone)]
pub struct VoiceTranslationResponse {
    pub translated_text: String,
    pub detected_source_lang: Option<String>,
    pub audio: SpeechAudio,
    pub voice_used: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCapabilityProbe {
    pub reachable: bool,
    pub detected_vision: Option<bool>,
    pub detected_audio: Option<bool>,
    pub detail: String,
}

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("Active model '{model_name}' does not support image/screenshot input")]
    VisionUnsupported { model_name: String },
    #[error("Model '{model_name}' does not support voice/audio output")]
    AudioUnsupported { model_name: String },
    #[error("Voice synthesis failed: {0}")]
    AudioError(String),
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
                    preserve_markdown: false,
                };
                google_free::translate(&self.http, profile, &test_req).await?;
                Ok(ModelCapabilityProbe {
                    reachable: true,
                    detected_vision: Some(false),
                    detected_audio: Some(false),
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
                    detected_audio: Some(false),
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
                let audio = infer_audio_capability(ProviderKind::Gemini, &profile.model_id);
                gemini::ping(&self.http, profile, api_key).await?;
                Ok(ModelCapabilityProbe {
                    reachable: true,
                    detected_vision: Some(vision),
                    detected_audio: Some(audio),
                    detail: format!("Gemini connected ({})", profile.model_id),
                })
            }
        }
    }

    pub async fn synthesize_speech(
        &self,
        profile: &ModelProfile,
        api_key: Option<&str>,
        text: &str,
        voice: &str,
    ) -> Result<(SpeechAudio, String), ProviderError> {
        if !profile.supports_audio_output() {
            return Err(ProviderError::AudioUnsupported {
                model_name: profile.name.clone(),
            });
        }
        if profile.requires_api_key && api_key.is_none_or(|k| k.trim().is_empty()) {
            return Err(ProviderError::MissingApiKey {
                model_name: profile.name.clone(),
            });
        }
        let clean_text = text.trim();
        if clean_text.is_empty() {
            return Err(ProviderError::AudioError(
                "Cannot synthesize empty text".to_string(),
            ));
        }
        let resolved_voice = resolve_voice_for_profile(profile, voice);
        let audio = match profile.provider {
            ProviderKind::Gemini => {
                gemini::synthesize_speech(&self.http, profile, api_key, clean_text, &resolved_voice)
                    .await?
            }
            ProviderKind::OpenAiCompatible => {
                openai_compat::synthesize_speech(
                    &self.http,
                    profile,
                    api_key,
                    clean_text,
                    &resolved_voice,
                )
                .await?
            }
            ProviderKind::GoogleTranslateFree | ProviderKind::Anthropic => {
                return Err(ProviderError::AudioUnsupported {
                    model_name: profile.name.clone(),
                });
            }
        };
        Ok((audio, resolved_voice))
    }

    pub async fn translate_with_voice(
        &self,
        text_profile: &ModelProfile,
        text_api_key: Option<&str>,
        voice_profile: &ModelProfile,
        voice_api_key: Option<&str>,
        req: &TranslateRequest,
        voice: &str,
    ) -> Result<VoiceTranslationResponse, ProviderError> {
        if !voice_profile.supports_audio_output() {
            return Err(ProviderError::AudioUnsupported {
                model_name: voice_profile.name.clone(),
            });
        }

        // Direct single-call audio+text translation when active/voice model is an audio-chat LLM
        // receiving plain text input.
        if matches!(req.input, TranslateInput::Text(_))
            && is_audio_chat_model(voice_profile.provider, &voice_profile.model_id)
            && (text_profile.id == voice_profile.id
                || is_dedicated_tts_model(text_profile.provider, &text_profile.model_id))
        {
            if voice_profile.requires_api_key && voice_api_key.is_none_or(|k| k.trim().is_empty()) {
                return Err(ProviderError::MissingApiKey {
                    model_name: voice_profile.name.clone(),
                });
            }
            let resolved_voice = resolve_voice_for_profile(voice_profile, voice);
            return openai_compat::translate_with_audio_chat(
                &self.http,
                voice_profile,
                voice_api_key,
                req,
                &resolved_voice,
            )
            .await;
        }

        let text_res = self.translate(text_profile, text_api_key, req).await?;
        let spoken = markdown_to_speech_text(&text_res.translated_text);
        let to_speak = if spoken.is_empty() {
            text_res.translated_text.as_str()
        } else {
            spoken.as_str()
        };
        let (audio, voice_used) = self
            .synthesize_speech(voice_profile, voice_api_key, to_speak, voice)
            .await?;
        Ok(VoiceTranslationResponse {
            translated_text: text_res.translated_text,
            detected_source_lang: text_res.detected_source_lang,
            audio,
            voice_used,
        })
    }

    pub async fn fetch_available_models_detailed(
        &self,
        profile: &ModelProfile,
        api_key: Option<&str>,
    ) -> Result<Vec<ModelCatalogEntry>, ProviderError> {
        match profile.provider {
            ProviderKind::GoogleTranslateFree => Ok(vec![ModelCatalogEntry {
                id: "gtx-free".to_string(),
                display_name: "Google Translate (Free)".to_string(),
                prompt_price_per_m: Some(0.0),
                completion_price_per_m: Some(0.0),
                is_free: true,
                is_local: false,
                supports_vision: Some(false),
                supports_audio: Some(false),
                context_length: None,
            }]),
            ProviderKind::OpenAiCompatible => {
                openai_compat::fetch_models_detailed(&self.http, profile, api_key).await
            }
            ProviderKind::Anthropic => {
                match anthropic::fetch_models_detailed(&self.http, profile, api_key).await {
                    Ok(list) if !list.is_empty() => Ok(list),
                    Ok(_) | Err(ProviderError::ApiError { status: 404, .. }) => Ok(vec![
                        ModelCatalogEntry {
                            id: "claude-sonnet-4-20250514".to_string(),
                            display_name: "Claude Sonnet 4".to_string(),
                            prompt_price_per_m: Some(3.0),
                            completion_price_per_m: Some(15.0),
                            is_free: false,
                            is_local: false,
                            supports_vision: Some(true),
                            supports_audio: Some(false),
                            context_length: Some(200_000),
                        },
                        ModelCatalogEntry {
                            id: "claude-3-7-sonnet-latest".to_string(),
                            display_name: "Claude 3.7 Sonnet".to_string(),
                            prompt_price_per_m: Some(3.0),
                            completion_price_per_m: Some(15.0),
                            is_free: false,
                            is_local: false,
                            supports_vision: Some(true),
                            supports_audio: Some(false),
                            context_length: Some(200_000),
                        },
                        ModelCatalogEntry {
                            id: "claude-3-5-haiku-latest".to_string(),
                            display_name: "Claude 3.5 Haiku".to_string(),
                            prompt_price_per_m: Some(0.80),
                            completion_price_per_m: Some(4.0),
                            is_free: false,
                            is_local: false,
                            supports_vision: Some(true),
                            supports_audio: Some(false),
                            context_length: Some(200_000),
                        },
                    ]),
                    Err(e) => Err(e),
                }
            }
            ProviderKind::Gemini => {
                gemini::fetch_models_detailed(&self.http, profile, api_key).await
            }
        }
    }

    pub async fn fetch_available_models(
        &self,
        profile: &ModelProfile,
        api_key: Option<&str>,
    ) -> Result<Vec<String>, ProviderError> {
        let detailed = self
            .fetch_available_models_detailed(profile, api_key)
            .await?;
        Ok(detailed.into_iter().map(|m| m.id).collect())
    }
}

const MARKDOWN_SYSTEM_INSTRUCTION: &str = "\n\nWhen input contains Markdown formatting (headings such as #, ##, ###, lists, blockquotes, **bold**, *italic*, or `inline code`), preserve all Markdown markers and structural hierarchy cleanly in the translation without flattening headings into plain prose.";

pub fn build_text_prompts(req: &TranslateRequest) -> (String, String) {
    let source_name = language_display_name(&req.source_lang);
    let target_name = language_display_name(&req.target_lang);
    let mut system = req
        .prompt
        .system_prompt
        .replace("{source_lang}", source_name)
        .replace("{target_lang}", target_name);
    if req.preserve_markdown {
        system.push_str(MARKDOWN_SYSTEM_INSTRUCTION);
    }
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
    let mut system = req
        .prompt
        .system_prompt
        .replace("{source_lang}", source_name)
        .replace("{target_lang}", target_name);
    if req.preserve_markdown {
        system.push_str(MARKDOWN_SYSTEM_INSTRUCTION);
    }
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

pub fn infer_audio_capability(provider: ProviderKind, model_id: &str) -> bool {
    let m = model_id.to_ascii_lowercase();
    match provider {
        ProviderKind::GoogleTranslateFree | ProviderKind::Anthropic => false,
        ProviderKind::Gemini => m.contains("-tts") || m.contains("tts-"),
        ProviderKind::OpenAiCompatible => {
            m.contains("tts")
                || m.contains("-audio")
                || m.contains("/audio")
                || m.contains("gpt-audio")
                || m.contains("audio-preview")
                || m.contains("kokoro")
                || m.contains("orpheus")
                || m.ends_with("audio")
                || m.ends_with("-speech")
        }
    }
}

pub fn is_dedicated_tts_model(provider: ProviderKind, model_id: &str) -> bool {
    let m = model_id.to_ascii_lowercase();
    match provider {
        ProviderKind::GoogleTranslateFree | ProviderKind::Anthropic => false,
        ProviderKind::Gemini => m.contains("-tts") || m.contains("tts-"),
        ProviderKind::OpenAiCompatible => {
            m.contains("tts")
                || m.contains("kokoro")
                || m.contains("orpheus")
                || m.ends_with("-speech")
        }
    }
}

pub fn is_audio_chat_model(provider: ProviderKind, model_id: &str) -> bool {
    provider == ProviderKind::OpenAiCompatible
        && infer_audio_capability(provider, model_id)
        && !is_dedicated_tts_model(provider, model_id)
}

pub const GEMINI_CURATED_VOICES: &[VoiceOption] = &[
    VoiceOption {
        id: "Kore",
        label: "Kore — Firm, Natural",
    },
    VoiceOption {
        id: "Zephyr",
        label: "Zephyr — Bright, Warm",
    },
    VoiceOption {
        id: "Puck",
        label: "Puck — Upbeat, Lively",
    },
    VoiceOption {
        id: "Charon",
        label: "Charon — Clear, Informative",
    },
    VoiceOption {
        id: "Fenrir",
        label: "Fenrir — Expressive",
    },
    VoiceOption {
        id: "Aoede",
        label: "Aoede — Smooth, Breezy",
    },
    VoiceOption {
        id: "Sulafat",
        label: "Sulafat — Warm, Engaging",
    },
    VoiceOption {
        id: "Achird",
        label: "Achird — Friendly",
    },
    VoiceOption {
        id: "Leda",
        label: "Leda — Crisp, Youthful",
    },
    VoiceOption {
        id: "Gacrux",
        label: "Gacrux — Calm, Mature",
    },
];

pub const OPENAI_CURATED_VOICES: &[VoiceOption] = &[
    VoiceOption {
        id: "nova",
        label: "Nova — Warm, Expressive",
    },
    VoiceOption {
        id: "alloy",
        label: "Alloy — Neutral, Balanced",
    },
    VoiceOption {
        id: "ash",
        label: "Ash — Clear, Crisp",
    },
    VoiceOption {
        id: "ballad",
        label: "Ballad — Smooth, Storyteller",
    },
    VoiceOption {
        id: "coral",
        label: "Coral — Friendly, Bright",
    },
    VoiceOption {
        id: "echo",
        label: "Echo — Resonant",
    },
    VoiceOption {
        id: "fable",
        label: "Fable — Narrative",
    },
    VoiceOption {
        id: "onyx",
        label: "Onyx — Deep, Calm",
    },
    VoiceOption {
        id: "sage",
        label: "Sage — Steady, Thoughtful",
    },
    VoiceOption {
        id: "shimmer",
        label: "Shimmer — Light, Clear",
    },
    VoiceOption {
        id: "verse",
        label: "Verse — Versatile",
    },
    VoiceOption {
        id: "marin",
        label: "Marin — Natural",
    },
    VoiceOption {
        id: "cedar",
        label: "Cedar — Natural Warm",
    },
];

pub fn curated_voices_for_profile(profile: &ModelProfile) -> Vec<VoiceOption> {
    match profile.provider {
        ProviderKind::Gemini => GEMINI_CURATED_VOICES.to_vec(),
        ProviderKind::OpenAiCompatible => OPENAI_CURATED_VOICES.to_vec(),
        ProviderKind::GoogleTranslateFree | ProviderKind::Anthropic => Vec::new(),
    }
}

pub fn resolve_voice_for_profile(profile: &ModelProfile, configured: &str) -> String {
    let trimmed = configured.trim();
    let options = curated_voices_for_profile(profile);
    if let Some(found) = options.iter().find(|v| v.id.eq_ignore_ascii_case(trimmed)) {
        return found.id.to_string();
    }
    match profile.provider {
        ProviderKind::Gemini => "Kore".to_string(),
        ProviderKind::OpenAiCompatible => {
            let lower_url = profile.base_url.to_ascii_lowercase();
            let is_local_or_custom = lower_url.contains("localhost")
                || lower_url.contains("127.0.0.1")
                || (!lower_url.contains("openai.com") && !lower_url.contains("openrouter.ai"));
            if is_local_or_custom && !trimmed.is_empty() {
                trimmed.to_string()
            } else {
                "nova".to_string()
            }
        }
        ProviderKind::GoogleTranslateFree | ProviderKind::Anthropic => "Kore".to_string(),
    }
}

/// Strip Markdown syntax into clean spoken prose for TTS synthesis while preserving paragraph pauses.
pub fn markdown_to_speech_text(input: &str) -> String {
    let mut out_lines = Vec::new();
    let mut in_fence = false;
    for raw_line in input.lines() {
        let trimmed = raw_line.trim();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            if !trimmed.is_empty() {
                out_lines.push(trimmed.to_string());
            }
            continue;
        }
        if trimmed.is_empty() {
            continue;
        }
        if trimmed
            .chars()
            .all(|c| c == '-' || c == '*' || c == '_' || c == ' ')
            && trimmed.len() >= 3
        {
            continue;
        }

        let mut line = trimmed.trim_start_matches('#').trim_start();
        while let Some(rest) = line.strip_prefix("> ") {
            line = rest.trim_start();
        }
        for prefix in ["- ", "* ", "+ "] {
            if let Some(rest) = line.strip_prefix(prefix) {
                line = rest.trim_start();
                break;
            }
        }
        if let Some((num_part, rest)) = line.split_once(". ")
            && !num_part.is_empty()
            && num_part.chars().all(|c| c.is_ascii_digit())
        {
            line = rest.trim_start();
        }

        let stripped = strip_inline_markdown(line);
        let clean = stripped.trim();
        if !clean.is_empty() {
            out_lines.push(clean.to_string());
        }
    }
    out_lines.join("\n")
}

fn strip_inline_markdown(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '['
            && let Some(close_bracket) = chars[i + 1..].iter().position(|&c| c == ']')
        {
            let b_end = i + 1 + close_bracket;
            if b_end + 1 < chars.len()
                && chars[b_end + 1] == '('
                && let Some(close_paren) = chars[b_end + 2..].iter().position(|&c| c == ')')
            {
                let label: String = chars[i + 1..b_end].iter().collect();
                out.push_str(&label);
                i = b_end + 2 + close_paren + 1;
                continue;
            }
        }
        if chars[i] == '`' || chars[i] == '*' || chars[i] == '~' {
            i += 1;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Wrap raw signed 16-bit little-endian PCM bytes in a canonical 44-byte RIFF WAVE container.
pub fn pcm16_le_to_wav(pcm_bytes: &[u8], sample_rate: u32, channels: u16) -> Vec<u8> {
    let data_len = u32::try_from(pcm_bytes.len()).unwrap_or(u32::MAX - 36);
    let byte_rate = sample_rate
        .saturating_mul(u32::from(channels))
        .saturating_mul(2);
    let block_align = channels.saturating_mul(2);
    let mut wav = Vec::with_capacity(44 + pcm_bytes.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36u32.saturating_add(data_len)).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    wav.extend_from_slice(pcm_bytes);
    wav
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
            preserve_markdown: false,
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
            preserve_markdown: false,
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

    #[test]
    fn infers_audio_and_tts_models_accurately() {
        assert!(!infer_audio_capability(
            ProviderKind::GoogleTranslateFree,
            "gtx-free"
        ));
        assert!(!infer_audio_capability(
            ProviderKind::Anthropic,
            "claude-sonnet-4-20250514"
        ));
        assert!(!infer_audio_capability(
            ProviderKind::Gemini,
            "gemini-2.5-flash"
        ));
        assert!(infer_audio_capability(
            ProviderKind::Gemini,
            "gemini-3.8-flash-tts"
        ));
        assert!(is_dedicated_tts_model(
            ProviderKind::Gemini,
            "gemini-3.8-flash-tts"
        ));
        assert!(infer_audio_capability(
            ProviderKind::OpenAiCompatible,
            "gpt-4o-mini-tts"
        ));
        assert!(is_dedicated_tts_model(
            ProviderKind::OpenAiCompatible,
            "gpt-4o-mini-tts"
        ));
        assert!(infer_audio_capability(
            ProviderKind::OpenAiCompatible,
            "openai/gpt-audio-mini"
        ));
        assert!(is_audio_chat_model(
            ProviderKind::OpenAiCompatible,
            "openai/gpt-audio-mini"
        ));
        assert!(!is_dedicated_tts_model(
            ProviderKind::OpenAiCompatible,
            "openai/gpt-audio-mini"
        ));
    }

    #[test]
    fn resolves_voices_across_providers() {
        let gem = ModelProfile::preset_gemini();
        assert_eq!(resolve_voice_for_profile(&gem, "kore"), "Kore");
        assert_eq!(resolve_voice_for_profile(&gem, "nova"), "Kore");

        let oa = ModelProfile::preset_openai();
        assert_eq!(resolve_voice_for_profile(&oa, "Kore"), "nova");
        assert_eq!(resolve_voice_for_profile(&oa, "ALLOY"), "alloy");
    }

    #[test]
    fn strips_markdown_for_natural_speech() {
        let md = "## Heading One\n- First **bold** item with [docs](https://example.com)\n2. Second `code` item\n";
        let speech = markdown_to_speech_text(md);
        assert_eq!(
            speech,
            "Heading One\nFirst bold item with docs\nSecond code item"
        );
    }

    #[test]
    fn builds_valid_wav_header_around_pcm16() {
        let pcm = vec![0u8, 1, 2, 3, 4, 5];
        let wav = pcm16_le_to_wav(&pcm, 24_000, 1);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 6);
        assert_eq!(&wav[44..], pcm.as_slice());
    }
}
