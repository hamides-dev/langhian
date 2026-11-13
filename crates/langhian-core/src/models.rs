use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    #[default]
    GoogleTranslateFree,
    OpenAiCompatible,
    Anthropic,
    Gemini,
}

impl ProviderKind {
    pub fn all() -> &'static [ProviderKind] {
        &[
            ProviderKind::GoogleTranslateFree,
            ProviderKind::OpenAiCompatible,
            ProviderKind::Anthropic,
            ProviderKind::Gemini,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CapabilitySource {
    #[default]
    Heuristic,
    AutoDetected,
    ManualOverride,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelProfile {
    pub id: String,
    pub name: String,
    pub provider: ProviderKind,
    pub base_url: String,
    pub model_id: String,
    pub requires_api_key: bool,
    pub supports_vision: bool,
    #[serde(default)]
    pub vision_source: CapabilitySource,
    #[serde(default)]
    pub extra_headers: BTreeMap<String, String>,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default)]
    pub max_tokens: Option<u32>,
}

fn default_temperature() -> f32 {
    0.2
}

impl ModelProfile {
    pub fn google_translate_free() -> Self {
        Self {
            id: "google-free-default".to_string(),
            name: "Google Translate (Free)".to_string(),
            provider: ProviderKind::GoogleTranslateFree,
            base_url: "https://translate.googleapis.com".to_string(),
            model_id: "gtx-free".to_string(),
            requires_api_key: false,
            supports_vision: false,
            vision_source: CapabilitySource::AutoDetected,
            extra_headers: BTreeMap::new(),
            temperature: 0.0,
            max_tokens: None,
        }
    }

    pub fn preset_openai() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: "OpenAI — GPT-4o Mini".to_string(),
            provider: ProviderKind::OpenAiCompatible,
            base_url: "https://api.openai.com/v1".to_string(),
            model_id: "gpt-4o-mini".to_string(),
            requires_api_key: true,
            supports_vision: true,
            vision_source: CapabilitySource::Heuristic,
            extra_headers: BTreeMap::new(),
            temperature: 0.2,
            max_tokens: None,
        }
    }

    pub fn preset_openrouter() -> Self {
        let mut headers = BTreeMap::new();
        headers.insert(
            "HTTP-Referer".to_string(),
            "https://github.com/hamides-dev/langhian".to_string(),
        );
        headers.insert("X-Title".to_string(), "Langhian Translation".to_string());
        Self {
            id: Uuid::new_v4().to_string(),
            name: "OpenRouter — Gemini 2.5 Flash".to_string(),
            provider: ProviderKind::OpenAiCompatible,
            base_url: "https://openrouter.ai/api/v1".to_string(),
            model_id: "google/gemini-2.5-flash".to_string(),
            requires_api_key: true,
            supports_vision: true,
            vision_source: CapabilitySource::Heuristic,
            extra_headers: headers,
            temperature: 0.2,
            max_tokens: None,
        }
    }

    pub fn preset_ollama() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: "Ollama (Local)".to_string(),
            provider: ProviderKind::OpenAiCompatible,
            base_url: "http://localhost:11434/v1".to_string(),
            model_id: "qwen2.5:7b".to_string(),
            requires_api_key: false,
            supports_vision: false,
            vision_source: CapabilitySource::Heuristic,
            extra_headers: BTreeMap::new(),
            temperature: 0.2,
            max_tokens: None,
        }
    }

    pub fn preset_lmstudio() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: "LM Studio (Local)".to_string(),
            provider: ProviderKind::OpenAiCompatible,
            base_url: "http://localhost:1234/v1".to_string(),
            model_id: "local-model".to_string(),
            requires_api_key: false,
            supports_vision: false,
            vision_source: CapabilitySource::Heuristic,
            extra_headers: BTreeMap::new(),
            temperature: 0.2,
            max_tokens: None,
        }
    }

    pub fn preset_anthropic() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: "Anthropic — Claude Sonnet".to_string(),
            provider: ProviderKind::Anthropic,
            base_url: "https://api.anthropic.com".to_string(),
            model_id: "claude-sonnet-4-20250514".to_string(),
            requires_api_key: true,
            supports_vision: true,
            vision_source: CapabilitySource::Heuristic,
            extra_headers: BTreeMap::new(),
            temperature: 0.2,
            max_tokens: Some(2048),
        }
    }

    pub fn preset_gemini() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: "Google Gemini — 2.5 Flash".to_string(),
            provider: ProviderKind::Gemini,
            base_url: "https://generativelanguage.googleapis.com".to_string(),
            model_id: "gemini-2.5-flash".to_string(),
            requires_api_key: true,
            supports_vision: true,
            vision_source: CapabilitySource::Heuristic,
            extra_headers: BTreeMap::new(),
            temperature: 0.2,
            max_tokens: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PromptPreset {
    pub id: String,
    pub name: String,
    pub shortcut_slot: Option<u8>,
    pub system_prompt: String,
    pub user_template: String,
    pub vision_instruction: String,
}

impl PromptPreset {
    pub fn defaults() -> Vec<Self> {
        vec![
            Self {
                id: "prompt-natural".to_string(),
                name: "Natural Translation".to_string(),
                shortcut_slot: Some(1),
                system_prompt: "You are a precise, natural translator. Output only the final translation without commentary, quotes, or explanations unless asked.".to_string(),
                user_template: "Translate the following text from {source_lang} to {target_lang} naturally and idiomatically:\n\n{text}".to_string(),
                vision_instruction: "Read all visible text in this screenshot and translate it cleanly into {target_lang}. Preserve layout structure where helpful and output only the translated result.".to_string(),
            },
            Self {
                id: "prompt-formal".to_string(),
                name: "Formal & Business".to_string(),
                shortcut_slot: Some(2),
                system_prompt: "You are a professional executive translator. Produce polished, formal business language suitable for emails, clients, and documentation. Output only the translation.".to_string(),
                user_template: "Translate from {source_lang} to {target_lang} using a clear, polite, formal tone:\n\n{text}".to_string(),
                vision_instruction: "Extract the text in this screenshot and translate it into formal, professional {target_lang}.".to_string(),
            },
            Self {
                id: "prompt-technical".to_string(),
                name: "Technical & Developer".to_string(),
                shortcut_slot: Some(3),
                system_prompt: "You are a software engineering translator. Keep code identifiers, CLI flags, API symbols, and markdown formatting intact while translating prose naturally. Output only the translation.".to_string(),
                user_template: "Translate this technical text from {source_lang} to {target_lang} while preserving technical identifiers:\n\n{text}".to_string(),
                vision_instruction: "Read the technical UI, error message, or code-adjacent text in this screenshot and translate it accurately into {target_lang}.".to_string(),
            },
            Self {
                id: "prompt-polish".to_string(),
                name: "Rewrite & Polish".to_string(),
                shortcut_slot: Some(4),
                system_prompt: "You are a skilled bilingual editor. Fix grammar, awkward phrasing, and spelling while translating into fluent {target_lang}. Output only the refined text.".to_string(),
                user_template: "Translate and polish the following text into fluent, concise {target_lang}:\n\n{text}".to_string(),
                vision_instruction: "Transcribe and translate the text shown in this screenshot into fluent, polished {target_lang}.".to_string(),
            },
        ]
    }

    pub fn render_user_prompt(&self, text: &str, source_lang: &str, target_lang: &str) -> String {
        self.user_template
            .replace("{text}", text)
            .replace("{source_lang}", source_lang)
            .replace("{target_lang}", target_lang)
    }

    pub fn render_vision_prompt(&self, target_lang: &str) -> String {
        self.vision_instruction
            .replace("{target_lang}", target_lang)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum UiThemeMode {
    #[default]
    Dark,
    Light,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ShortcutCombo {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
    pub key: String,
}

impl ShortcutCombo {
    pub fn new(ctrl: bool, alt: bool, shift: bool, meta: bool, key: impl Into<String>) -> Self {
        Self {
            ctrl,
            alt,
            shift,
            meta,
            key: key.into().to_uppercase(),
        }
    }

    pub fn format_display(&self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("Ctrl".to_string());
        }
        if self.alt {
            parts.push("Alt".to_string());
        }
        if self.shift {
            parts.push("Shift".to_string());
        }
        if self.meta {
            #[cfg(target_os = "macos")]
            parts.push("Cmd".to_string());
            #[cfg(not(target_os = "macos"))]
            parts.push("Super".to_string());
        }
        parts.push(self.key.clone());
        parts.join("+")
    }

    pub fn parse_display(raw: &str) -> Option<Self> {
        let tokens: Vec<String> = raw
            .split('+')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if tokens.is_empty() {
            return None;
        }
        let mut ctrl = false;
        let mut alt = false;
        let mut shift = false;
        let mut meta = false;
        let mut key: Option<String> = None;

        for (idx, token) in tokens.iter().enumerate() {
            let lower = token.to_ascii_lowercase();
            let is_last = idx + 1 == tokens.len();
            match lower.as_str() {
                "ctrl" | "control" => ctrl = true,
                "alt" | "option" => alt = true,
                "shift" => shift = true,
                "meta" | "cmd" | "super" | "win" => meta = true,
                _ if is_last => key = Some(token.to_ascii_uppercase()),
                _ => return None,
            }
        }

        key.map(|k| Self {
            ctrl,
            alt,
            shift,
            meta,
            key: k,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ShortcutConfig {
    pub enable_double_alt: bool,
    pub auto_select_all_fallback: bool,
    pub enable_double_ctrl: bool,
    pub double_tap_ms: u64,
    pub screenshot: ShortcutCombo,
    pub next_prompt: ShortcutCombo,
    pub prev_prompt: ShortcutCombo,
    pub open_dashboard: ShortcutCombo,
}

impl Default for ShortcutConfig {
    fn default() -> Self {
        Self {
            enable_double_alt: true,
            auto_select_all_fallback: true,
            enable_double_ctrl: true,
            double_tap_ms: 340,
            screenshot: ShortcutCombo::new(true, false, true, false, "X"),
            next_prompt: ShortcutCombo::new(true, true, false, false, "RIGHT"),
            prev_prompt: ShortcutCombo::new(true, true, false, false, "LEFT"),
            open_dashboard: ShortcutCombo::new(true, false, true, false, "L"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelCatalogEntry {
    pub id: String,
    pub display_name: String,
    pub prompt_price_per_m: Option<f64>,
    pub completion_price_per_m: Option<f64>,
    pub is_free: bool,
    pub is_local: bool,
    pub supports_vision: Option<bool>,
    pub context_length: Option<u32>,
}

impl ModelCatalogEntry {
    pub fn format_price_badge(&self) -> String {
        if self.is_local {
            return "Local · Free".to_string();
        }
        if self.is_free {
            return "Free".to_string();
        }
        match (self.prompt_price_per_m, self.completion_price_per_m) {
            (Some(p_in), Some(p_out)) if p_in == 0.0 && p_out == 0.0 => "Free".to_string(),
            (Some(p_in), Some(p_out)) => {
                format!("{} in · {} out / 1M", format_usd(p_in), format_usd(p_out))
            }
            (Some(p_in), None) => format!("{} in / 1M", format_usd(p_in)),
            _ => "API Rate".to_string(),
        }
    }

    pub fn format_context_badge(&self) -> Option<String> {
        self.context_length.map(|ctx| {
            if ctx >= 1_000_000 {
                format!("{}M ctx", ctx / 1_000_000)
            } else if ctx >= 1_000 {
                format!("{}K ctx", ctx / 1_000)
            } else {
                format!("{ctx} ctx")
            }
        })
    }
}

fn format_usd(val: f64) -> String {
    if val < 0.01 && val > 0.0 {
        format!("${val:.4}")
    } else if val < 0.10 {
        format!("${val:.3}")
    } else {
        format!("${val:.2}")
    }
}

fn default_native_language() -> String {
    "en".to_string()
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppConfig {
    pub ui_language: String,
    pub theme: UiThemeMode,
    #[serde(default)]
    pub onboarding_completed: bool,
    #[serde(default = "default_native_language")]
    pub native_language: String,
    pub source_language: String,
    pub target_language: String,
    #[serde(default = "default_true")]
    pub preserve_markdown: bool,
    #[serde(default)]
    pub modal_position: Option<[f32; 2]>,
    #[serde(default)]
    pub modal_size: Option<[f32; 2]>,
    #[serde(default)]
    pub modal_pinned: bool,
    pub active_model_id: String,
    pub active_prompt_id: String,
    pub models: Vec<ModelProfile>,
    pub prompts: Vec<PromptPreset>,
    pub shortcuts: ShortcutConfig,
    pub restore_clipboard: bool,
    pub launch_at_startup: bool,
    pub show_hud: bool,
    pub auto_check_updates: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        let default_model = ModelProfile::google_translate_free();
        let prompts = PromptPreset::defaults();
        let active_prompt_id = prompts[0].id.clone();

        Self {
            ui_language: "en".to_string(),
            theme: UiThemeMode::Dark,
            onboarding_completed: false,
            native_language: "en".to_string(),
            source_language: "auto".to_string(),
            target_language: "fa".to_string(),
            preserve_markdown: true,
            modal_position: None,
            modal_size: None,
            modal_pinned: false,
            active_model_id: default_model.id.clone(),
            active_prompt_id,
            models: vec![default_model],
            prompts,
            shortcuts: ShortcutConfig::default(),
            restore_clipboard: true,
            launch_at_startup: false,
            show_hud: true,
            auto_check_updates: true,
        }
    }
}

impl AppConfig {
    pub fn active_model(&self) -> &ModelProfile {
        self.models
            .iter()
            .find(|m| m.id == self.active_model_id)
            .unwrap_or(&self.models[0])
    }

    pub fn active_prompt(&self) -> &PromptPreset {
        self.prompts
            .iter()
            .find(|p| p.id == self.active_prompt_id)
            .unwrap_or(&self.prompts[0])
    }

    pub fn ensure_valid(&mut self) {
        if self.models.is_empty() {
            self.models.push(ModelProfile::google_translate_free());
        }
        if !self.models.iter().any(|m| m.id == self.active_model_id) {
            self.active_model_id = self.models[0].id.clone();
        }
        if self.prompts.is_empty() {
            self.prompts = PromptPreset::defaults();
        }
        if !self.prompts.iter().any(|p| p.id == self.active_prompt_id) {
            self.active_prompt_id = self.prompts[0].id.clone();
        }
        self.shortcuts.double_tap_ms = self.shortcuts.double_tap_ms.clamp(160, 800);
        if self.native_language.trim().is_empty() || self.native_language == "auto" {
            self.native_language = "en".to_string();
        }
        if self.target_language.trim().is_empty() || self.target_language == "auto" {
            self.target_language = if self.native_language == "en" {
                "fa".to_string()
            } else {
                "en".to_string()
            };
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageOption {
    pub code: &'static str,
    pub name: &'static str,
    pub native_name: &'static str,
    pub rtl: bool,
}

impl std::fmt::Display for LanguageOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.name == self.native_name {
            write!(f, "{} ({})", self.name, self.code)
        } else {
            write!(f, "{} — {}", self.name, self.native_name)
        }
    }
}

pub fn supported_languages() -> &'static [LanguageOption] {
    &[
        LanguageOption {
            code: "en",
            name: "English",
            native_name: "English",
            rtl: false,
        },
        LanguageOption {
            code: "fa",
            name: "Persian",
            native_name: "فارسی",
            rtl: true,
        },
        LanguageOption {
            code: "ar",
            name: "Arabic",
            native_name: "العربية",
            rtl: true,
        },
        LanguageOption {
            code: "tr",
            name: "Turkish",
            native_name: "Türkçe",
            rtl: false,
        },
        LanguageOption {
            code: "de",
            name: "German",
            native_name: "Deutsch",
            rtl: false,
        },
        LanguageOption {
            code: "fr",
            name: "French",
            native_name: "Français",
            rtl: false,
        },
        LanguageOption {
            code: "es",
            name: "Spanish",
            native_name: "Español",
            rtl: false,
        },
        LanguageOption {
            code: "it",
            name: "Italian",
            native_name: "Italiano",
            rtl: false,
        },
        LanguageOption {
            code: "pt",
            name: "Portuguese",
            native_name: "Português",
            rtl: false,
        },
        LanguageOption {
            code: "ru",
            name: "Russian",
            native_name: "Русский",
            rtl: false,
        },
        LanguageOption {
            code: "zh-CN",
            name: "Chinese (Simplified)",
            native_name: "简体中文",
            rtl: false,
        },
        LanguageOption {
            code: "ja",
            name: "Japanese",
            native_name: "日本語",
            rtl: false,
        },
        LanguageOption {
            code: "ko",
            name: "Korean",
            native_name: "한국어",
            rtl: false,
        },
        LanguageOption {
            code: "nl",
            name: "Dutch",
            native_name: "Nederlands",
            rtl: false,
        },
        LanguageOption {
            code: "ur",
            name: "Urdu",
            native_name: "اردو",
            rtl: true,
        },
        LanguageOption {
            code: "he",
            name: "Hebrew",
            native_name: "עברית",
            rtl: true,
        },
        LanguageOption {
            code: "hi",
            name: "Hindi",
            native_name: "हिन्दी",
            rtl: false,
        },
    ]
}

pub fn language_display_name(code: &str) -> &str {
    if code == "auto" {
        return "Auto-detect";
    }
    supported_languages()
        .iter()
        .find(|l| l.code.eq_ignore_ascii_case(code))
        .map(|l| l.name)
        .unwrap_or(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_config_deserializes_with_sensible_defaults() {
        let base = serde_json::to_value(AppConfig::default()).unwrap();
        let mut obj = base.as_object().unwrap().clone();
        obj.remove("onboarding_completed");
        obj.remove("native_language");
        obj.remove("preserve_markdown");
        obj.remove("modal_position");
        obj.remove("modal_pinned");
        let parsed: AppConfig =
            serde_json::from_value(serde_json::Value::Object(obj)).expect("deserializes legacy");
        assert!(!parsed.onboarding_completed);
        assert_eq!(parsed.native_language, "en");
        assert!(parsed.preserve_markdown);
        assert!(parsed.modal_position.is_none());
        assert!(!parsed.modal_pinned);
    }
}
