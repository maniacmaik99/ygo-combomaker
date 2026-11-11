//! Forbidden & Limited Lists ("banlists") for Master Duel, TCG and OCG.
//!
//! Source: the YAML Yugi project (DawnbrandBots/yaml-yugi-limit-regulation),
//! which publishes the current lists as small JSON files and updates them
//! daily. The lists use Konami's card database IDs, which the card database
//! of this app stores per card (`Card::konami_id`).

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::cards::CardDb;

/// Game version whose banlist is shown.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Format {
    #[default]
    MasterDuel,
    Tcg,
    Ocg,
}

impl Format {
    pub const ALL: [Format; 3] = [Format::MasterDuel, Format::Tcg, Format::Ocg];

    pub fn label(self) -> &'static str {
        match self {
            Format::MasterDuel => "Master Duel",
            Format::Tcg => "TCG",
            Format::Ocg => "OCG",
        }
    }

    fn slug(self) -> &'static str {
        match self {
            Format::MasterDuel => "master-duel",
            Format::Tcg => "tcg",
            Format::Ocg => "ocg",
        }
    }

    fn index(self) -> usize {
        self as usize
    }

    fn url(self) -> String {
        format!(
            "https://dawnbrandbots.github.io/yaml-yugi-limit-regulation/{}/current.vector.json",
            self.slug()
        )
    }
}

/// Restriction of one card.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Limit {
    Forbidden,
    Limited,
    SemiLimited,
}

impl Limit {
    /// Number of copies allowed in a deck.
    pub fn max_copies(self) -> u8 {
        match self {
            Limit::Forbidden => 0,
            Limit::Limited => 1,
            Limit::SemiLimited => 2,
        }
    }

    pub fn label(self) -> &'static str {
        crate::i18n::tr(match self {
            Limit::Forbidden => "ban.forbidden",
            Limit::Limited => "ban.limited",
            Limit::SemiLimited => "ban.semi",
        })
    }

    fn from_copies(n: u8) -> Option<Self> {
        match n {
            0 => Some(Limit::Forbidden),
            1 => Some(Limit::Limited),
            2 => Some(Limit::SemiLimited),
            _ => None,
        }
    }
}

/// One list as published: effective date and allowed copies per Konami ID.
#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
pub struct List {
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub regulation: HashMap<String, u8>,
}

/// The lists of all formats, as far as they are available.
#[derive(Clone, Default, Debug)]
pub struct Banlists {
    lists: [Option<List>; 3],
}

fn cache_file(dir: &Path, format: Format) -> std::path::PathBuf {
    dir.join(format!("banlist-{}.json", format.slug()))
}

impl Banlists {
    /// Lists saved by an earlier start (used until the download finishes
    /// and when there is no internet connection).
    pub fn load_cached(dir: &Path) -> Self {
        let mut out = Self::default();
        for f in Format::ALL {
            out.lists[f.index()] = std::fs::read(cache_file(dir, f))
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok());
        }
        out
    }

    /// Downloads the current lists. A list that cannot be downloaded keeps
    /// its cached version. Fails only if no list could be downloaded.
    pub fn fetch(dir: &Path) -> Result<Self> {
        let mut out = Self::load_cached(dir);
        let mut last_err = None;
        let mut ok = 0;
        for f in Format::ALL {
            let got = crate::cards::http_get(&f.url()).and_then(|bytes| {
                let list: List = serde_json::from_slice(&bytes).context("banlist: invalid JSON")?;
                let _ = crate::storage::write_atomic(&cache_file(dir, f), &bytes);
                Ok(list)
            });
            match got {
                Ok(list) => {
                    out.lists[f.index()] = Some(list);
                    ok += 1;
                }
                Err(e) => last_err = Some(e),
            }
        }
        match last_err {
            Some(e) if ok == 0 => Err(e),
            _ => Ok(out),
        }
    }

    /// Effective date of the list, if the list is available.
    pub fn date(&self, format: Format) -> Option<&str> {
        self.lists[format.index()].as_ref().map(|l| l.date.as_str())
    }

    /// Restricted cards of a format, by card ID (alternative artworks included).
    pub fn index(&self, format: Format, db: &CardDb) -> HashMap<u32, Limit> {
        let mut out = HashMap::new();
        let Some(list) = &self.lists[format.index()] else {
            return out;
        };
        let by_konami: HashMap<u32, Limit> = list
            .regulation
            .iter()
            .filter_map(|(k, n)| Some((k.parse().ok()?, Limit::from_copies(*n)?)))
            .collect();
        for c in &db.cards {
            if let Some(limit) = c.konami_id.and_then(|k| by_konami.get(&k)) {
                out.insert(c.id, *limit);
                for alt in &c.alt_ids {
                    out.insert(*alt, *limit);
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{Card, CardDbFile};
    use crate::model::CardKind;

    #[test]
    fn index_maps_konami_ids_to_cards() {
        let list: List = serde_json::from_str(
            r#"{"date":"2026-11-05","regulation":{"4007":0,"4010":1,"4020":2,"9999":1}}"#,
        )
        .unwrap();
        let mut lists = Banlists::default();
        lists.lists[Format::MasterDuel.index()] = Some(list);
        let card = |id: u32, konami: Option<u32>, alt: Vec<u32>| Card {
            id,
            image_id: id,
            name: format!("Card {id}"),
            name_de: None,
            names: Default::default(),
            kind: CardKind::Monster,
            type_line: "Effect Monster".into(),
            archetype: None,
            desc: String::new(),
            in_master_duel: true,
            alt_ids: alt,
            konami_id: konami,
        };
        let db = CardDb::from_file(CardDbFile {
            version: "t".into(),
            cards: vec![
                card(1, Some(4007), vec![11]),
                card(2, Some(4010), vec![]),
                card(3, Some(4020), vec![]),
                card(4, Some(5000), vec![]),
                card(5, None, vec![]),
            ],
        });
        let idx = lists.index(Format::MasterDuel, &db);
        assert_eq!(idx.get(&1), Some(&Limit::Forbidden));
        assert_eq!(idx.get(&11), Some(&Limit::Forbidden));
        assert_eq!(idx.get(&2), Some(&Limit::Limited));
        assert_eq!(idx.get(&3), Some(&Limit::SemiLimited));
        assert_eq!(idx.get(&4), None);
        assert_eq!(idx.get(&5), None);
        assert_eq!(lists.date(Format::MasterDuel), Some("2026-11-05"));
        // No list for this format: nothing is restricted, date unknown.
        assert!(lists.index(Format::Tcg, &db).is_empty());
        assert_eq!(lists.date(Format::Tcg), None);
        assert_eq!(Limit::Limited.max_copies(), 1);
    }
}
