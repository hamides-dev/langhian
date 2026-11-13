use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TranslationMode {
    Modal,
    InPlace,
    Screenshot,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistoryEntry {
    pub id: String,
    pub mode: TranslationMode,
    pub source_text: String,
    pub translated_text: String,
    pub model_name: String,
    pub prompt_name: String,
    pub target_lang: String,
    pub created_at: DateTime<Utc>,
}

impl HistoryEntry {
    pub fn new(
        mode: TranslationMode,
        source_text: impl Into<String>,
        translated_text: impl Into<String>,
        model_name: impl Into<String>,
        prompt_name: impl Into<String>,
        target_lang: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            mode,
            source_text: source_text.into(),
            translated_text: translated_text.into(),
            model_name: model_name.into(),
            prompt_name: prompt_name.into(),
            target_lang: target_lang.into(),
            created_at: Utc::now(),
        }
    }
}

pub const MAX_HISTORY_ITEMS: usize = 300;

pub fn push_bounded_history(history: &mut Vec<HistoryEntry>, entry: HistoryEntry) {
    history.insert(0, entry);
    if history.len() > MAX_HISTORY_ITEMS {
        history.truncate(MAX_HISTORY_ITEMS);
    }
}
