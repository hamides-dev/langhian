use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq)]
pub struct LanguageDetection {
    pub lang_code: Option<String>,
    pub confidence: f32,
    pub is_confident: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BidirectionalResolution {
    pub effective_source: String,
    pub effective_target: String,
    pub detected_lang: Option<String>,
    pub detection_confidence: f32,
    pub is_confident: bool,
    pub was_reversed: bool,
}

pub const CONFIDENCE_THRESHOLD: f32 = 0.70;

pub fn detect_dominant_language(text: &str, preferred_langs: &[&str]) -> LanguageDetection {
    let cleaned = strip_urls_and_code_tokens(text);
    let mut arabic_persian_chars = 0usize;
    let mut persian_specific_chars = 0usize;
    let mut arabic_specific_chars = 0usize;
    let mut latin_chars = 0usize;
    let mut turkish_specific_chars = 0usize;
    let mut german_specific_chars = 0usize;
    let mut french_specific_chars = 0usize;
    let mut spanish_specific_chars = 0usize;
    let mut cyrillic_chars = 0usize;
    let mut cjk_chars = 0usize;
    let mut hangul_chars = 0usize;
    let mut kana_chars = 0usize;
    let mut devanagari_chars = 0usize;
    let mut total_letters = 0usize;

    for ch in cleaned.chars() {
        if !ch.is_alphabetic() {
            continue;
        }
        total_letters += 1;

        match ch {
            // Persian-distinctive Arabic script letters
            '\u{067E}' /* پ */
            | '\u{0686}' /* چ */
            | '\u{0698}' /* ژ */
            | '\u{06AF}' /* گ */
            | '\u{06A9}' /* ک */
            | '\u{06CC}' /* ی */ => {
                arabic_persian_chars += 1;
                persian_specific_chars += 1;
            }
            // Arabic-distinctive letters
            '\u{0629}' /* ة */
            | '\u{0649}' /* ى */
            | '\u{0623}' /* أ */
            | '\u{0625}' /* إ */
            | '\u{064A}' /* ي */
            | '\u{0643}' /* ك */ => {
                arabic_persian_chars += 1;
                arabic_specific_chars += 1;
            }
            '\u{0600}'..='\u{06FF}' | '\u{0750}'..='\u{077F}' | '\u{FB50}'..='\u{FDFF}' | '\u{FE70}'..='\u{FEFF}' => {
                arabic_persian_chars += 1;
            }
            'a'..='z' | 'A'..='Z' | '\u{00C0}'..='\u{024F}' => {
                latin_chars += 1;
                match ch {
                    'ğ' | 'Ğ' | 'ş' | 'Ş' | 'ı' | 'İ' => turkish_specific_chars += 1,
                    'ß' | 'ä' | 'Ä' => german_specific_chars += 1,
                    'œ' | 'Œ' | 'æ' | 'Æ' | 'ê' | 'è' | 'ë' | 'î' | 'ï' | 'ô' | 'û' | 'ù' => {
                        french_specific_chars += 1
                    }
                    'ñ' | 'Ñ' | 'á' | 'í' | 'ó' | 'ú' => spanish_specific_chars += 1,
                    'ç' | 'Ç' | 'ö' | 'Ö' | 'ü' | 'Ü' => {
                        // Shared across tr/de/fr; small boost handled in word analysis
                    }
                    _ => {}
                }
            }
            '\u{0400}'..='\u{04FF}' => cyrillic_chars += 1,
            '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}' => cjk_chars += 1,
            '\u{3040}'..='\u{30FF}' => kana_chars += 1,
            '\u{AC00}'..='\u{D7AF}' => hangul_chars += 1,
            '\u{0900}'..='\u{097F}' => devanagari_chars += 1,
            _ => {}
        }
    }

    if total_letters == 0 {
        return LanguageDetection {
            lang_code: None,
            confidence: 0.0,
            is_confident: false,
        };
    }

    let ar_fa_ratio = arabic_persian_chars as f32 / total_letters as f32;
    let latin_ratio = latin_chars as f32 / total_letters as f32;
    let cyrillic_ratio = cyrillic_chars as f32 / total_letters as f32;
    let cjk_ratio = (cjk_chars + kana_chars) as f32 / total_letters as f32;
    let hangul_ratio = hangul_chars as f32 / total_letters as f32;
    let hindi_ratio = devanagari_chars as f32 / total_letters as f32;

    if ar_fa_ratio >= latin_ratio
        && ar_fa_ratio >= cyrillic_ratio
        && ar_fa_ratio >= cjk_ratio
        && ar_fa_ratio >= hangul_ratio
        && ar_fa_ratio >= hindi_ratio
    {
        let words = tokenize_words(&cleaned);
        let fa_words = count_hits(
            &words,
            &[
                "و", "در", "به", "از", "که", "این", "آن", "را", "با", "برای", "است", "هست", "نیست",
                "می", "های", "یک", "شما", "من", "ما", "هم", "بود", "شد", "شود", "دارد", "کرد",
                "کنید", "سلام", "خوب", "خیلی", "چطور", "چرا", "اگر", "اما", "یا", "تا", "روی",
                "زیر", "خود",
            ],
        );
        let ar_words = count_hits(
            &words,
            &[
                "في",
                "من",
                "على",
                "إلى",
                "عن",
                "هذا",
                "هذه",
                "التي",
                "الذي",
                "أن",
                "لا",
                "ما",
                "مع",
                "كان",
                "هل",
                "كيف",
                "مرحبا",
                "اليوم",
                "أنت",
                "أنا",
                "نحن",
                "على",
                "بين",
                "كل",
                "لقد",
            ],
        );
        let ar_prefix_hits = words
            .iter()
            .filter(|w| w.starts_with("ال") && w.chars().count() >= 4)
            .count();

        let fa_score = persian_specific_chars * 3 + fa_words * 4;
        let ar_score = arabic_specific_chars * 3 + ar_words * 4 + ar_prefix_hits * 2;

        let prefers_fa = preferred_langs.contains(&"fa");
        let prefers_ar = preferred_langs.contains(&"ar");

        let code = if ar_score > fa_score + 1 || (prefers_ar && !prefers_fa && fa_score == 0) {
            "ar"
        } else if fa_score > 0 || prefers_fa || arabic_specific_chars == 0 {
            "fa"
        } else {
            "ar"
        };

        return LanguageDetection {
            lang_code: Some(code.to_string()),
            confidence: ar_fa_ratio,
            is_confident: ar_fa_ratio >= CONFIDENCE_THRESHOLD,
        };
    }

    if latin_ratio >= cyrillic_ratio
        && latin_ratio >= cjk_ratio
        && latin_ratio >= hangul_ratio
        && latin_ratio >= hindi_ratio
    {
        let lower = cleaned.to_lowercase();
        let words = tokenize_words(&lower);
        let total_words = words.len().max(1);

        let en_hits = count_hits(
            &words,
            &[
                "the", "is", "are", "was", "were", "be", "been", "to", "of", "and", "a", "an",
                "in", "on", "at", "for", "with", "that", "this", "it", "you", "we", "they", "i",
                "my", "your", "our", "from", "by", "as", "not", "but", "or", "if", "can", "will",
                "have", "has", "had", "do", "does", "please", "hello", "how", "what", "when",
                "where", "why", "who",
            ],
        );
        let tr_hits = count_hits(
            &words,
            &[
                "ve", "bir", "bu", "şu", "o", "için", "ile", "de", "da", "mi", "mı", "mu", "mü",
                "ben", "sen", "biz", "siz", "ama", "çok", "ne", "nasıl", "merhaba", "gün", "iyi",
                "olan", "olarak", "gibi", "kadar", "var", "yok",
            ],
        );
        let de_hits = count_hits(
            &words,
            &[
                "der", "die", "das", "und", "ist", "in", "zu", "den", "von", "mit", "auf", "für",
                "nicht", "ein", "eine", "ich", "sie", "wir", "es", "auch", "als", "wie", "hallo",
            ],
        );
        let fr_hits = count_hits(
            &words,
            &[
                "le", "la", "les", "et", "est", "un", "une", "de", "des", "du", "en", "dans",
                "pour", "avec", "que", "qui", "pas", "je", "nous", "vous", "bonjour", "merci",
            ],
        );
        let es_hits = count_hits(
            &words,
            &[
                "el", "la", "los", "las", "y", "es", "en", "de", "del", "un", "una", "por", "para",
                "con", "que", "no", "hola", "como", "gracias", "pero",
            ],
        );

        let tr_score = turkish_specific_chars * 4 + tr_hits * 4;
        let de_score = german_specific_chars * 4 + de_hits * 4;
        let fr_score = french_specific_chars * 3 + fr_hits * 4;
        let es_score = spanish_specific_chars * 3 + es_hits * 4;
        let en_score = en_hits * 4;

        let mut candidates = [
            ("en", en_score),
            ("tr", tr_score),
            ("de", de_score),
            ("fr", fr_score),
            ("es", es_score),
        ];
        candidates.sort_by_key(|a| std::cmp::Reverse(a.1));
        let (best_code, best_score) = candidates[0];
        let second_score = candidates[1].1;

        let resolved_code = if best_score > 0 && best_score > second_score {
            best_code
        } else {
            preferred_langs
                .iter()
                .copied()
                .find(|p| is_latin_lang(p))
                .unwrap_or("en")
        };

        // Combine script dominance and word signal gently so short mixed phrases stay honest.
        let word_margin = if best_score == 0 {
            if total_words <= 3 { 1.0 } else { 0.88 }
        } else {
            let ratio = best_score as f32 / (best_score + second_score) as f32;
            0.60 + 0.40 * ratio
        };
        let conf = (latin_ratio * word_margin).clamp(0.0, 1.0);

        return LanguageDetection {
            lang_code: Some(resolved_code.to_string()),
            confidence: conf,
            is_confident: conf >= CONFIDENCE_THRESHOLD,
        };
    }

    if cyrillic_ratio >= cjk_ratio
        && cyrillic_ratio >= hangul_ratio
        && cyrillic_ratio >= hindi_ratio
    {
        return LanguageDetection {
            lang_code: Some("ru".to_string()),
            confidence: cyrillic_ratio,
            is_confident: cyrillic_ratio >= CONFIDENCE_THRESHOLD,
        };
    }

    if kana_chars > 0 && cjk_ratio >= hangul_ratio && cjk_ratio >= hindi_ratio {
        return LanguageDetection {
            lang_code: Some("ja".to_string()),
            confidence: cjk_ratio,
            is_confident: cjk_ratio >= CONFIDENCE_THRESHOLD,
        };
    }

    if cjk_ratio >= hangul_ratio && cjk_ratio >= hindi_ratio {
        return LanguageDetection {
            lang_code: Some("zh".to_string()),
            confidence: cjk_ratio,
            is_confident: cjk_ratio >= CONFIDENCE_THRESHOLD,
        };
    }

    if hangul_ratio >= hindi_ratio {
        return LanguageDetection {
            lang_code: Some("ko".to_string()),
            confidence: hangul_ratio,
            is_confident: hangul_ratio >= CONFIDENCE_THRESHOLD,
        };
    }

    LanguageDetection {
        lang_code: Some("hi".to_string()),
        confidence: hindi_ratio,
        is_confident: hindi_ratio >= CONFIDENCE_THRESHOLD,
    }
}

/// Resolves the exact `(source_lang, target_lang)` for a translation request without mutating user settings.
///
/// - When `smart_bidirectional` is `true` (Modal & Dashboard smart-pair mode):
///   - Texts detected with >= 70% confidence as `native_lang` flip dynamically to `target_lang`.
///   - Texts detected with >= 70% confidence as `target_lang` (or any other language) translate to `native_lang`.
/// - When `smart_bidirectional` is `false` (Double-Alt in-place input mode):
///   - Always translates into `target_lang` and never flips back to `native_lang`.
pub fn resolve_bidirectional_direction(
    text: &str,
    native_lang: &str,
    target_lang: &str,
    smart_bidirectional: bool,
    explicit_source: Option<&str>,
) -> BidirectionalResolution {
    let native = normalize_lang(native_lang, "fa");
    let target = normalize_lang(target_lang, "en");
    let det = detect_dominant_language(text, &[native, target]);

    if !smart_bidirectional {
        let eff_src = if det.is_confident
            && det
                .lang_code
                .as_deref()
                .is_some_and(|c| !c.eq_ignore_ascii_case(target))
        {
            det.lang_code.clone().unwrap_or_else(|| "auto".to_string())
        } else {
            "auto".to_string()
        };
        return BidirectionalResolution {
            effective_source: eff_src,
            effective_target: target.to_string(),
            detected_lang: det.lang_code,
            detection_confidence: det.confidence,
            is_confident: det.is_confident,
            was_reversed: false,
        };
    }

    if let Some(src) = explicit_source
        && !src.eq_ignore_ascii_case("auto")
    {
        let resolved_target =
            if src.eq_ignore_ascii_case(native) && !native.eq_ignore_ascii_case(target) {
                target
            } else {
                native
            };
        return BidirectionalResolution {
            effective_source: src.to_string(),
            effective_target: resolved_target.to_string(),
            detected_lang: Some(src.to_string()),
            detection_confidence: 1.0,
            is_confident: true,
            was_reversed: resolved_target.eq_ignore_ascii_case(target)
                && !native.eq_ignore_ascii_case(target),
        };
    }

    if det.is_confident
        && let Some(detected) = det.lang_code.as_deref()
    {
        if detected.eq_ignore_ascii_case(native) && !native.eq_ignore_ascii_case(target) {
            return BidirectionalResolution {
                effective_source: native.to_string(),
                effective_target: target.to_string(),
                detected_lang: det.lang_code,
                detection_confidence: det.confidence,
                is_confident: true,
                was_reversed: true,
            };
        }

        return BidirectionalResolution {
            effective_source: detected.to_string(),
            effective_target: native.to_string(),
            detected_lang: det.lang_code,
            detection_confidence: det.confidence,
            is_confident: true,
            was_reversed: false,
        };
    }

    BidirectionalResolution {
        effective_source: "auto".to_string(),
        effective_target: native.to_string(),
        detected_lang: det.lang_code,
        detection_confidence: det.confidence,
        is_confident: det.is_confident,
        was_reversed: false,
    }
}

fn normalize_lang<'a>(code: &'a str, fallback: &'a str) -> &'a str {
    let trimmed = code.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("auto") {
        fallback
    } else {
        trimmed
    }
}

fn is_latin_lang(code: &str) -> bool {
    matches!(
        code.to_ascii_lowercase().as_str(),
        "en" | "tr" | "de" | "fr" | "es" | "it" | "pt" | "nl"
    )
}

fn strip_urls_and_code_tokens(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for token in input.split_whitespace() {
        let lower = token.to_ascii_lowercase();
        if lower.starts_with("http://")
            || lower.starts_with("https://")
            || lower.starts_with("www.")
            || (token.contains('@') && token.contains('.'))
        {
            continue;
        }
        out.push_str(token);
        out.push(' ');
    }
    out
}

fn tokenize_words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphabetic() && c != '\u{200C}')
        .map(|w| w.trim_matches('\u{200C}').to_string())
        .filter(|w| !w.is_empty())
        .collect()
}

fn count_hits(words: &[String], lexicon: &[&str]) -> usize {
    let set: HashSet<&str> = lexicon.iter().copied().collect();
    words.iter().filter(|w| set.contains(w.as_str())).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_english_and_persian_above_70_percent_with_minor_loanwords() {
        let persian_with_tech =
            "ما امروز سرویس API جدید را برای ترجمه خودکار متون راه‌اندازی کردیم و عالی کار می‌کند";
        let det_fa = detect_dominant_language(persian_with_tech, &["fa", "en"]);
        assert_eq!(det_fa.lang_code.as_deref(), Some("fa"));
        assert!(det_fa.is_confident);
        assert!(det_fa.confidence >= 0.70);

        let english_with_persian =
            "Please verify how the keyword کتاب behaves inside this paragraph.";
        let det_en = detect_dominant_language(english_with_persian, &["fa", "en"]);
        assert_eq!(det_en.lang_code.as_deref(), Some("en"));
        assert!(det_en.is_confident);
        assert!(det_en.confidence >= 0.70);
    }

    #[test]
    fn mixed_50_50_text_stays_below_confidence_threshold() {
        let balanced = "hello world developer testing سلام دنیا برنامه نویس آزمایش";
        let det = detect_dominant_language(balanced, &["fa", "en"]);
        assert!(!det.is_confident);
        assert!(det.confidence < 0.70);
    }

    #[test]
    fn resolves_bidirectional_modal_and_keeps_inplace_strictly_target() {
        let res_en = resolve_bidirectional_direction(
            "This is a clear English sentence for testing.",
            "fa",
            "en",
            true,
            None,
        );
        assert_eq!(res_en.effective_source, "en");
        assert_eq!(res_en.effective_target, "fa");
        assert!(!res_en.was_reversed);

        let res_fa = resolve_bidirectional_direction(
            "این یک جمله فارسی روشن برای بررسی جهت ترجمه است.",
            "fa",
            "en",
            true,
            None,
        );
        assert_eq!(res_fa.effective_source, "fa");
        assert_eq!(res_fa.effective_target, "en");
        assert!(res_fa.was_reversed);

        // In-place (Double-Alt) MUST always target target_lang (never flip English back to Persian)
        let inplace_en = resolve_bidirectional_direction(
            "This is already English inside an input box.",
            "fa",
            "en",
            false,
            None,
        );
        assert_eq!(inplace_en.effective_target, "en");
        assert!(!inplace_en.was_reversed);

        let inplace_fa = resolve_bidirectional_direction(
            "لطفا این پیام را به انگلیسی ترجمه کن.",
            "fa",
            "en",
            false,
            None,
        );
        assert_eq!(inplace_fa.effective_source, "fa");
        assert_eq!(inplace_fa.effective_target, "en");
    }

    #[test]
    fn disambiguates_turkish_and_arabic() {
        let tr_text = "Bugün hava çok güzel ve arkadaşlarımla şehir merkezinde buluşacağım.";
        let det_tr = detect_dominant_language(tr_text, &["tr", "en"]);
        assert_eq!(det_tr.lang_code.as_deref(), Some("tr"));
        assert!(det_tr.is_confident);

        let ar_text = "مرحبا كيف حالك اليوم في هذا الاجتماع المهم";
        let det_ar = detect_dominant_language(ar_text, &["fa", "en"]);
        assert_eq!(det_ar.lang_code.as_deref(), Some("ar"));
        assert!(det_ar.is_confident);
    }
}
