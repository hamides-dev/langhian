use aho_corasick::{AhoCorasick, MatchKind};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VocabEntry {
    pub id: String,
    pub term: String,
    pub translation: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub source_lang: String,
    #[serde(default)]
    pub target_lang: String,
    pub created_at: DateTime<Utc>,
}

impl VocabEntry {
    pub fn new(
        term: impl Into<String>,
        translation: impl Into<String>,
        notes: impl Into<String>,
        source_lang: impl Into<String>,
        target_lang: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            term: term.into().trim().to_string(),
            translation: translation.into().trim().to_string(),
            notes: notes.into().trim().to_string(),
            source_lang: source_lang.into(),
            target_lang: target_lang.into(),
            created_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextSegment {
    Plain(String),
    Saved { text: String, entry: VocabEntry },
}

pub struct VocabMatcher {
    entries: Vec<VocabEntry>,
    automaton: Option<AhoCorasick>,
}

impl VocabMatcher {
    pub fn build(entries: Vec<VocabEntry>) -> Self {
        let filtered: Vec<VocabEntry> = entries
            .into_iter()
            .filter(|e| !e.term.trim().is_empty())
            .collect();

        if filtered.is_empty() {
            return Self {
                entries: Vec::new(),
                automaton: None,
            };
        }

        let patterns: Vec<&str> = filtered.iter().map(|e| e.term.as_str()).collect();
        let automaton = AhoCorasick::builder()
            .ascii_case_insensitive(true)
            .match_kind(MatchKind::LeftmostLongest)
            .build(patterns)
            .ok();

        Self {
            entries: filtered,
            automaton,
        }
    }

    pub fn highlight_segments(&self, text: &str) -> Vec<TextSegment> {
        let Some(ac) = &self.automaton else {
            return if text.is_empty() {
                Vec::new()
            } else {
                vec![TextSegment::Plain(text.to_string())]
            };
        };

        let mut segments = Vec::new();
        let mut cursor = 0usize;

        for mat in ac.find_iter(text) {
            let start = mat.start();
            let end = mat.end();
            if start < cursor {
                continue;
            }
            if !has_word_boundaries(text, start, end) {
                continue;
            }

            if start > cursor {
                segments.push(TextSegment::Plain(text[cursor..start].to_string()));
            }
            let entry = self.entries[mat.pattern().as_usize()].clone();
            segments.push(TextSegment::Saved {
                text: text[start..end].to_string(),
                entry,
            });
            cursor = end;
        }

        if cursor < text.len() {
            segments.push(TextSegment::Plain(text[cursor..].to_string()));
        }

        segments
    }
}

fn has_word_boundaries(text: &str, start: usize, end: usize) -> bool {
    let matched = &text[start..end];
    let first_is_word = matched.chars().next().is_some_and(is_word_char);
    let last_is_word = matched.chars().next_back().is_some_and(is_word_char);

    let left_ok = if first_is_word {
        text[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !is_word_char(c))
    } else {
        true
    };

    let right_ok = if last_is_word {
        text[end..].chars().next().is_none_or(|c| !is_word_char(c))
    } else {
        true
    };

    left_ok && right_ok
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '\u{200C}'
}

pub fn find_vocab_match_index(
    entries: &[VocabEntry],
    term: &str,
    translation: &str,
) -> Option<usize> {
    let t = term.trim();
    let tr = translation.trim();
    if t.is_empty() {
        return None;
    }
    entries.iter().position(|e| {
        e.term.trim().eq_ignore_ascii_case(t)
            && (tr.is_empty() || e.translation.trim().eq_ignore_ascii_case(tr))
    })
}

pub fn export_vocab_json(entries: &[VocabEntry]) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(entries)
}

pub fn export_vocab_csv(entries: &[VocabEntry]) -> String {
    let mut out = String::from("term,translation,notes,source_lang,target_lang,created_at\n");
    for e in entries {
        out.push_str(&format!(
            "{},{},{},{},{},{}\n",
            csv_escape(&e.term),
            csv_escape(&e.translation),
            csv_escape(&e.notes),
            csv_escape(&e.source_lang),
            csv_escape(&e.target_lang),
            csv_escape(&e.created_at.to_rfc3339())
        ));
    }
    out
}

fn csv_escape(value: &str) -> String {
    let escaped = value.replace('"', "\"\"");
    format!("\"{escaped}\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_saved_words_and_phrases_respecting_boundaries() {
        let entries = vec![
            VocabEntry::new("art", "هنر", "", "en", "fa"),
            VocabEntry::new("state of the art", "پیشرفته", "", "en", "fa"),
            VocabEntry::new("کتاب", "book", "", "fa", "en"),
        ];
        let matcher = VocabMatcher::build(entries);

        // Should NOT match "start" or "smart", SHOULD match "state of the art" as longest phrase
        let segs = matcher.highlight_segments("Start using State of the Art tools and کتاب.");
        let saved: Vec<String> = segs
            .iter()
            .filter_map(|s| match s {
                TextSegment::Saved { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(saved, vec!["State of the Art", "کتاب"]);
    }
}
