//! Kartendatenbank. Quelle ist die freie API von YGOPRODeck
//! (https://ygoprodeck.com/api-guide/). Die Daten werden einmal komplett
//! geladen und lokal gespeichert, damit die API kaum belastet wird.

use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::model::{CardKind, Slot};

pub const API_BASE: &str = "https://db.ygoprodeck.com/api/v7";
pub const USER_AGENT: &str = "ygo-combomaker/0.1 (Desktop-App zum Combo-Ueben)";
/// Die komplette Datenbank ist mehrere MB groß. ureq begrenzt standardmäßig
/// auf 10 MB, deshalb wird das Limit erhöht.
const MAX_BODY: u64 = 300 * 1024 * 1024;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Card {
    pub id: u32,
    pub image_id: u32,
    pub name: String,
    #[serde(default)]
    pub name_de: Option<String>,
    pub kind: CardKind,
    /// z. B. "Effect Monster", "Fusion Monster", "Spell Card".
    pub type_line: String,
    #[serde(default)]
    pub archetype: Option<String>,
    #[serde(default)]
    pub desc: String,
    #[serde(default)]
    pub in_master_duel: bool,
    /// IDs alternativer Artworks. Decklisten (YDK) enthalten manchmal diese IDs.
    #[serde(default)]
    pub alt_ids: Vec<u32>,
}

impl Card {
    pub fn to_slot(&self) -> Slot {
        Slot::Card {
            id: self.id,
            image_id: self.image_id,
            name: self.name.clone(),
        }
    }
}

/// So wird die Datenbank auf der Festplatte gespeichert.
#[derive(Serialize, Deserialize, Default)]
pub struct CardDbFile {
    pub version: String,
    pub cards: Vec<Card>,
}

#[derive(Default)]
pub struct CardDb {
    pub version: String,
    pub cards: Vec<Card>,
    /// Kleingeschriebene Namen (englisch + deutsch) für die Suche.
    search_keys: Vec<String>,
    by_id: std::collections::HashMap<u32, usize>,
    /// `false`, falls die Daten keine Format-Angaben enthalten. Dann wird
    /// der Master-Duel-Filter ignoriert, damit die Suche nicht leer bleibt.
    has_md_info: bool,
}

impl CardDb {
    pub fn from_file(file: CardDbFile) -> Self {
        let mut db = CardDb {
            version: file.version,
            cards: file.cards,
            ..Default::default()
        };
        db.rebuild_index();
        db
    }

    fn rebuild_index(&mut self) {
        self.search_keys = self
            .cards
            .iter()
            .map(|c| {
                let mut key = c.name.to_lowercase();
                if let Some(de) = &c.name_de {
                    key.push('\u{1}');
                    key.push_str(&de.to_lowercase());
                }
                key
            })
            .collect();
        self.by_id.clear();
        for (i, c) in self.cards.iter().enumerate() {
            for alt in &c.alt_ids {
                self.by_id.entry(*alt).or_insert(i);
            }
        }
        for (i, c) in self.cards.iter().enumerate() {
            self.by_id.insert(c.id, i);
        }
        self.has_md_info = self.cards.iter().any(|c| c.in_master_duel);
    }

    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }

    pub fn get(&self, id: u32) -> Option<&Card> {
        self.by_id.get(&id).map(|&i| &self.cards[i])
    }

    /// Sucht eine Karte über den genauen Namen (englisch oder deutsch,
    /// Groß-/Kleinschreibung egal).
    pub fn find_by_name(&self, name: &str) -> Option<&Card> {
        let name = name.trim().to_lowercase();
        self.cards.iter().find(|c| {
            c.name.to_lowercase() == name
                || c.name_de.as_ref().is_some_and(|d| d.to_lowercase() == name)
        })
    }

    /// Alle Archetypen, alphabetisch sortiert.
    pub fn archetypes(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .cards
            .iter()
            .filter_map(|c| c.archetype.clone())
            .collect();
        v.sort();
        v.dedup();
        v
    }

    /// Alle Karten eines Archetyps.
    pub fn cards_of_archetype(&self, archetype: &str, md_only: bool) -> Vec<&Card> {
        let md_only = md_only && self.has_md_info;
        self.cards
            .iter()
            .filter(|c| c.archetype.as_deref() == Some(archetype))
            .filter(|c| !md_only || c.in_master_duel)
            .collect()
    }

    /// Sucht Karten, deren englischer oder deutscher Name alle Suchwörter
    /// enthält. Treffer, die mit dem Suchtext beginnen, kommen zuerst.
    pub fn search(&self, query: &str, md_only: bool, limit: usize) -> Vec<&Card> {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return Vec::new();
        }
        let md_only = md_only && self.has_md_info;
        let words: Vec<&str> = query.split_whitespace().collect();
        let mut hits: Vec<(u8, &str, &Card)> = self
            .cards
            .iter()
            .zip(&self.search_keys)
            .filter(|(c, _)| !md_only || c.in_master_duel)
            .filter(|(_, key)| words.iter().all(|w| key.contains(w)))
            .map(|(c, key)| {
                let rank = if key == &query {
                    0
                } else if key.starts_with(&query) {
                    1
                } else {
                    2
                };
                (rank, c.name.as_str(), c)
            })
            .collect();
        hits.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(b.1)));
        hits.into_iter().take(limit).map(|(_, _, c)| c).collect()
    }
}

// ---------------------------------------------------------------------------
// Laden und Speichern
// ---------------------------------------------------------------------------

pub fn load_from_disk(path: &Path) -> Result<CardDbFile> {
    let bytes = std::fs::read(path).with_context(|| format!("Lesen von {}", path.display()))?;
    Ok(serde_json::from_slice(&bytes)?)
}

pub fn save_to_disk(path: &Path, file: &CardDbFile) -> Result<()> {
    crate::storage::write_atomic(path, &serde_json::to_vec(file)?)
}

// ---------------------------------------------------------------------------
// YGOPRODeck-API
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ApiResponse {
    data: Vec<ApiCard>,
}

#[derive(Deserialize)]
struct ApiCard {
    id: u32,
    name: String,
    #[serde(rename = "type", default)]
    type_line: String,
    #[serde(default)]
    desc: String,
    #[serde(default)]
    archetype: Option<String>,
    #[serde(default)]
    card_images: Vec<ApiImage>,
    #[serde(default)]
    misc_info: Vec<ApiMisc>,
}

#[derive(Deserialize)]
struct ApiImage {
    id: u32,
}

#[derive(Deserialize)]
struct ApiMisc {
    #[serde(default)]
    formats: Vec<String>,
}

#[derive(Deserialize)]
struct ApiDbVersion {
    database_version: serde_json::Value,
}

fn kind_from_type(type_line: &str) -> CardKind {
    if type_line.contains("Spell") {
        CardKind::Spell
    } else if type_line.contains("Trap") {
        CardKind::Trap
    } else {
        CardKind::Monster
    }
}

/// Wandelt die API-Antwort in unsere kompakte Kartenliste um.
pub fn parse_api_cards(json: &[u8]) -> Result<Vec<Card>> {
    let resp: ApiResponse =
        serde_json::from_slice(json).context("API-Antwort ist kein gültiges JSON")?;
    Ok(resp
        .data
        .into_iter()
        .map(|c| {
            let in_md = c
                .misc_info
                .iter()
                .any(|m| m.formats.iter().any(|f| f == "Master Duel"));
            Card {
                id: c.id,
                image_id: c.card_images.first().map(|i| i.id).unwrap_or(c.id),
                kind: kind_from_type(&c.type_line),
                name: c.name,
                name_de: None,
                type_line: c.type_line,
                archetype: c.archetype,
                desc: c.desc,
                in_master_duel: in_md,
                alt_ids: c.card_images.iter().skip(1).map(|i| i.id).collect(),
            }
        })
        .collect())
}

/// Liest aus einer deutschen API-Antwort nur ID und Namen.
pub fn parse_api_names(json: &[u8]) -> Result<Vec<(u32, String)>> {
    let resp: ApiResponse = serde_json::from_slice(json)?;
    Ok(resp.data.into_iter().map(|c| (c.id, c.name)).collect())
}

fn http_get(url: &str) -> Result<Vec<u8>> {
    let mut resp = ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .with_context(|| format!("Anfrage an {url} fehlgeschlagen"))?;
    Ok(resp
        .body_mut()
        .with_config()
        .limit(MAX_BODY)
        .read_to_vec()?)
}

pub fn fetch_db_version() -> Result<String> {
    let bytes = http_get(&format!("{API_BASE}/checkDBVer.php"))?;
    let v: Vec<ApiDbVersion> = serde_json::from_slice(&bytes)?;
    let v = v.first().context("Leere Versionsantwort")?;
    Ok(match &v.database_version {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    })
}

/// Lädt die komplette Datenbank (2 Anfragen: Englisch mit Formaten,
/// danach deutsche Namen). Schlägt der deutsche Teil fehl, bleibt nur
/// Englisch.
pub fn fetch_full_db(progress: &dyn Fn(&str)) -> Result<CardDbFile> {
    progress("Prüfe Datenbank-Version …");
    let version = fetch_db_version().unwrap_or_default();

    progress("Lade alle Karten (englisch) …");
    let bytes = http_get(&format!("{API_BASE}/cardinfo.php?misc=yes"))?;
    progress("Verarbeite Karten …");
    let mut cards = parse_api_cards(&bytes)?;

    progress("Lade deutsche Kartennamen …");
    // Pause, damit die zwei großen Anfragen nicht direkt hintereinander kommen.
    std::thread::sleep(std::time::Duration::from_millis(500));
    if let Ok(bytes) = http_get(&format!("{API_BASE}/cardinfo.php?language=de"))
        && let Ok(names) = parse_api_names(&bytes)
    {
        let map: std::collections::HashMap<u32, String> = names.into_iter().collect();
        for c in &mut cards {
            c.name_de = map.get(&c.id).cloned();
        }
    }
    Ok(CardDbFile { version, cards })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"data":[
      {"id":46986414,"name":"Dark Magician","type":"Normal Monster","frameType":"normal",
       "desc":"The ultimate wizard.","atk":2500,"def":2100,"level":7,"race":"Spellcaster",
       "attribute":"DARK","archetype":"Dark Magician",
       "card_images":[{"id":46986414,"image_url":"x","image_url_small":"y","image_url_cropped":"z"},
                      {"id":36996508,"image_url":"x","image_url_small":"y","image_url_cropped":"z"}],
       "misc_info":[{"formats":["TCG","OCG","Master Duel"],"md_rarity":"UR"}]},
      {"id":24094653,"name":"Polymerization","type":"Spell Card","frameType":"spell",
       "desc":"Fusion Summon.","race":"Normal",
       "card_images":[{"id":24094653}],"misc_info":[{"formats":["TCG"]}]},
      {"id":1,"name":"Solemn Judgment","type":"Trap Card","desc":"","card_images":[]}
    ]}"#;

    #[test]
    fn parses_api_sample() {
        let cards = parse_api_cards(SAMPLE.as_bytes()).unwrap();
        assert_eq!(cards.len(), 3);
        assert_eq!(cards[0].kind, CardKind::Monster);
        assert!(cards[0].in_master_duel);
        assert_eq!(cards[0].image_id, 46986414);
        assert_eq!(cards[1].kind, CardKind::Spell);
        assert!(!cards[1].in_master_duel);
        assert_eq!(cards[2].kind, CardKind::Trap);
        assert_eq!(cards[2].image_id, 1);
    }

    #[test]
    fn search_ranks_and_filters() {
        let mut cards = parse_api_cards(SAMPLE.as_bytes()).unwrap();
        cards[1].name_de = Some("Polymerisation".into());
        let db = CardDb::from_file(CardDbFile {
            version: "1".into(),
            cards,
        });
        // Deutscher Name wird gefunden.
        assert_eq!(db.search("polymerisation", false, 10)[0].id, 24094653);
        // Mehrere Wörter, Groß-/Kleinschreibung egal.
        assert_eq!(db.search("MAGICIAN dark", false, 10).len(), 1);
        // Master-Duel-Filter.
        assert!(db.search("poly", true, 10).is_empty());
        assert_eq!(db.get(46986414).unwrap().name, "Dark Magician");
        // Alternatives Artwork führt zur gleichen Karte.
        assert_eq!(db.get(36996508).unwrap().id, 46986414);
        assert_eq!(db.find_by_name("POLYMERISATION").unwrap().id, 24094653);
        assert_eq!(db.archetypes(), vec!["Dark Magician".to_string()]);
    }

    #[test]
    fn md_filter_ignored_without_format_info() {
        let mut cards = parse_api_cards(SAMPLE.as_bytes()).unwrap();
        for c in &mut cards {
            c.in_master_duel = false;
        }
        let db = CardDb::from_file(CardDbFile {
            version: "1".into(),
            cards,
        });
        assert_eq!(db.search("poly", true, 10).len(), 1);
    }
}
