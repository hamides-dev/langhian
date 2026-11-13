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
    ModelCapabilityProbe, ProviderError, TranslateInput, TranslateRequest, TranslateResponse,
    TranslatorClient, infer_vision_capability,
};
pub use rtl::{TextDirection, contains_rtl_script, detect_direction, is_rtl_lang};
pub use storage::{AppStorage, SecretStore};
pub use updater::{ReleaseAsset, ReleaseManifest, UpdateCheckResult, UpdaterClient};
pub use vocab::{TextSegment, VocabEntry, VocabMatcher, find_vocab_match_index};
