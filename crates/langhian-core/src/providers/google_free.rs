use super::{ProviderError, TranslateInput, TranslateRequest, TranslateResponse};
use crate::models::ModelProfile;
use reqwest::Client;
use serde_json::Value;
use url::Url;

pub async fn translate(
    http: &Client,
    profile: &ModelProfile,
    req: &TranslateRequest,
) -> Result<TranslateResponse, ProviderError> {
    let text = match &req.input {
        TranslateInput::Text(t) => t,
        TranslateInput::ImagePng { .. } => {
            return Err(ProviderError::VisionUnsupported {
                model_name: profile.name.clone(),
            });
        }
    };

    let url = Url::parse_with_params(
        "https://translate.googleapis.com/translate_a/single",
        &[
            ("client", "gtx"),
            ("sl", req.source_lang.as_str()),
            ("tl", req.target_lang.as_str()),
            ("dt", "t"),
            ("q", text.as_str()),
        ],
    )
    .map_err(|e| ProviderError::Network(e.to_string()))?;

    let resp = http
        .get(url)
        .header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
        )
        .send()
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?;

    let status = resp.status().as_u16();
    if status == 429 {
        return Err(ProviderError::RateLimited);
    }
    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(ProviderError::ApiError {
            status,
            message: body,
        });
    }

    let json: Value = resp
        .json()
        .await
        .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

    parse_google_translate_payload(&json)
}

pub fn parse_google_translate_payload(json: &Value) -> Result<TranslateResponse, ProviderError> {
    let sentences = json
        .get(0)
        .and_then(|v| v.as_array())
        .ok_or_else(|| ProviderError::InvalidResponse("Missing sentences array".to_string()))?;

    let mut translated = String::new();
    for item in sentences {
        if let Some(fragment) = item.get(0).and_then(|v| v.as_str()) {
            translated.push_str(fragment);
        }
    }

    if translated.is_empty() {
        return Err(ProviderError::InvalidResponse(
            "Empty translation payload".to_string(),
        ));
    }

    let detected_source_lang = json.get(2).and_then(|v| v.as_str()).map(|s| s.to_string());

    Ok(TranslateResponse {
        translated_text: translated,
        detected_source_lang,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_multi_segment_google_free_json() {
        let payload = json!([
            [
                ["سلام ", "Hello ", null, null, 1],
                ["دنیا!", "world!", null, null, 1]
            ],
            null,
            "en"
        ]);
        let res = parse_google_translate_payload(&payload).expect("should parse");
        assert_eq!(res.translated_text, "سلام دنیا!");
        assert_eq!(res.detected_source_lang.as_deref(), Some("en"));
    }
}
