use iced::widget::image::{Handle as ImageHandle, Image};
use iced::widget::svg::{self, Handle, Svg};
use iced::{Color, ContentFit, Length};
use langhian_core::ProviderKind;
use std::sync::OnceLock;

pub const APP_LOGO_PNG: &[u8] = include_bytes!("../../../../assets/icons/langhian.png");

static APP_LOGO_HANDLE: OnceLock<ImageHandle> = OnceLock::new();

fn app_logo_handle() -> ImageHandle {
    APP_LOGO_HANDLE
        .get_or_init(|| match image::load_from_memory(APP_LOGO_PNG) {
            Ok(img) => {
                let rgba = img.to_rgba8();
                let (width, height) = rgba.dimensions();
                ImageHandle::from_rgba(width, height, rgba.into_raw())
            }
            Err(_) => ImageHandle::from_bytes(APP_LOGO_PNG),
        })
        .clone()
}

/// Renders the transparent full-color Langhian application icon using a cached handle
/// so texture allocations remain stable across hover/focus repaints.
pub fn app_logo(size: f32) -> Image<ImageHandle> {
    Image::new(app_logo_handle())
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .content_fit(ContentFit::Contain)
}

// Embedded Material Symbols Rounded UI SVGs (400 weight, 24px grid)
const ICON_ADD: &[u8] = include_bytes!("../../../../assets/icons/ui/add.svg");
const ICON_BOLT: &[u8] = include_bytes!("../../../../assets/icons/ui/bolt.svg");
const ICON_CHECK: &[u8] = include_bytes!("../../../../assets/icons/ui/check.svg");
const ICON_CHECK_CIRCLE: &[u8] = include_bytes!("../../../../assets/icons/ui/check_circle.svg");
const ICON_CHEVRON_LEFT: &[u8] = include_bytes!("../../../../assets/icons/ui/chevron_left.svg");
const ICON_CHEVRON_RIGHT: &[u8] = include_bytes!("../../../../assets/icons/ui/chevron_right.svg");
const ICON_CLOSE: &[u8] = include_bytes!("../../../../assets/icons/ui/close.svg");
const ICON_CONTENT_COPY: &[u8] = include_bytes!("../../../../assets/icons/ui/content_copy.svg");
const ICON_CROP_FREE: &[u8] = include_bytes!("../../../../assets/icons/ui/crop_free.svg");
const ICON_CROP_SQUARE: &[u8] = include_bytes!("../../../../assets/icons/ui/crop_square.svg");
const ICON_DARK_MODE: &[u8] = include_bytes!("../../../../assets/icons/ui/dark_mode.svg");
const ICON_DELETE: &[u8] = include_bytes!("../../../../assets/icons/ui/delete.svg");
const ICON_DOWNLOAD: &[u8] = include_bytes!("../../../../assets/icons/ui/download.svg");
const ICON_EDIT: &[u8] = include_bytes!("../../../../assets/icons/ui/edit.svg");
const ICON_ERROR: &[u8] = include_bytes!("../../../../assets/icons/ui/error.svg");
const ICON_FILE_COPY: &[u8] = include_bytes!("../../../../assets/icons/ui/file_copy.svg");
const ICON_FILTER_NONE: &[u8] = include_bytes!("../../../../assets/icons/ui/filter_none.svg");
const ICON_HISTORY: &[u8] = include_bytes!("../../../../assets/icons/ui/history.svg");
const ICON_INFO: &[u8] = include_bytes!("../../../../assets/icons/ui/info.svg");
const ICON_KEEP: &[u8] = include_bytes!("../../../../assets/icons/ui/keep.svg");
const ICON_KEEP_FILL: &[u8] = include_bytes!("../../../../assets/icons/ui/keep-fill.svg");
const ICON_KEYBOARD: &[u8] = include_bytes!("../../../../assets/icons/ui/keyboard.svg");
const ICON_LANGUAGE: &[u8] = include_bytes!("../../../../assets/icons/ui/language.svg");
const ICON_LIGHT_MODE: &[u8] = include_bytes!("../../../../assets/icons/ui/light_mode.svg");
const ICON_MENU_BOOK: &[u8] = include_bytes!("../../../../assets/icons/ui/menu_book.svg");
const ICON_PAYMENTS: &[u8] = include_bytes!("../../../../assets/icons/ui/payments.svg");
const ICON_PSYCHOLOGY: &[u8] = include_bytes!("../../../../assets/icons/ui/psychology.svg");
const ICON_REFRESH: &[u8] = include_bytes!("../../../../assets/icons/ui/refresh.svg");
const ICON_REMOVE: &[u8] = include_bytes!("../../../../assets/icons/ui/remove.svg");
const ICON_RESTART_ALT: &[u8] = include_bytes!("../../../../assets/icons/ui/restart_alt.svg");
const ICON_ROCKET_LAUNCH: &[u8] = include_bytes!("../../../../assets/icons/ui/rocket_launch.svg");
const ICON_SEARCH: &[u8] = include_bytes!("../../../../assets/icons/ui/search.svg");
const ICON_SETTINGS: &[u8] = include_bytes!("../../../../assets/icons/ui/settings.svg");
const ICON_SMART_TOY: &[u8] = include_bytes!("../../../../assets/icons/ui/smart_toy.svg");
const ICON_STAR: &[u8] = include_bytes!("../../../../assets/icons/ui/star.svg");
const ICON_STAR_FILL: &[u8] = include_bytes!("../../../../assets/icons/ui/star-fill.svg");
const ICON_STAR_SHINE: &[u8] = include_bytes!("../../../../assets/icons/ui/star_shine.svg");
const ICON_SWAP_HORIZ: &[u8] = include_bytes!("../../../../assets/icons/ui/swap_horiz.svg");
const ICON_TRANSLATE: &[u8] = include_bytes!("../../../../assets/icons/ui/translate.svg");
const ICON_TUNE: &[u8] = include_bytes!("../../../../assets/icons/ui/tune.svg");
const ICON_VISIBILITY: &[u8] = include_bytes!("../../../../assets/icons/ui/visibility.svg");
const ICON_WAND_STARS: &[u8] = include_bytes!("../../../../assets/icons/ui/wand_stars.svg");

// Embedded Provider Brand SVGs
const PROV_ANTHROPIC: &[u8] = include_bytes!("../../../../assets/icons/providers/anthropic.svg");
const PROV_CUSTOM: &[u8] = include_bytes!("../../../../assets/icons/providers/custom.svg");
const PROV_GEMINI: &[u8] = include_bytes!("../../../../assets/icons/providers/gemini.svg");
const PROV_GOOGLE_TRANSLATE: &[u8] =
    include_bytes!("../../../../assets/icons/providers/google_translate.svg");
const PROV_LMSTUDIO: &[u8] = include_bytes!("../../../../assets/icons/providers/lmstudio.svg");
const PROV_OLLAMA: &[u8] = include_bytes!("../../../../assets/icons/providers/ollama.svg");
const PROV_OPENAI: &[u8] = include_bytes!("../../../../assets/icons/providers/openai.svg");
const PROV_OPENROUTER: &[u8] = include_bytes!("../../../../assets/icons/providers/openrouter.svg");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiIcon {
    Add,
    Bolt,
    Check,
    CheckCircle,
    ChevronLeft,
    ChevronRight,
    Close,
    Copy,
    CropFree,
    CropSquare,
    DarkMode,
    Delete,
    Download,
    Edit,
    Error,
    FileCopy,
    FilterNone,
    History,
    Info,
    Keep,
    KeepFill,
    Keyboard,
    Language,
    LightMode,
    MenuBook,
    Payments,
    Psychology,
    Refresh,
    Remove,
    RestartAlt,
    RocketLaunch,
    Search,
    Settings,
    SmartToy,
    Star,
    StarFill,
    StarShine,
    SwapHoriz,
    Translate,
    Tune,
    Visibility,
    WandStars,
}

impl UiIcon {
    pub fn bytes(self) -> &'static [u8] {
        match self {
            Self::Add => ICON_ADD,
            Self::Bolt => ICON_BOLT,
            Self::Check => ICON_CHECK,
            Self::CheckCircle => ICON_CHECK_CIRCLE,
            Self::ChevronLeft => ICON_CHEVRON_LEFT,
            Self::ChevronRight => ICON_CHEVRON_RIGHT,
            Self::Close => ICON_CLOSE,
            Self::Copy => ICON_CONTENT_COPY,
            Self::CropFree => ICON_CROP_FREE,
            Self::CropSquare => ICON_CROP_SQUARE,
            Self::DarkMode => ICON_DARK_MODE,
            Self::Delete => ICON_DELETE,
            Self::Download => ICON_DOWNLOAD,
            Self::Edit => ICON_EDIT,
            Self::Error => ICON_ERROR,
            Self::FileCopy => ICON_FILE_COPY,
            Self::FilterNone => ICON_FILTER_NONE,
            Self::History => ICON_HISTORY,
            Self::Info => ICON_INFO,
            Self::Keep => ICON_KEEP,
            Self::KeepFill => ICON_KEEP_FILL,
            Self::Keyboard => ICON_KEYBOARD,
            Self::Language => ICON_LANGUAGE,
            Self::LightMode => ICON_LIGHT_MODE,
            Self::MenuBook => ICON_MENU_BOOK,
            Self::Payments => ICON_PAYMENTS,
            Self::Psychology => ICON_PSYCHOLOGY,
            Self::Refresh => ICON_REFRESH,
            Self::Remove => ICON_REMOVE,
            Self::RestartAlt => ICON_RESTART_ALT,
            Self::RocketLaunch => ICON_ROCKET_LAUNCH,
            Self::Search => ICON_SEARCH,
            Self::Settings => ICON_SETTINGS,
            Self::SmartToy => ICON_SMART_TOY,
            Self::Star => ICON_STAR,
            Self::StarFill => ICON_STAR_FILL,
            Self::StarShine => ICON_STAR_SHINE,
            Self::SwapHoriz => ICON_SWAP_HORIZ,
            Self::Translate => ICON_TRANSLATE,
            Self::Tune => ICON_TUNE,
            Self::Visibility => ICON_VISIBILITY,
            Self::WandStars => ICON_WAND_STARS,
        }
    }
}

/// Renders a crisp Material Symbols Rounded SVG icon tinted to the requested color.
pub fn ui_icon<'a>(icon: UiIcon, size: f32, color: Color) -> Svg<'a> {
    Svg::new(Handle::from_memory(icon.bytes()))
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .style(move |_, _| svg::Style { color: Some(color) })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderLogoKind {
    GoogleTranslate,
    OpenRouter,
    OpenAi,
    Anthropic,
    Gemini,
    Ollama,
    LmStudio,
    Custom,
}

impl ProviderLogoKind {
    pub fn from_profile(provider: ProviderKind, base_url: &str, name: &str) -> Self {
        match provider {
            ProviderKind::GoogleTranslateFree => Self::GoogleTranslate,
            ProviderKind::Anthropic => Self::Anthropic,
            ProviderKind::Gemini => Self::Gemini,
            ProviderKind::OpenAiCompatible => {
                let lower = format!("{} {}", base_url, name).to_lowercase();
                if lower.contains("openrouter.ai") || lower.contains("openrouter") {
                    Self::OpenRouter
                } else if lower.contains("11434") || lower.contains("ollama") {
                    Self::Ollama
                } else if lower.contains("1234")
                    || lower.contains("lm studio")
                    || lower.contains("lmstudio")
                {
                    Self::LmStudio
                } else if lower.contains("api.openai.com") || lower.contains("openai") {
                    Self::OpenAi
                } else {
                    Self::Custom
                }
            }
        }
    }
}

/// Renders a provider brand SVG. Multi-color logos keep native gradient colors;
/// monochrome vector marks tint seamlessly to match Light & Dark themes.
pub fn provider_logo<'a>(kind: ProviderLogoKind, size: f32, mono_tint: Color) -> Svg<'a> {
    let (bytes, override_color) = match kind {
        ProviderLogoKind::Gemini => (PROV_GEMINI, None),
        ProviderLogoKind::GoogleTranslate => {
            (PROV_GOOGLE_TRANSLATE, Some(Color::from_rgb8(66, 133, 244)))
        }
        ProviderLogoKind::Anthropic => (PROV_ANTHROPIC, Some(mono_tint)),
        ProviderLogoKind::OpenAi => (PROV_OPENAI, Some(mono_tint)),
        ProviderLogoKind::OpenRouter => (PROV_OPENROUTER, Some(mono_tint)),
        ProviderLogoKind::Ollama => (PROV_OLLAMA, Some(mono_tint)),
        ProviderLogoKind::LmStudio => (PROV_LMSTUDIO, Some(mono_tint)),
        ProviderLogoKind::Custom => (PROV_CUSTOM, Some(mono_tint)),
    };

    Svg::new(Handle::from_memory(bytes))
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .style(move |_, _| svg::Style {
            color: override_color,
        })
}
