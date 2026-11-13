use super::{
    ProviderError, TranslateInput, TranslateRequest, TranslateResponse, build_text_prompts,
    build_vision_prompts, looks_like_vision_rejection,
};
use crate::models::{ModelCatalogEntry, ModelProfile};
use base64::Engine;
use reqwest::Client;
use serde_json::{Value, json};

fn messages_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with("/messages") {
        trimmed.to_string()
    } else if trimmed.ends_with("/v1") {
        format!("{trimmed}/messages")
    } else {
        format!("{trimmed}/v1/messages")
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

    let (system_prompt, content) = match &req.input {
        TranslateInput::Text(_) => {
            let (sys, usr) = build_text_prompts(req);
            (sys, json!([{ "type": "text", "text": usr }]))
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
                    {
                        "type": "image",
                        "source": {
                            "type": "base64",
                            "media_type": "image/png",
                            "data": encoded
                        }
                    },
                    { "type": "text", "text": usr }
                ]),
            )
        }
    };

    let body = json!({
        "model": profile.model_id,
        "max_tokens": profile.max_tokens.unwrap_or(2048),
        "temperature": profile.temperature,
        "system": system_prompt,
        "messages": [
            { "role": "user", "content": content }
        ]
    });

    let mut builder = http
        .post(messages_url(&profile.base_url))
        .header("x-api-key", key)
        .header("anthropic-version", "2023-06-01");

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
    let text = json
        .get("content")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string()
        })
        .unwrap_or_default();

    if text.is_empty() {
        return Err(ProviderError::InvalidResponse(
            "Missing content text in Anthropic response".to_string(),
        ));
    }

    Ok(TranslateResponse {
        translated_text: text,
        detected_source_lang: None,
    })
}

fn list_models_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with("/models") {
        trimmed.to_string()
    } else if trimmed.ends_with("/v1") {
        format!("{trimmed}/models")
    } else {
        format!("{trimmed}/v1/models")
    }
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
        .header("x-api-key", key)
        .header("anthropic-version", "2023-06-01")
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
    if let Some(arr) = json.get("data").and_then(|v| v.as_array()) {
        for item in arr {
            let Some(id) = item.get("id").and_then(|v| v.as_str()) else {
                continue;
            };
            let display_name = item
                .get("display_name")
                .and_then(|v| v.as_str())
                .unwrap_or(id)
                .to_string();
            let (p_in, p_out) = anthropic_reference_pricing(id);
            entries.push(ModelCatalogEntry {
                id: id.to_string(),
                display_name,
                prompt_price_per_m: p_in,
                completion_price_per_m: p_out,
                is_free: false,
                is_local: false,
                supports_vision: Some(super::infer_vision_capability(
                    crate::models::ProviderKind::Anthropic,
                    id,
                )),
                context_length: Some(200_000),
            });
        }
    }
    Ok(entries)
}

fn anthropic_reference_pricing(model_id: &str) -> (Option<f64>, Option<f64>) {
    let m = model_id.to_ascii_lowercase();
    if m.contains("opus-4") || m.contains("claude-3-opus") {
        (Some(15.0), Some(75.0))
    } else if m.contains("sonnet-4")
        || m.contains("claude-3-7-sonnet")
        || m.contains("claude-3-5-sonnet")
    {
        (Some(3.0), Some(15.0))
    } else if m.contains("haiku-3-5") || m.contains("3-5-haiku") {
        (Some(0.80), Some(4.0))
    } else if m.contains("haiku-4") || m.contains("4-5-haiku") {
        (Some(1.0), Some(5.0))
    } else if m.contains("haiku") {
        (Some(0.25), Some(1.25))
    } else {
        (None, None)
    }
}

pub async fn ping(
    http: &Client,
    profile: &ModelProfile,
    api_key: Option<&str>,
) -> Result<(), ProviderError> {
    let key = api_key
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .ok_or_else(|| ProviderError::MissingApiKey {
            model_name: profile.name.clone(),
        })?;
    let body = json!({
        "model": profile.model_id,
        "max_tokens": 8,
        "messages": [{ "role": "user", "content": "Say ok" }]
    });
    let resp = http
        .post(messages_url(&profile.base_url))
        .header("x-api-key", key)
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?;

    let status = resp.status().as_u16();
    if !(200..300).contains(&status) {
        let raw = resp.text().await.unwrap_or_default();
        return Err(ProviderError::ApiError {
            status,
            message: extract_error(&raw),
        });
    }
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
