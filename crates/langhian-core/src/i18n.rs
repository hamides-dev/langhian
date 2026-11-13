use crate::rtl::TextDirection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const EN_JSON: &str = include_str!("../../../assets/locales/en.json");
const FA_JSON: &str = include_str!("../../../assets/locales/fa.json");
const TR_JSON: &str = include_str!("../../../assets/locales/tr.json");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocaleMeta {
    pub code: String,
    pub name: String,
    pub native_name: String,
    pub dir: String,
}

impl std::fmt::Display for LocaleMeta {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.name == self.native_name {
            write!(f, "{}", self.native_name)
        } else {
            write!(f, "{} ({})", self.native_name, self.name)
        }
    }
}

#[derive(Debug, Clone)]
struct LoadedLocale {
    meta: LocaleMeta,
    tree: Value,
}

#[derive(Debug, Clone)]
pub struct Translator {
    locales: BTreeMap<String, LoadedLocale>,
    active_code: String,
}

impl Default for Translator {
    fn default() -> Self {
        Self::new("en", None)
    }
}

impl Translator {
    pub fn new(active_code: &str, custom_locales_dir: Option<&Path>) -> Self {
        let mut locales = BTreeMap::new();
        for raw in [EN_JSON, FA_JSON, TR_JSON] {
            if let Ok(loaded) = parse_locale(raw) {
                locales.insert(loaded.meta.code.clone(), loaded);
            }
        }

        if let Some(dir) = custom_locales_dir
            && let Ok(entries) = std::fs::read_dir(dir)
        {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("json")
                    && let Ok(raw) = std::fs::read_to_string(&path)
                    && let Ok(loaded) = parse_locale(&raw)
                {
                    locales.insert(loaded.meta.code.clone(), loaded);
                }
            }
        }

        let resolved = if locales.contains_key(active_code) {
            active_code.to_string()
        } else {
            "en".to_string()
        };

        Self {
            locales,
            active_code: resolved,
        }
    }

    pub fn set_language(&mut self, code: &str) {
        if self.locales.contains_key(code) {
            self.active_code = code.to_string();
        }
    }

    pub fn active_code(&self) -> &str {
        &self.active_code
    }

    pub fn direction(&self) -> TextDirection {
        let is_rtl = self
            .locales
            .get(&self.active_code)
            .is_some_and(|l| l.meta.dir.eq_ignore_ascii_case("rtl"));
        if is_rtl {
            TextDirection::Rtl
        } else {
            TextDirection::Ltr
        }
    }

    pub fn available_locales(&self) -> Vec<LocaleMeta> {
        self.locales.values().map(|l| l.meta.clone()).collect()
    }

    pub fn t(&self, key_path: &str) -> String {
        if let Some(loc) = self.locales.get(&self.active_code)
            && let Some(val) = lookup_path(&loc.tree, key_path)
        {
            return val.to_string();
        }
        if let Some(en) = self.locales.get("en")
            && let Some(val) = lookup_path(&en.tree, key_path)
        {
            return val.to_string();
        }
        key_path.to_string()
    }
}

fn parse_locale(raw: &str) -> Result<LoadedLocale, serde_json::Error> {
    let tree: Value = serde_json::from_str(raw)?;
    let meta: LocaleMeta = serde_json::from_value(
        tree.get("meta")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
    )?;
    Ok(LoadedLocale { meta, tree })
}

fn lookup_path<'a>(root: &'a Value, path: &str) -> Option<&'a str> {
    let mut current = root;
    for part in path.split('.') {
        current = current.get(part)?;
    }
    current.as_str()
}

pub fn collect_leaf_keys(value: &Value, prefix: &str, out: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                let next = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                collect_leaf_keys(v, &next, out);
            }
        }
        Value::String(_) => {
            out.insert(prefix.to_string());
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_bundled_locales_have_complete_key_parity() {
        let en = parse_locale(EN_JSON).expect("valid en.json");
        let fa = parse_locale(FA_JSON).expect("valid fa.json");
        let tr = parse_locale(TR_JSON).expect("valid tr.json");

        let mut en_keys = BTreeSet::new();
        let mut fa_keys = BTreeSet::new();
        let mut tr_keys = BTreeSet::new();
        collect_leaf_keys(&en.tree, "", &mut en_keys);
        collect_leaf_keys(&fa.tree, "", &mut fa_keys);
        collect_leaf_keys(&tr.tree, "", &mut tr_keys);

        let missing_fa: Vec<_> = en_keys.difference(&fa_keys).collect();
        let missing_tr: Vec<_> = en_keys.difference(&tr_keys).collect();
        assert!(
            missing_fa.is_empty(),
            "Missing keys in fa.json: {missing_fa:?}"
        );
        assert!(
            missing_tr.is_empty(),
            "Missing keys in tr.json: {missing_tr:?}"
        );
    }

    #[test]
    fn translator_resolves_rtl_for_persian() {
        let mut t = Translator::default();
        assert_eq!(t.direction(), TextDirection::Ltr);
        assert_eq!(t.t("nav.translate"), "Translate");

        t.set_language("fa");
        assert_eq!(t.direction(), TextDirection::Rtl);
        assert_eq!(t.t("nav.translate"), "ترجمه");

        t.set_language("tr");
        assert_eq!(t.direction(), TextDirection::Ltr);
        assert_eq!(t.t("nav.translate"), "Çeviri");
    }
}
