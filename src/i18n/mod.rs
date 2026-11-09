//! Übersetzungen. Jede Sprache hat eine Tabelle `(Schlüssel, Text)` in
//! einer eigenen Datei. Fehlt ein Schlüssel, wird Englisch benutzt, und
//! fehlt er auch dort, der Schlüssel selbst.
//!
//! Platzhalter in Texten: `{0}`, `{1}`, … (siehe [`trf`]).

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

use serde::{Deserialize, Serialize};

mod de;
mod en;
mod es;
mod fr;
mod it;
mod pt;

/// Sprachen, in denen Yu-Gi-Oh! offiziell erscheint und die von links nach
/// rechts geschrieben werden.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub enum Lang {
    #[default]
    En,
    De,
    Fr,
    It,
    Es,
    Pt,
}

impl Lang {
    pub const ALL: [Lang; 6] = [Lang::En, Lang::De, Lang::Fr, Lang::It, Lang::Es, Lang::Pt];

    /// Name der Sprache in der Sprache selbst.
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::De => "Deutsch",
            Lang::Fr => "Français",
            Lang::It => "Italiano",
            Lang::Es => "Español",
            Lang::Pt => "Português",
        }
    }

    /// Sprachcode, wie ihn die YGOPRODeck-API für Kartennamen nutzt.
    /// Spanisch bietet die API nicht an.
    pub fn card_api_code(self) -> Option<&'static str> {
        match self {
            Lang::De => Some("de"),
            Lang::Fr => Some("fr"),
            Lang::It => Some("it"),
            Lang::Pt => Some("pt"),
            Lang::En | Lang::Es => None,
        }
    }

    /// Sprache des Betriebssystems, falls unterstützt, sonst Englisch.
    pub fn from_system() -> Lang {
        let loc = sys_locale::get_locale().unwrap_or_default().to_lowercase();
        match loc.get(..2) {
            Some("de") => Lang::De,
            Some("fr") => Lang::Fr,
            Some("it") => Lang::It,
            Some("es") => Lang::Es,
            Some("pt") => Lang::Pt,
            _ => Lang::En,
        }
    }

    fn table(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Lang::En => en::T,
            Lang::De => de::T,
            Lang::Fr => fr::T,
            Lang::It => it::T,
            Lang::Es => es::T,
            Lang::Pt => pt::T,
        }
    }

    fn index(self) -> u8 {
        Lang::ALL.iter().position(|l| *l == self).unwrap_or(0) as u8
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set_lang(lang: Lang) {
    CURRENT.store(lang.index(), Ordering::Relaxed);
}

pub fn lang() -> Lang {
    Lang::ALL[CURRENT.load(Ordering::Relaxed) as usize % Lang::ALL.len()]
}

type Map = HashMap<&'static str, &'static str>;

fn maps() -> &'static HashMap<Lang, Map> {
    static MAPS: OnceLock<HashMap<Lang, Map>> = OnceLock::new();
    MAPS.get_or_init(|| {
        Lang::ALL
            .iter()
            .map(|l| (*l, l.table().iter().copied().collect()))
            .collect()
    })
}

/// Text für den Schlüssel in der aktuellen Sprache.
pub fn tr(key: &'static str) -> &'static str {
    let maps = maps();
    maps.get(&lang())
        .and_then(|m| m.get(key))
        .or_else(|| maps.get(&Lang::En).and_then(|m| m.get(key)))
        .copied()
        .unwrap_or(key)
}

/// Wie [`tr`], ersetzt aber `{0}`, `{1}`, … durch die Argumente.
pub fn trf(key: &'static str, args: &[&dyn std::fmt::Display]) -> String {
    let mut s = tr(key).to_string();
    for (i, a) in args.iter().enumerate() {
        s = s.replace(&format!("{{{i}}}"), &a.to_string());
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Jede Sprache muss genau die Schlüssel der englischen Tabelle haben
    /// und dieselben Platzhalter benutzen.
    #[test]
    fn all_languages_complete() {
        let en: Map = en::T.iter().copied().collect();
        assert_eq!(en.len(), en::T.len(), "doppelte Schlüssel in en.rs");
        for l in Lang::ALL {
            let m: Map = l.table().iter().copied().collect();
            assert_eq!(m.len(), l.table().len(), "doppelte Schlüssel in {l:?}");
            for (k, v) in &en {
                let t = m
                    .get(k)
                    .unwrap_or_else(|| panic!("{l:?}: Schlüssel {k} fehlt"));
                for i in 0..5 {
                    let ph = format!("{{{i}}}");
                    assert_eq!(
                        v.contains(&ph),
                        t.contains(&ph),
                        "{l:?}: Platzhalter {ph} in {k}"
                    );
                }
            }
            for k in m.keys() {
                assert!(en.contains_key(k), "{l:?}: unbekannter Schlüssel {k}");
            }
        }
    }

    #[test]
    fn format_and_fallback() {
        set_lang(Lang::De);
        assert_eq!(trf("test.format", &[&3, &"x"]), "3 und x");
        assert_eq!(tr("does.not.exist"), "does.not.exist");
        set_lang(Lang::En);
    }
}
