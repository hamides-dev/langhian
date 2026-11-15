use super::{
    AudioFormat, ProviderError, SpeechAudio, TranslateInput, TranslateRequest, TranslateResponse,
    build_text_prompts, build_vision_prompts, looks_like_vision_rejection, pcm16_le_to_wav,
};
use crate::models::{ModelCatalogEntry, ModelProfile, ProviderKind};
use base64::Engine;
use reqwest::Client;
use serde_json::{Value, json};

fn generate_url(base_url: &str, model_id: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    let model = model_id.trim().trim_start_matches("models/");
    if trimmed.contains(":generateContent") {
        trimmed.to_string()
    } else if trimmed.ends_with("/v1beta") || trimmed.ends_with("/v1") {
        format!("{trimmed}/models/{model}:generateContent")
    } else {
        format!("{trimmed}/v1beta/models/{model}:generateContent")
    }
}

fn interactions_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with("/interactions") {
        trimmed.to_string()
    } else if trimmed.ends_with("/v1beta") || trimmed.ends_with("/v1") {
        format!("{trimmed}/interactions")
    } else {
        format!("{trimmed}/v1beta/interactions")
    }
}

fn list_models_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with("/v1beta") || trimmed.ends_with("/v1") {
        format!("{trimmed}/models")
    } else {
        format!("{trimmed}/v1beta/models")
    }
}

pub async fn translate(
    http: &Client,
    profile: &ModelProfile,
    api_key: Option<&str>,
    req: &TranslateRequest,
) -> Result<TranslateResponse, ProviderError> {
    let key = api_key
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .ok_or_else(|| ProviderError::MissingApiKey {
            model_name: profile.name.clone(),
        })?;

    let (system_prompt, parts) = match &req.input {
        TranslateInput::Text(_) => {
            let (sys, usr) = build_text_prompts(req);
            (sys, json!([{ "text": usr }]))
        }
        TranslateInput::ImagePng { bytes, .. } => {
            if !profile.supports_vision {
                return Err(ProviderError::VisionUnsupported {
                    model_name: profile.name.clone(),
                });
            }
            let (sys, usr) = build_vision_prompts(req);
            let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
            (
                sys,
                json!([
                    { "text": usr },
                    { "inlineData": { "mimeType": "image/png", "data": encoded } }
                ]),
            )
        }
    };

    let mut generation_config = json!({
        "temperature": profile.temperature
    });
    if let Some(max_tokens) = profile.max_tokens {
        generation_config["maxOutputTokens"] = json!(max_tokens);
    }

    let body = json!({
        "systemInstruction": { "parts": [{ "text": system_prompt }] },
        "contents": [{ "role": "user", "parts": parts }],
        "generationConfig": generation_config
    });

    let mut builder = http
        .post(generate_url(&profile.base_url, &profile.model_id))
        .header("x-goog-api-key", key);
    for (k, v) in &profile.extra_headers {
        if !k.trim().is_empty() {
            builder = builder.header(k.trim(), v.trim());
        }
    }

    let resp = builder
        .json(&body)
        .send()
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?;

    let status = resp.status().as_u16();
    let raw = resp.text().await.unwrap_or_default();

    if status == 429 {
        return Err(ProviderError::RateLimited);
    }
    if !(200..300).contains(&status) {
        if matches!(req.input, TranslateInput::ImagePng { .. })
            && looks_like_vision_rejection(status, &raw)
        {
            return Err(ProviderError::VisionUnsupported {
                model_name: profile.name.clone(),
            });
        }
        return Err(ProviderError::ApiError {
            status,
            message: extract_error(&raw),
        });
    }

    let json: Value =
        serde_json::from_str(&raw).map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;
    let text = json["candidates"][0]["content"]["parts"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string()
        })
        .unwrap_or_default();

    if text.is_empty() {
        return Err(ProviderError::InvalidResponse(
            "Missing candidate text in Gemini response".to_string(),
        ));
    }

    Ok(TranslateResponse {
        translated_text: text,
        detected_source_lang: None,
    })
}

pub async fn fetch_models_detailed(
    http: &Client,
    profile: &ModelProfile,
    api_key: Option<&str>,
) -> Result<Vec<ModelCatalogEntry>, ProviderError> {
    let key = api_key
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .ok_or_else(|| ProviderError::MissingApiKey {
            model_name: profile.name.clone(),
        })?;
    let resp = http
        .get(list_models_url(&profile.base_url))
        .header("x-goog-api-key", key)
        .send()
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?;

    let status = resp.status().as_u16();
    let raw = resp.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(ProviderError::ApiError {
            status,
            message: extract_error(&raw),
        });
    }

    let json: Value =
        serde_json::from_str(&raw).map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;
    let mut entries = Vec::new();
    if let Some(arr) = json.get("models").and_then(|v| v.as_array()) {
        for m in arr {
            let Some(name) = m.get("name").and_then(|v| v.as_str()) else {
                continue;
            };
            let id = name.trim_start_matches("models/");
            if !id.starts_with("gemini-") {
                continue;
            }
            let is_audio = super::infer_audio_capability(ProviderKind::Gemini, id);
            let supports_generate = m
                .get("supportedGenerationMethods")
                .and_then(|v| v.as_array())
                .is_none_or(|methods| {
                    methods.iter().any(|s| {
                        matches!(
                            s.as_str(),
                            Some("generateContent") | Some("createInteraction")
                        )
                    })
                });
            if !supports_generate && !is_audio {
                continue;
            }
            let display_name = m
                .get("displayName")
                .and_then(|v| v.as_str())
                .unwrap_or(id)
                .to_string();
            let context_length = m
                .get("inputTokenLimit")
                .and_then(|v| v.as_u64())
                .and_then(|n| u32::try_from(n).ok());
            let (p_in, p_out) = gemini_reference_pricing(id);
            entries.push(ModelCatalogEntry {
                id: id.to_string(),
                display_name,
                prompt_price_per_m: p_in,
                completion_price_per_m: p_out,
                is_free: false,
                is_local: false,
                supports_vision: Some(super::infer_vision_capability(ProviderKind::Gemini, id)),
                supports_audio: Some(is_audio),
                context_length,
            });
        }
    }

    // Ensure current official Gemini TTS models are discoverable even if `/v1beta/models`
    // omits Interactions-API-only endpoints.
    for (tts_id, tts_name) in [
        ("gemini-3.8-flash-tts", "Gemini 3.8 Flash TTS"),
        ("gemini-3.8-flash-lite-tts", "Gemini 3.8 Flash Lite TTS"),
        (
            "gemini-3.1-flash-tts-preview",
            "Gemini 3.1 Flash TTS Preview",
        ),
        (
            "gemini-2.5-flash-preview-tts",
            "Gemini 2.5 Flash Preview TTS",
        ),
    ] {
        if !entries.iter().any(|e| e.id == tts_id) {
            let (p_in, p_out) = gemini_reference_pricing(tts_id);
            entries.push(ModelCatalogEntry {
                id: tts_id.to_string(),
                display_name: tts_name.to_string(),
                prompt_price_per_m: p_in,
                completion_price_per_m: p_out,
                is_free: false,
                is_local: false,
                supports_vision: Some(false),
                supports_audio: Some(true),
                context_length: Some(32_000),
            });
        }
    }

    Ok(entries)
}

pub async fn synthesize_speech(
    http: &Client,
    profile: &ModelProfile,
    api_key: Option<&str>,
    text: &str,
    voice: &str,
) -> Result<SpeechAudio, ProviderError> {
    let key = api_key
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .ok_or_else(|| ProviderError::MissingApiKey {
            model_name: profile.name.clone(),
        })?;

    let model = profile.model_id.trim().trim_start_matches("models/");
    let lower = model.to_ascii_lowercase();
    let prefer_generate_content = lower.contains("2.5") || lower.contains("2.0");

    if prefer_generate_content {
        match synthesize_via_generate_content(http, profile, key, model, text, voice).await {
            Ok(audio) => Ok(audio),
            Err(ProviderError::ApiError {
                status: 400 | 404, ..
            }) => synthesize_via_interactions(http, profile, key, model, text, voice).await,
            Err(e) => Err(e),
        }
    } else {
        match synthesize_via_interactions(http, profile, key, model, text, voice).await {
            Ok(audio) => Ok(audio),
            Err(ProviderError::ApiError { status: 404, .. }) => {
                synthesize_via_generate_content(http, profile, key, model, text, voice).await
            }
            Err(e) => Err(e),
        }
    }
}

async fn synthesize_via_interactions(
    http: &Client,
    profile: &ModelProfile,
    key: &str,
    model: &str,
    text: &str,
    voice: &str,
) -> Result<SpeechAudio, ProviderError> {
    let url = interactions_url(&profile.base_url);
    let body = json!({
        "model": model,
        "input": [{
            "type": "user_input",
            "content": [{ "type": "text", "text": text }]
        }],
        "response_format": {
            "type": "audio",
            "mime_type": "audio/wav"
        },
        "generation_config": {
            "speech_config": [{
                "voice": voice
            }]
        }
    });

    let resp = http
        .post(&url)
        .header("x-goog-api-key", key)
        .json(&body)
        .send()
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?;

    let status = resp.status().as_u16();
    let raw = resp.text().await.unwrap_or_default();
    if status == 429 {
        return Err(ProviderError::RateLimited);
    }
    if !(200..300).contains(&status) {
        return Err(ProviderError::ApiError {
            status,
            message: extract_error(&raw),
        });
    }

    let json: Value =
        serde_json::from_str(&raw).map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;
    parse_interactions_audio(&json)
}

async fn synthesize_via_generate_content(
    http: &Client,
    profile: &ModelProfile,
    key: &str,
    model: &str,
    text: &str,
    voice: &str,
) -> Result<SpeechAudio, ProviderError> {
    let url = generate_url(&profile.base_url, model);
    let body = json!({
        "contents": [{
            "role": "user",
            "parts": [{ "text": text }]
        }],
        "generationConfig": {
            "responseModalities": ["AUDIO"],
            "speechConfig": {
                "voiceConfig": {
                    "prebuiltVoiceConfig": {
                        "voiceName": voice
                    }
                }
            }
        }
    });

    let resp = http
        .post(&url)
        .header("x-goog-api-key", key)
        .json(&body)
        .send()
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?;

    let status = resp.status().as_u16();
    let raw = resp.text().await.unwrap_or_default();
    if status == 429 {
        return Err(ProviderError::RateLimited);
    }
    if !(200..300).contains(&status) {
        return Err(ProviderError::ApiError {
            status,
            message: extract_error(&raw),
        });
    }

    let json: Value =
        serde_json::from_str(&raw).map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;
    parse_generate_content_audio(&json)
}

fn parse_interactions_audio(json: &Value) -> Result<SpeechAudio, ProviderError> {
    if let Some(steps) = json.get("steps").and_then(|v| v.as_array()) {
        for step in steps {
            if let Some(content) = step.get("content").and_then(|v| v.as_array()) {
                for item in content {
                    if item.get("type").and_then(|v| v.as_str()) == Some("audio")
                        && let Some(b64) = item.get("data").and_then(|v| v.as_str())
                    {
                        let mime = item.get("mime_type").and_then(|v| v.as_str()).unwrap_or("");
                        return decode_gemini_audio(b64, mime);
                    }
                }
            }
        }
    }
    if let Some(outputs) = json.get("outputs").and_then(|v| v.as_array()) {
        for item in outputs {
            if item.get("type").and_then(|v| v.as_str()) == Some("audio")
                && let Some(b64) = item.get("data").and_then(|v| v.as_str())
            {
                let mime = item.get("mime_type").and_then(|v| v.as_str()).unwrap_or("");
                return decode_gemini_audio(b64, mime);
            }
        }
    }
    Err(ProviderError::AudioError(
        "Missing audio payload in Gemini Interactions response".to_string(),
    ))
}

fn parse_generate_content_audio(json: &Value) -> Result<SpeechAudio, ProviderError> {
    if let Some(parts) = json
        .pointer("/candidates/0/content/parts")
        .and_then(|v| v.as_array())
    {
        for part in parts {
            let inline = part.get("inlineData").or_else(|| part.get("inline_data"));
            if let Some(inline) = inline
                && let Some(b64) = inline.get("data").and_then(|v| v.as_str())
            {
                let mime = inline
                    .get("mimeType")
                    .or_else(|| inline.get("mime_type"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                return decode_gemini_audio(b64, mime);
            }
        }
    }
    Err(ProviderError::AudioError(
        "Missing inline audio data in Gemini generateContent response".to_string(),
    ))
}

fn decode_gemini_audio(b64: &str, mime_type: &str) -> Result<SpeechAudio, ProviderError> {
    let raw = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .map_err(|e| ProviderError::AudioError(format!("Base64 audio decode failed: {e}")))?;
    if raw.is_empty() {
        return Err(ProviderError::AudioError(
            "Gemini returned empty audio payload".to_string(),
        ));
    }

    let lower = mime_type.to_ascii_lowercase();
    if raw.starts_with(b"RIFF") {
        return Ok(SpeechAudio {
            bytes: raw,
            format: AudioFormat::Wav,
        });
    }
    if raw.starts_with(b"ID3") || lower.contains("mpeg") || lower.contains("mp3") {
        return Ok(SpeechAudio {
            bytes: raw,
            format: AudioFormat::Mp3,
        });
    }
    if lower.contains("l16") || lower.contains("pcm") || !lower.contains("wav") {
        let rate = parse_pcm_rate(mime_type).unwrap_or(24_000);
        return Ok(SpeechAudio {
            bytes: pcm16_le_to_wav(&raw, rate, 1),
            format: AudioFormat::Wav,
        });
    }
    Ok(SpeechAudio {
        bytes: raw,
        format: AudioFormat::Wav,
    })
}

fn parse_pcm_rate(mime_type: &str) -> Option<u32> {
    for part in mime_type.split(';') {
        let trimmed = part.trim();
        if let Some(val) = trimmed.strip_prefix("rate=")
            && let Ok(hz) = val.trim().parse::<u32>()
            && (8_000..=192_000).contains(&hz)
        {
            return Some(hz);
        }
    }
    None
}

pub async fn fetch_models(
    http: &Client,
    profile: &ModelProfile,
    api_key: Option<&str>,
) -> Result<Vec<String>, ProviderError> {
    let entries = fetch_models_detailed(http, profile, api_key).await?;
    Ok(entries.into_iter().map(|e| e.id).collect())
}

fn gemini_reference_pricing(model_id: &str) -> (Option<f64>, Option<f64>) {
    let m = model_id.to_ascii_lowercase();
    if m.contains("3.8-flash-lite-tts") {
        (Some(0.50), Some(6.0))
    } else if m.contains("3.8-flash-tts") {
        (Some(0.50), Some(9.0))
    } else if m.contains("2.5-pro") {
        (Some(1.25), Some(10.0))
    } else if m.contains("2.5-flash-lite") || m.contains("2.0-flash-lite") {
        (Some(0.075), Some(0.30))
    } else if m.contains("2.5-flash") {
        (Some(0.15), Some(0.60))
    } else if m.contains("2.0-flash") {
        (Some(0.10), Some(0.40))
    } else if m.contains("1.5-pro") {
        (Some(1.25), Some(5.0))
    } else if m.contains("1.5-flash") {
        (Some(0.075), Some(0.30))
    } else {
        (None, None)
    }
}

pub async fn ping(
    http: &Client,
    profile: &ModelProfile,
    api_key: Option<&str>,
) -> Result<(), ProviderError> {
    let _ = fetch_models(http, profile, api_key).await?;
    Ok(())
}

fn extract_error(raw: &str) -> String {
    serde_json::from_str::<Value>(raw)
        .ok()
        .and_then(|v| {
            v.pointer("/error/message")
                .and_then(|m| m.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| raw.chars().take(280).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gemini_interactions_wav_payload() {
        let fake_wav = b"RIFF\x24\x00\x00\x00WAVEfmt ";
        let b64 = base64::engine::general_purpose::STANDARD.encode(fake_wav);
        let payload = json!({
            "steps": [{
                "content": [
                    { "type": "audio", "data": b64, "mime_type": "audio/wav" }
                ]
            }]
        });
        let audio = parse_interactions_audio(&payload).expect("valid interactions audio");
        assert_eq!(audio.format, AudioFormat::Wav);
        assert_eq!(audio.bytes, fake_wav);
    }

    #[test]
    fn wraps_gemini_pcm_l16_as_wav() {
        let pcm = vec![0u8, 0, 10, 0, 20, 0];
        let b64 = base64::engine::general_purpose::STANDARD.encode(&pcm);
        let payload = json!({
            "candidates": [{
                "content": {
                    "parts": [{
                        "inlineData": {
                            "mimeType": "audio/L16;codec=pcm;rate=24000",
                            "data": b64
                        }
                    }]
                }
            }]
        });
        let audio = parse_generate_content_audio(&payload).expect("valid generateContent audio");
        assert_eq!(audio.format, AudioFormat::Wav);
        assert!(audio.bytes.starts_with(b"RIFF"));
        assert_eq!(&audio.bytes[44..], pcm.as_slice());
    }
}
