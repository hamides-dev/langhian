use super::{
    ProviderError, TranslateInput, TranslateRequest, TranslateResponse, build_text_prompts,
    build_vision_prompts, looks_like_vision_rejection,
};
use crate::models::{ModelCatalogEntry, ModelProfile};
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
            let supports_generate = m
                .get("supportedGenerationMethods")
                .and_then(|v| v.as_array())
                .is_none_or(|methods| {
                    methods
                        .iter()
                        .any(|s| s.as_str() == Some("generateContent"))
                });
            if !supports_generate {
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
                supports_vision: Some(super::infer_vision_capability(
                    crate::models::ProviderKind::Gemini,
                    id,
                )),
                context_length,
            });
        }
    }
    Ok(entries)
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
    if m.contains("2.5-pro") {
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
