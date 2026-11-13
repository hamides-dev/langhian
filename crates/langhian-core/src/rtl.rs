#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextDirection {
    #[default]
    Ltr,
    Rtl,
}

impl TextDirection {
    pub fn is_rtl(self) -> bool {
        matches!(self, TextDirection::Rtl)
    }
}

pub fn is_rtl_lang(code: &str) -> bool {
    let lower = code.trim().to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "fa" | "per" | "pes" | "ar" | "ara" | "he" | "iw" | "ur" | "ps" | "ckb" | "ku-arab" | "yi"
    )
}

pub fn is_rtl_char(ch: char) -> bool {
    let cp = ch as u32;
    matches!(
        cp,
        0x0590..=0x05FF   // Hebrew
        | 0x0600..=0x06FF // Arabic / Persian
        | 0x0750..=0x077F // Arabic Supplement
        | 0x0780..=0x07BF // Thaana
        | 0x08A0..=0x08FF // Arabic Extended-A
        | 0xFB50..=0xFDFF // Arabic Presentation Forms-A
        | 0xFE70..=0xFEFF // Arabic Presentation Forms-B
    )
}

pub fn contains_rtl_script(text: &str) -> bool {
    text.chars().any(is_rtl_char)
}

pub fn detect_direction(text: &str, lang_hint: Option<&str>) -> TextDirection {
    for ch in text.chars() {
        if is_rtl_char(ch) {
            return TextDirection::Rtl;
        }
        if ch.is_ascii_alphabetic() {
            // Keep scanning only to check if RTL dominates if mixed; first strong char wins like UAX#9,
            // unless explicit RTL target language with predominantly RTL characters.
            let rtl_count = text.chars().filter(|&c| is_rtl_char(c)).count();
            let ltr_count = text.chars().filter(|c| c.is_ascii_alphabetic()).count();
            if rtl_count > ltr_count {
                return TextDirection::Rtl;
            }
            if rtl_count > 0 && lang_hint.is_some_and(is_rtl_lang) {
                return TextDirection::Rtl;
            }
            return TextDirection::Ltr;
        }
    }

    if lang_hint.is_some_and(is_rtl_lang) {
        TextDirection::Rtl
    } else {
        TextDirection::Ltr
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_persian_and_arabic_rtl() {
        assert_eq!(detect_direction("سلام دنیا", Some("en")), TextDirection::Rtl);
        assert_eq!(detect_direction("مرحبا بالعالم", None), TextDirection::Rtl);
        assert_eq!(detect_direction("", Some("fa")), TextDirection::Rtl);
    }

    #[test]
    fn detects_english_and_mixed_bidi() {
        assert_eq!(
            detect_direction("Hello world", Some("fa")),
            TextDirection::Ltr
        );
        assert_eq!(
            detect_direction("Hello: سلام خوبی عزیزم", Some("fa")),
            TextDirection::Rtl
        );
        assert_eq!(
            detect_direction("Using Rust for ترجمه سریع", Some("en")),
            TextDirection::Ltr
        );
    }
}
