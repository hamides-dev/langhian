pub mod history;
pub mod hotkey_logic;
pub mod i18n;
pub mod lang_detect;
pub mod markdown;
pub mod models;
pub mod providers;
pub mod rtl;
pub mod storage;
pub mod updater;
pub mod vocab;

pub use history::{HistoryEntry, TranslationMode};
pub use i18n::{LocaleMeta, Translator};
pub use lang_detect::{
    BidirectionalResolution, LanguageDetection, detect_dominant_language,
    resolve_bidirectional_direction,
};
pub use markdown::{
    InlineSpan, MarkdownBlock, contains_markdown_structure, html_fragment_to_markdown,
    parse_inline_spans, parse_markdown_blocks,
};
pub use models::{
    AppConfig, CapabilitySource, LanguageOption, ModelCatalogEntry, ModelProfile, PromptPreset,
    ProviderKind, ShortcutCombo, ShortcutConfig, UiThemeMode, language_display_name,
    supported_languages,
};
pub use providers::{
    AudioFormat, ModelCapabilityProbe, ProviderError, SpeechAudio, TranslateInput,
    TranslateRequest, TranslateResponse, TranslatorClient, VoiceOption, VoiceTranslationResponse,
    curated_voices_for_profile, infer_audio_capability, infer_vision_capability,
    is_audio_chat_model, is_dedicated_tts_model, markdown_to_speech_text, pcm16_le_to_wav,
    resolve_voice_for_profile,
};
pub use rtl::{TextDirection, contains_rtl_script, detect_direction, is_rtl_lang};
pub use storage::{AppStorage, SecretStore};
pub use updater::{ReleaseAsset, ReleaseManifest, UpdateCheckResult, UpdaterClient};
pub use vocab::{TextSegment, VocabEntry, VocabMatcher, find_vocab_match_index};
