//! Decklisten einlesen. Unterstützt werden 3 Formate:
//! 1. ydke-Link: `ydke://<main>!<extra>!<side>!` (z. B. YGOPRODeck, EDOPro)
//! 2. YDK-Datei: Zeilen mit Passcodes, Abschnitte `#main`, `#extra`, `!side`
//! 3. Kartenliste: eine Karte pro Zeile, z. B. `3 Lunalight Gold Leo`,
//!    `3x Lunalight Gold Leo` oder `Lunalight Gold Leo x3`

use base64::Engine;

use crate::cards::CardDb;
use crate::model::Deck;

pub struct ImportResult {
    pub deck: Deck,
    /// Zeilen oder IDs, die keiner Karte zugeordnet werden konnten.
    pub unknown: Vec<String>,
}

/// Erkennt das Format automatisch. Side-Deck-Karten werden übersprungen.
pub fn import(text: &str, name: &str, db: &CardDb) -> ImportResult {
    let text = text.trim();
    let entries: Vec<(Result<u32, String>, u8)> = if let Some(rest) = text.strip_prefix("ydke://") {
        parse_ydke(rest).into_iter().map(|id| (Ok(id), 1)).collect()
    } else if looks_like_ydk(text) {
        parse_ydk(text).into_iter().map(|id| (Ok(id), 1)).collect()
    } else {
        parse_name_list(text)
            .into_iter()
            .map(|(n, c)| (Err(n), c))
            .collect()
    };

    let mut deck = Deck::new(name);
    let mut unknown = Vec::new();
    for (key, count) in entries {
        let card = match &key {
            Ok(id) => db.get(*id),
            Err(n) => db.find_by_name(n),
        };
        match card {
            Some(c) => deck.add(c.id, c.image_id, &c.name, count),
            None => unknown.push(match key {
                Ok(id) => id.to_string(),
                Err(n) => n,
            }),
        }
    }
    ImportResult { deck, unknown }
}

/// Main- und Extra-Deck aus einem ydke-Link (ohne `ydke://`).
fn parse_ydke(rest: &str) -> Vec<u32> {
    rest.split('!')
        .take(2) // main, extra; side wird ignoriert
        .filter_map(|part| {
            base64::engine::general_purpose::STANDARD
                .decode(part.trim())
                .ok()
        })
        .flat_map(|bytes| {
            let (chunks, _rest) = bytes.as_chunks::<4>();
            chunks
                .iter()
                .map(|c| u32::from_le_bytes(*c))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn looks_like_ydk(text: &str) -> bool {
    text.lines().any(|l| l.trim().eq_ignore_ascii_case("#main"))
        || text.lines().filter(|l| !l.trim().is_empty()).all(|l| {
            l.trim().starts_with('#')
                || l.trim().starts_with('!')
                || l.trim().parse::<u32>().is_ok()
        })
}

fn parse_ydk(text: &str) -> Vec<u32> {
    let mut out = Vec::new();
    let mut in_side = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('!') {
            in_side = true;
        } else if line.starts_with('#') {
            in_side = false;
        } else if !in_side && let Ok(id) = line.parse::<u32>() {
            out.push(id);
        }
    }
    out
}

/// Liest Zeilen wie "3 Name", "3x Name", "Name x3" oder nur "Name".
fn parse_name_list(text: &str) -> Vec<(String, u8)> {
    let mut out = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with("//") || line.ends_with(':') {
            continue;
        }
        let (name, count) = split_count(line);
        if !name.is_empty() {
            out.push((name.to_string(), count));
        }
    }
    out
}

fn split_count(line: &str) -> (&str, u8) {
    // Vorne: "3 Name" oder "3x Name"
    if let Some((first, rest)) = line.split_once(' ') {
        let n = first.trim_end_matches(['x', 'X']);
        if let Ok(c) = n.parse::<u8>()
            && (1..=3).contains(&c)
        {
            return (rest.trim(), c);
        }
    }
    // Hinten: "Name x3"
    if let Some((rest, last)) = line.rsplit_once(' ') {
        let n = last.trim_start_matches(['x', 'X']);
        if last.len() > n.len()
            && let Ok(c) = n.parse::<u8>()
            && (1..=3).contains(&c)
        {
            return (rest.trim(), c);
        }
    }
    (line, 1)
}

/// Erzeugt einen ydke-Link aus einem Deck (zum Weitergeben).
pub fn to_ydke(deck: &Deck) -> String {
    let mut bytes = Vec::new();
    for c in &deck.cards {
        for _ in 0..c.count {
            bytes.extend_from_slice(&c.id.to_le_bytes());
        }
    }
    let main = base64::engine::general_purpose::STANDARD.encode(bytes);
    format!("ydke://{main}!!!")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{CardDb, CardDbFile, parse_api_cards};

    fn db() -> CardDb {
        let json = r#"{"data":[
          {"id":46986414,"name":"Dark Magician","type":"Normal Monster","card_images":[{"id":46986414},{"id":36996508}]},
          {"id":24094653,"name":"Polymerization","type":"Spell Card","card_images":[{"id":24094653}]}
        ]}"#;
        let mut cards = parse_api_cards(json.as_bytes()).unwrap();
        cards[1].names.insert("de".into(), "Polymerisation".into());
        CardDb::from_file(CardDbFile {
            version: "1".into(),
            cards,
        })
    }

    #[test]
    fn ydke_roundtrip() {
        let db = db();
        let mut d = Deck::new("x");
        d.add(46986414, 46986414, "Dark Magician", 3);
        d.add(24094653, 24094653, "Polymerization", 1);
        let link = to_ydke(&d);
        let r = import(&link, "y", &db);
        assert!(r.unknown.is_empty());
        assert_eq!(r.deck.total(), 4);
    }

    #[test]
    fn ydk_with_alt_art_and_side() {
        let text = "#created by test\n#main\n36996508\n36996508\n#extra\n!side\n24094653\n";
        let r = import(text, "y", &db());
        assert_eq!(r.deck.cards.len(), 1);
        assert_eq!(r.deck.cards[0].id, 46986414);
        assert_eq!(r.deck.cards[0].count, 2);
    }

    #[test]
    fn name_list_formats() {
        let text = "Main Deck:\n3 Dark Magician\n2x Polymerisation\nUnbekannte Karte x2\n";
        let r = import(text, "y", &db());
        assert_eq!(r.deck.total(), 5);
        assert_eq!(r.unknown, vec!["Unbekannte Karte".to_string()]);
    }
}
