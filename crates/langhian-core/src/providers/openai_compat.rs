use super::{
    ModelCapabilityProbe, ProviderError, TranslateInput, TranslateRequest, TranslateResponse,
    build_text_prompts, build_vision_prompts, infer_vision_capability, looks_like_vision_rejection,
};
use crate::models::{ModelCatalogEntry, ModelProfile, ProviderKind};
use base64::Engine;
use reqwest::{Client, RequestBuilder};
use serde_json::{Value, json};

fn chat_completions_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/chat/completions")
    }
}

fn models_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    let root = trimmed.strip_suffix("/chat/completions").unwrap_or(trimmed);
    format!("{root}/models")
}

fn apply_auth_and_headers(
    mut builder: RequestBuilder,
    profile: &ModelProfile,
    api_key: Option<&str>,
) -> RequestBuilder {
    if let Some(key) = api_key {
        let k = key.trim();
        if !k.is_empty() {
            builder = builder.bearer_auth(k);
        }
    }
    for (k, v) in &profile.extra_headers {
        if !k.trim().is_empty() {
            builder = builder.header(k.trim(), v.trim());
        }
    }
    builder
}

pub async fn translate(
    http: &Client,
    profile: &ModelProfile,
    api_key: Option<&str>,
    req: &TranslateRequest,
) -> Result<TranslateResponse, ProviderError> {
    let url = chat_completions_url(&profile.base_url);

    let (system_prompt, user_content) = match &req.input {
        TranslateInput::Text(_) => {
            let (sys, usr) = build_text_prompts(req);
            (sys, Value::String(usr))
        }
        TranslateInput::ImagePng { bytes, .. } => {
            if !profile.supports_vision {
                return Err(ProviderError::VisionUnsupported {
                    model_name: profile.name.clone(),
                });
            }
            let (sys, usr) = build_vision_prompts(req);
            let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
            let data_url = format!("data:image/png;base64,{encoded}");
            (
                sys,
                json!([
                    { "type": "text", "text": usr },
                    { "type": "image_url", "image_url": { "url": data_url } }
                ]),
            )
        }
    };

    let mut body = json!({
        "model": profile.model_id,
        "temperature": profile.temperature,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": user_content }
        ]
    });

    if let Some(max_tokens) = profile.max_tokens {
        body["max_tokens"] = json!(max_tokens);
    }

    let builder = apply_auth_and_headers(http.post(&url), profile, api_key);
    let resp = builder
        .json(&body)
        .send()
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?;

    let status = resp.status().as_u16();
    let raw_text = resp.text().await.unwrap_or_default();

    if status == 429 {
        return Err(ProviderError::RateLimited);
    }
    if !(200..300).contains(&status) {
        if matches!(req.input, TranslateInput::ImagePng { .. })
            && looks_like_vision_rejection(status, &raw_text)
        {
            return Err(ProviderError::VisionUnsupported {
                model_name: profile.name.clone(),
            });
        }
        return Err(ProviderError::ApiError {
            status,
            message: extract_error_message(&raw_text),
        });
    }

    let json: Value = serde_json::from_str(&raw_text)
        .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;
    parse_openai_response(&json)
}

pub fn parse_openai_response(json: &Value) -> Result<TranslateResponse, ProviderError> {
    let content = &json["choices"][0]["message"]["content"];
    let text = if let Some(s) = content.as_str() {
        s.trim().to_string()
    } else if let Some(arr) = content.as_array() {
        arr.iter()
            .filter_map(|part| part.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string()
    } else {
        String::new()
    };

    if text.is_empty() {
        return Err(ProviderError::InvalidResponse(
            "Missing choices[0].message.content".to_string(),
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
    let url = models_url(&profile.base_url);
    let builder = apply_auth_and_headers(http.get(&url), profile, api_key);
    let resp = builder
        .send()
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?;

    let status = resp.status().as_u16();
    let raw = resp.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(ProviderError::ApiError {
            status,
            message: extract_error_message(&raw),
        });
    }

    let json: Value =
        serde_json::from_str(&raw).map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;
    Ok(parse_models_catalog(&profile.base_url, &json))
}

pub fn parse_models_catalog(base_url: &str, json: &Value) -> Vec<ModelCatalogEntry> {
    let base_lower = base_url.to_ascii_lowercase();
    let is_local = base_lower.contains("localhost") || base_lower.contains("127.0.0.1");
    let is_openai_official = base_lower.contains("api.openai.com");
    let mut entries = Vec::new();

    if let Some(arr) = json.get("data").and_then(|v| v.as_array()) {
        for item in arr {
            let Some(id) = item.get("id").and_then(|v| v.as_str()) else {
                continue;
            };
            if is_openai_official && !is_openai_chat_model(id) {
                continue;
            }

            let display_name = item
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(id)
                .to_string();

            let context_length = item
                .get("context_length")
                .and_then(|v| v.as_u64())
                .and_then(|n| u32::try_from(n).ok());

            let vision_from_modalities = item
                .pointer("/architecture/input_modalities")
                .and_then(|v| v.as_array())
                .map(|mods| mods.iter().any(|m| m.as_str() == Some("image")));
            let supports_vision =
                Some(vision_from_modalities.unwrap_or_else(|| {
                    infer_vision_capability(ProviderKind::OpenAiCompatible, id)
                }));

            let prompt_price_raw = parse_price_per_token(item.pointer("/pricing/prompt"));
            let completion_price_raw = parse_price_per_token(item.pointer("/pricing/completion"));

            let (prompt_price_per_m, completion_price_per_m) =
                match (prompt_price_raw, completion_price_raw) {
                    (Some(pi), Some(po)) => (Some(pi * 1_000_000.0), Some(po * 1_000_000.0)),
                    (Some(pi), None) => (Some(pi * 1_000_000.0), None),
                    _ if is_openai_official => openai_reference_pricing(id),
                    _ => (None, None),
                };

            let is_free = is_local
                || id.ends_with(":free")
                || (prompt_price_per_m == Some(0.0) && completion_price_per_m == Some(0.0));

            entries.push(ModelCatalogEntry {
                id: id.to_string(),
                display_name,
                prompt_price_per_m,
                completion_price_per_m,
                is_free,
                is_local,
                supports_vision,
                context_length,
            });
        }
    }

    entries.sort_by(|a, b| a.id.cmp(&b.id));
    entries
}

fn parse_price_per_token(val: Option<&Value>) -> Option<f64> {
    let v = val?;
    let parsed = if let Some(n) = v.as_f64() {
        Some(n)
    } else if let Some(s) = v.as_str() {
        s.trim().parse::<f64>().ok()
    } else {
        None
    }?;
    if parsed.is_finite() && parsed >= 0.0 {
        Some(parsed)
    } else {
        None
    }
}

fn is_openai_chat_model(id: &str) -> bool {
    let lower = id.to_ascii_lowercase();
    if lower.contains("embedding")
        || lower.contains("tts")
        || lower.contains("whisper")
        || lower.contains("dall-e")
        || lower.contains("moderation")
        || lower.contains("realtime")
        || lower.contains("transcribe")
        || lower.contains("audio")
        || lower.contains("image-")
        || lower.contains("sora")
    {
        return false;
    }
    lower.starts_with("gpt-")
        || lower.starts_with("o1")
        || lower.starts_with("o3")
        || lower.starts_with("o4")
        || lower.starts_with("chatgpt-")
}

fn openai_reference_pricing(id: &str) -> (Option<f64>, Option<f64>) {
    let m = id.to_ascii_lowercase();
    if m.starts_with("gpt-4o-mini") {
        (Some(0.15), Some(0.60))
    } else if m.starts_with("gpt-4o") {
        (Some(2.50), Some(10.0))
    } else if m.starts_with("gpt-4.1-nano") {
        (Some(0.10), Some(0.40))
    } else if m.starts_with("gpt-4.1-mini") {
        (Some(0.40), Some(1.60))
    } else if m.starts_with("gpt-4.1") {
        (Some(2.00), Some(8.00))
    } else if m.starts_with("o4-mini") || m.starts_with("o3-mini") {
        (Some(1.10), Some(4.40))
    } else if m.starts_with("o3") {
        (Some(2.00), Some(8.00))
    } else if m.starts_with("gpt-5-nano") {
        (Some(0.05), Some(0.40))
    } else if m.starts_with("gpt-5-mini") {
        (Some(0.25), Some(2.00))
    } else if m.starts_with("gpt-5") {
        (Some(1.25), Some(10.00))
    } else {
        (None, None)
    }
}

pub async fn fetch_models(
    http: &Client,
    profile: &ModelProfile,
    api_key: Option<&str>,
) -> Result<Vec<String>, ProviderError> {
    let entries = fetch_models_detailed(http, profile, api_key).await?;
    Ok(entries.into_iter().map(|e| e.id).collect())
}

pub async fn probe_capabilities(
    http: &Client,
    profile: &ModelProfile,
    api_key: Option<&str>,
) -> Result<ModelCapabilityProbe, ProviderError> {
    let base_lower = profile.base_url.to_ascii_lowercase();

    // 1. Check Ollama native /api/show endpoint when running against Ollama
    if base_lower.contains("11434") {
        let ollama_root = profile
            .base_url
            .trim()
            .trim_end_matches('/')
            .trim_end_matches("/v1");
        let show_url = format!("{ollama_root}/api/show");
        if let Ok(resp) = http
            .post(&show_url)
            .json(&json!({ "name": profile.model_id }))
            .send()
            .await
            && resp.status().is_success()
            && let Ok(val) = resp.json::<Value>().await
            && let Some(caps) = val.get("capabilities").and_then(|v| v.as_array())
        {
            let has_vision = caps
                .iter()
                .any(|c| c.as_str() == Some("vision") || c.as_str() == Some("image"));
            return Ok(ModelCapabilityProbe {
                reachable: true,
                detected_vision: Some(has_vision),
                detail: format!(
                    "Ollama model '{}' capabilities: {}",
                    profile.model_id,
                    if has_vision {
                        "vision + text"
                    } else {
                        "text only"
                    }
                ),
            });
        }
    }

    // 2. Check /models listing (OpenRouter includes input_modalities!)
    let url = models_url(&profile.base_url);
    let builder = apply_auth_and_headers(http.get(&url), profile, api_key);
    let resp = builder
        .send()
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?;

    let status = resp.status().as_u16();
    let raw = resp.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(ProviderError::ApiError {
            status,
            message: extract_error_message(&raw),
        });
    }

    if let Ok(json) = serde_json::from_str::<Value>(&raw)
        && let Some(arr) = json.get("data").and_then(|v| v.as_array())
        && let Some(model_obj) = arr.iter().find(|m| {
            m.get("id")
                .and_then(|v| v.as_str())
                .is_some_and(|id| id == profile.model_id)
        })
        && let Some(modalities) = model_obj
            .pointer("/architecture/input_modalities")
            .and_then(|v| v.as_array())
    {
        let has_image = modalities.iter().any(|m| m.as_str() == Some("image"));
        return Ok(ModelCapabilityProbe {
            reachable: true,
            detected_vision: Some(has_image),
            detail: format!(
                "Endpoint verified; '{}' input modalities: {}",
                profile.model_id,
                if has_image {
                    "image + text"
                } else {
                    "text only"
                }
            ),
        });
    }

    let inferred = infer_vision_capability(ProviderKind::OpenAiCompatible, &profile.model_id);
    Ok(ModelCapabilityProbe {
        reachable: true,
        detected_vision: Some(inferred),
        detail: format!(
            "Endpoint reachable; vision inferred as {} from model ID",
            inferred
        ),
    })
}

fn extract_error_message(raw: &str) -> String {
    if let Ok(v) = serde_json::from_str::<Value>(raw) {
        if let Some(msg) = v.pointer("/error/message").and_then(|m| m.as_str()) {
            return msg.to_string();
        }
        if let Some(msg) = v.get("message").and_then(|m| m.as_str()) {
            return msg.to_string();
        }
    }
    raw.chars().take(280).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_openai_urls() {
        assert_eq!(
            chat_completions_url("https://openrouter.ai/api/v1/"),
            "https://openrouter.ai/api/v1/chat/completions"
        );
        assert_eq!(
            chat_completions_url("http://localhost:11434/v1/chat/completions"),
            "http://localhost:11434/v1/chat/completions"
        );
        assert_eq!(
            models_url("http://localhost:1234/v1/chat/completions"),
            "http://localhost:1234/v1/models"
        );
    }

    #[test]
    fn parses_openai_chat_completion() {
        let payload = json!({
            "choices": [{
                "message": { "content": "  ترجمه تست  " }
            }]
        });
        let res = parse_openai_response(&payload).expect("valid response");
        assert_eq!(res.translated_text, "ترجمه تست");
    }

    #[test]
    fn parses_openrouter_models_with_pricing_and_modalities() {
        let payload = json!({
            "data": [
                {
                    "id": "google/gemini-2.5-flash",
                    "name": "Gemini 2.5 Flash",
                    "context_length": 1000000,
                    "architecture": { "input_modalities": ["text", "image"] },
                    "pricing": { "prompt": "0.00000015", "completion": "0.00000060" }
                },
                {
                    "id": "meta-llama/llama-3.3-70b:free",
                    "name": "Llama 3.3 70B Free",
                    "context_length": 131072,
                    "architecture": { "input_modalities": ["text"] },
                    "pricing": { "prompt": "0", "completion": "0" }
                }
            ]
        });
        let list = parse_models_catalog("https://openrouter.ai/api/v1", &payload);
        assert_eq!(list.len(), 2);
        let flash = &list[0];
        assert_eq!(flash.id, "google/gemini-2.5-flash");
        assert_eq!(flash.supports_vision, Some(true));
        assert_eq!(flash.format_price_badge(), "$0.15 in · $0.60 out / 1M");
        assert_eq!(list[1].format_price_badge(), "Free");
    }
}
