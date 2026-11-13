//! Text format for combos (experimental).
//!
//! A combo can be written as plain text instead of being clicked together
//! on the board. Card names may be shortened; they are looked up in the
//! card database. Example:
//!
//! ```text
//! # Gold Leo through Droll
//! deck: Lunalight
//! hand: Gold Leo + Foolish Burial
//!
//! ns Gold Leo
//! eff Gold Leo > send Yellow Marten
//!
//! link S:P Little Knight [emz] // uses Chick + Marten
//! end: Liger Dancer + S:P Little Knight
//! ```
//!
//! Rules:
//! - `# Title` starts a combo. One text may contain several combos.
//! - `deck:`, `author:`, `hand:`, `end:`, `through:` and `note:` are optional.
//!   Cards in a list are separated by `+`. In `hand:`, `|` separates
//!   alternatives for one card: `hand: Gold Leo + Foolish Burial | Wolf`.
//! - Every other line is one move: keyword, then the card name. Several
//!   moves in one line are separated by ` > `.
//! - An empty line (or `---`) starts a new step.
//! - `[zone]` at the end of a move picks a zone, `// text` adds a note.

use std::collections::{HashMap, HashSet};

use crate::board::{self, Board};
use crate::cards::{Card, CardDb};
use crate::i18n::{tr, trf};
use crate::model::{Action, CardKind, Combo, Connector, Deck, Entry, Slot, Step, StepPart, Zone};

/// Keywords for actions. The first keyword of an action is the one used
/// when a combo is written out as text.
pub const KEYWORDS: &[(&str, Action)] = &[
    ("ns", Action::NormalSummon),
    ("normal", Action::NormalSummon),
    ("ss", Action::SpecialSummon),
    ("special", Action::SpecialSummon),
    ("ts", Action::TributeSummon),
    ("set", Action::Set),
    ("flip", Action::FlipSummon),
    ("fusion", Action::FusionSummon),
    ("ritual", Action::RitualSummon),
    ("synchro", Action::SynchroSummon),
    ("xyz", Action::XyzSummon),
    ("pendulum", Action::PendulumSummon),
    ("link", Action::LinkSummon),
    ("act", Action::Activate),
    ("activate", Action::Activate),
    ("eff", Action::Activate),
    ("effect", Action::Activate),
    ("scale", Action::PlacePendulum),
    ("setst", Action::SetSpellTrap),
    ("place", Action::PlaceOnField),
    ("equip", Action::Equip),
    ("tribute", Action::Tribute),
    ("mat", Action::UseAsMaterial),
    ("material", Action::UseAsMaterial),
    ("attach", Action::AttachMaterial),
    ("detach", Action::DetachMaterial),
    ("discard", Action::Discard),
    ("search", Action::Search),
    ("add", Action::Search),
    ("draw", Action::Draw),
    ("send", Action::SendToGy),
    ("mill", Action::SendToGy),
    ("banish", Action::Banish),
    ("destroy", Action::Destroy),
    ("bounce", Action::ReturnToHand),
    ("gytohand", Action::GyToHand),
    ("gytodeck", Action::GyToDeck),
    ("banishedtohand", Action::BanishedToHand),
    ("banishedtogy", Action::BanishedToGy),
    ("todeck", Action::ReturnToDeck),
    ("toextra", Action::ReturnToExtraDeck),
    ("reveal", Action::Reveal),
    ("negate", Action::Negate),
    ("target", Action::Target),
    ("pos", Action::ChangePosition),
    ("card", Action::None),
];

/// Zone tags, written as `[tag]` at the end of a move.
pub const ZONE_TAGS: &[(&str, Zone)] = &[
    ("mz", Zone::MonsterZone),
    ("emz", Zone::ExtraMonsterZone),
    ("st", Zone::SpellTrapZone),
    ("pl", Zone::PendulumLeft),
    ("pr", Zone::PendulumRight),
    ("field", Zone::FieldZone),
    ("hand", Zone::Hand),
    ("gy", Zone::Graveyard),
];

/// Example shown in the syntax help.
pub const EXAMPLE: &str = "# Gold Leo combo\n\
deck: Lunalight\n\
hand: Gold Leo + any\n\
\n\
ns Gold Leo\n\
eff Gold Leo > send Yellow Marten\n\
\n\
eff Yellow Marten > search Kaleido Chick // from the graveyard\n\
cl2 Kaleido Chick > send Wolf\n\
scale Wolf [pl]\n\
fusion Liger Dancer [emz]\n\
\n\
end: Liger Dancer";

const HEADER_KEYS: &[&str] = &["title", "deck", "author", "hand", "end", "through", "note"];

/// Result of reading a text.
#[derive(Default, Debug)]
pub struct Parsed {
    pub combos: Vec<Combo>,
    /// Shortened or misspelled names and the card chosen for them.
    pub assumed: Vec<(String, String)>,
    /// Names without any matching card (inserted as placeholders).
    pub not_found: Vec<String>,
    /// Lines that could not be read or moves that do not work on the board.
    pub problems: Vec<String>,
}

impl Parsed {
    /// Summary for the message window.
    pub fn summary(&self) -> String {
        let mut out = trf("textimport.result", &[&self.combos.len()]);
        if !self.assumed.is_empty() {
            out.push_str("\n\n");
            out.push_str(tr("textimport.assumed"));
            for (a, b) in &self.assumed {
                out.push_str(&format!("\n  {a} → {b}"));
            }
        }
        if !self.not_found.is_empty() {
            out.push_str("\n\n");
            out.push_str(tr("textimport.not_found"));
            for n in &self.not_found {
                out.push_str(&format!("\n  {n}"));
            }
        }
        if !self.problems.is_empty() {
            out.push_str("\n\n");
            out.push_str(tr("textimport.problems"));
            for p in self.problems.iter().take(25) {
                out.push_str(&format!("\n  {p}"));
            }
            if self.problems.len() > 25 {
                out.push_str("\n  …");
            }
        }
        out
    }
}

/// Lowercase, only letters and digits, single spaces.
fn norm(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space = true;
    for ch in s.chars() {
        if ch.is_alphanumeric() {
            out.extend(ch.to_lowercase());
            space = false;
        } else if !space {
            out.push(' ');
            space = true;
        }
    }
    out.trim_end().to_string()
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let sub = prev[j] + usize::from(ca != cb);
            cur[j + 1] = sub.min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

enum Found {
    Exact(Slot),
    Assumed(Slot),
    Missing,
}

/// Looks up card names. Cards of the linked deck and of the archetype named
/// in `deck:` win when a short name fits several cards.
struct Resolver<'a> {
    db: &'a CardDb,
    /// Normalised names (English and translated) with the index of the card.
    names: Vec<(String, usize)>,
    prefer: HashSet<u32>,
    hint: String,
    cache: HashMap<String, Option<(usize, bool)>>,
}

impl<'a> Resolver<'a> {
    fn new(db: &'a CardDb) -> Self {
        let mut names = Vec::with_capacity(db.cards.len());
        for (i, c) in db.cards.iter().enumerate() {
            names.push((norm(&c.name), i));
            for n in c.names.values() {
                names.push((norm(n), i));
            }
        }
        Self {
            db,
            names,
            prefer: HashSet::new(),
            hint: String::new(),
            cache: HashMap::new(),
        }
    }

    fn set_context(&mut self, hint: &str, deck: Option<&Deck>) {
        self.hint = norm(hint);
        self.prefer = deck
            .map(|d| d.cards.iter().map(|c| c.id).collect())
            .unwrap_or_default();
        self.cache.clear();
    }

    /// Lower is better: deck card, then archetype of the hint, then the rest.
    fn rank(&self, card: &Card) -> u8 {
        if self.prefer.contains(&card.id) {
            return 0;
        }
        if !self.hint.is_empty() {
            let arch = card.archetype.as_deref().map(norm).unwrap_or_default();
            if arch == self.hint || norm(&card.name).contains(&self.hint) {
                return 1;
            }
        }
        2
    }

    /// Returns the card index and whether the name matched exactly.
    fn lookup(&mut self, name: &str) -> Option<(usize, bool)> {
        let q = norm(name);
        if q.is_empty() {
            return None;
        }
        if let Some(hit) = self.cache.get(&q) {
            return *hit;
        }
        let hit = self.lookup_uncached(&q);
        self.cache.insert(q, hit);
        hit
    }

    fn lookup_uncached(&self, q: &str) -> Option<(usize, bool)> {
        let cards = &self.db.cards;
        let best = |it: &mut dyn Iterator<Item = usize>| -> Option<usize> {
            it.min_by_key(|&i| (self.rank(&cards[i]), cards[i].name.len(), i))
        };
        // 1. Exact name. 2. Every word of the query starts a word of the name.
        let mut exact = self.names.iter().filter(|(n, _)| n == q).map(|(_, i)| *i);
        let exact = best(&mut exact);
        let words: Vec<&str> = q.split(' ').collect();
        let mut partial = self
            .names
            .iter()
            .filter(|(n, _)| {
                words
                    .iter()
                    .all(|w| n.split(' ').any(|nw| nw.starts_with(w)))
            })
            .map(|(_, i)| *i);
        let partial = best(&mut partial);
        // A card of the deck or archetype beats an unrelated card that
        // happens to have exactly the short name (e.g. "Wolf").
        match (exact, partial) {
            (Some(e), Some(p)) if self.rank(&cards[e]) == 2 && self.rank(&cards[p]) < 2 => {
                return Some((p, false));
            }
            (Some(e), _) => return Some((e, true)),
            (None, Some(p)) => return Some((p, false)),
            (None, None) => {}
        }
        // 3. Typos: smallest edit distance, also with the deck name in front.
        let with_hint = (!self.hint.is_empty()).then(|| format!("{} {q}", self.hint));
        let limit = (q.chars().count() / 4).max(1);
        let mut found: Option<(usize, u8, usize)> = None;
        for (n, i) in &self.names {
            let mut d = levenshtein(q, n);
            if let Some(h) = &with_hint {
                d = d.min(levenshtein(h, n));
            }
            if d > limit {
                continue;
            }
            let key = (d, self.rank(&cards[*i]), *i);
            if found.is_none_or(|f| key < f) {
                found = Some(key);
            }
        }
        found.map(|(_, _, i)| (i, false))
    }

    fn resolve(&mut self, name: &str) -> Found {
        let name = name.trim();
        if matches!(norm(name).as_str(), "any" | "any card" | "x") {
            return Found::Exact(Slot::Placeholder {
                kind: CardKind::Monster,
                label: "Any Card".into(),
            });
        }
        match self.lookup(name) {
            Some((i, true)) => Found::Exact(self.db.cards[i].to_slot()),
            Some((i, false)) => Found::Assumed(self.db.cards[i].to_slot()),
            None => Found::Missing,
        }
    }
}

/// One combo while it is being read.
struct Builder {
    combo: Combo,
    step: Step,
    board: Option<Board>,
    has_end: bool,
    touched: bool,
}

impl Builder {
    fn new(title: &str) -> Self {
        Self {
            combo: Combo::new(title),
            step: Step::default(),
            board: None,
            has_end: false,
            touched: false,
        }
    }

    fn end_step(&mut self) {
        if !self.step.parts.is_empty() {
            self.combo.steps.push(std::mem::take(&mut self.step));
        }
    }

    fn finish(mut self) -> Option<Combo> {
        self.end_step();
        if !self.touched {
            return None;
        }
        if !self.has_end
            && let Some(b) = &self.board
        {
            self.combo.endboard = b.field_entries();
        }
        Some(self.combo)
    }
}

/// Reads one or more combos from text.
pub fn parse(text: &str, db: &CardDb, decks: &[Deck]) -> Parsed {
    let mut out = Parsed::default();
    let mut res = Resolver::new(db);
    let mut cur = Builder::new(tr("textimport.default_title"));

    // Looks up a name and records how it was matched.
    fn slot_for(name: &str, res: &mut Resolver, out: &mut Parsed) -> Slot {
        match res.resolve(name) {
            Found::Exact(s) => s,
            Found::Assumed(s) => {
                let pair = (name.trim().to_string(), s.display_name().to_string());
                if !out.assumed.contains(&pair) {
                    out.assumed.push(pair);
                }
                s
            }
            Found::Missing => {
                let n = name.trim().to_string();
                if !out.not_found.contains(&n) {
                    out.not_found.push(n.clone());
                }
                Slot::Placeholder {
                    kind: CardKind::Monster,
                    label: n,
                }
            }
        }
    }
    fn list(value: &str, res: &mut Resolver, out: &mut Parsed) -> Vec<Entry> {
        value
            .split('+')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|n| Entry::new(slot_for(n, res, out)))
            .collect()
    }

    for (idx, raw) in text.lines().enumerate() {
        let line_no = idx + 1;
        let line = raw.trim();
        if line.starts_with("//") {
            continue;
        }
        if line.is_empty() || (line.len() >= 3 && line.chars().all(|c| c == '-')) {
            cur.end_step();
            continue;
        }
        if let Some(title) = line.strip_prefix('#') {
            let old = std::mem::replace(&mut cur, Builder::new(title.trim()));
            out.combos.extend(old.finish());
            cur.touched = true;
            res.set_context("", None);
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_lowercase();
            let value = value.trim();
            if HEADER_KEYS.contains(&key.as_str()) {
                cur.touched = true;
                match key.as_str() {
                    "title" => cur.combo.title = value.to_string(),
                    "author" => cur.combo.author = value.to_string(),
                    "deck" => {
                        let deck = decks
                            .iter()
                            .find(|d| d.name.trim().eq_ignore_ascii_case(value));
                        cur.combo.deck = value.to_string();
                        cur.combo.deck_id = deck.map(|d| d.id);
                        res.set_context(value, deck);
                    }
                    "hand" => {
                        // "A | B" = Karte A, ersetzbar durch B.
                        cur.combo.starting_hand = value
                            .split('+')
                            .filter(|s| !s.trim().is_empty())
                            .filter_map(|group| {
                                let mut names =
                                    group.split('|').map(str::trim).filter(|s| !s.is_empty());
                                let mut entry =
                                    Entry::new(slot_for(names.next()?, &mut res, &mut out));
                                for n in names {
                                    let alt = slot_for(n, &mut res, &mut out);
                                    if alt != entry.slot && !entry.alternatives.contains(&alt) {
                                        entry.alternatives.push(alt);
                                    }
                                }
                                Some(entry)
                            })
                            .collect();
                    }
                    "through" => cur.combo.plays_around = list(value, &mut res, &mut out),
                    "end" => {
                        cur.combo.endboard = list(value, &mut res, &mut out);
                        cur.has_end = true;
                    }
                    _ => {
                        // note: before the first move = note of the combo,
                        // later = note of the current step.
                        let target = if cur.step.parts.is_empty() && cur.combo.steps.is_empty() {
                            &mut cur.combo.notes
                        } else if cur.step.parts.is_empty() {
                            &mut cur.combo.steps.last_mut().expect("not empty").note
                        } else {
                            &mut cur.step.note
                        };
                        if !target.is_empty() {
                            target.push('\n');
                        }
                        target.push_str(value);
                    }
                }
                continue;
            }
        }

        for seg in line.split(" > ") {
            let (seg, note) = match seg.split_once("//") {
                Some((s, n)) => (s.trim(), n.trim()),
                None => (seg.trim(), ""),
            };
            if seg.is_empty() {
                continue;
            }
            // [zone] at the end.
            let (seg, zone) = match seg.rfind('[') {
                Some(p) if seg.ends_with(']') => {
                    let tag = seg[p + 1..seg.len() - 1].trim().to_lowercase();
                    match ZONE_TAGS.iter().find(|(t, _)| *t == tag) {
                        Some((_, z)) => (seg[..p].trim(), *z),
                        None => {
                            out.problems.push(trf(
                                "textimport.line",
                                &[&line_no, &trf("textimport.unknown_zone", &[&tag])],
                            ));
                            (seg[..p].trim(), Zone::None)
                        }
                    }
                }
                _ => (seg, Zone::None),
            };
            let (kw, rest) = seg.split_once(char::is_whitespace).unwrap_or((seg, ""));
            let kw = kw.to_lowercase();
            let mut rest = rest.trim();
            let action = if kw == "chain" {
                let (n, r) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
                rest = r.trim();
                n.parse::<u8>().ok().map(Action::Chain)
            } else if let Some(n) = kw.strip_prefix("cl").and_then(|n| n.parse::<u8>().ok()) {
                Some(Action::Chain(n))
            } else {
                KEYWORDS.iter().find(|(k, _)| *k == kw).map(|(_, a)| *a)
            };
            let Some(mut action) = action else {
                out.problems.push(trf(
                    "textimport.line",
                    &[&line_no, &trf("textimport.unknown_keyword", &[&kw])],
                ));
                continue;
            };
            if rest.is_empty() {
                out.problems.push(trf(
                    "textimport.line",
                    &[&line_no, &tr("textimport.no_card")],
                ));
                continue;
            }
            let slot = slot_for(rest, &mut res, &mut out);
            let (kind, extra) = match &slot {
                Slot::Card { id, .. } => db
                    .get(*id)
                    .map(|c| (c.kind, c.is_extra_deck()))
                    .unwrap_or_default(),
                Slot::Placeholder { kind, .. } => (*kind, false),
            };
            if action == Action::Set && kind != CardKind::Monster {
                action = Action::SetSpellTrap;
            }
            let zone = if action == Action::Search && zone == Zone::None {
                Zone::Hand
            } else {
                zone
            };
            let entry = Entry {
                action,
                zone,
                note: note.to_string(),
                ..Entry::new(slot)
            };
            // Play the move on a board to find moves that cannot work.
            let board = cur
                .board
                .get_or_insert_with(|| Board::with_hand(&cur.combo.starting_hand));
            let mv = board::part_to_move(board, &entry, kind, extra);
            if let Err(e) = board.apply(&mv, kind) {
                out.problems.push(trf(
                    "textimport.line",
                    &[
                        &line_no,
                        &format!("{} – {}", entry.slot.display_name(), tr(e.key())),
                    ],
                ));
            }
            cur.touched = true;
            cur.step.parts.push(StepPart {
                connector: Connector::Then,
                entry,
            });
        }
    }
    out.combos.extend(cur.finish());
    out
}

fn keyword(action: Action) -> String {
    match action {
        Action::Chain(n) => format!("cl{n}"),
        a => KEYWORDS
            .iter()
            .find(|(_, x)| *x == a)
            .map(|(k, _)| (*k).to_string())
            .unwrap_or_else(|| "card".into()),
    }
}

/// Writes a combo in the text format.
pub fn to_text(combo: &Combo) -> String {
    let names = |v: &[Entry]| {
        v.iter()
            .map(|e| e.slot.display_name().to_string())
            .collect::<Vec<_>>()
            .join(" + ")
    };
    let mut out = format!("# {}\n", combo.title.trim());
    if !combo.deck.trim().is_empty() {
        out.push_str(&format!("deck: {}\n", combo.deck.trim()));
    }
    if !combo.author.trim().is_empty() {
        out.push_str(&format!("author: {}\n", combo.author.trim()));
    }
    for l in combo.notes.lines().filter(|l| !l.trim().is_empty()) {
        out.push_str(&format!("note: {}\n", l.trim()));
    }
    if !combo.starting_hand.is_empty() {
        let hand = combo
            .starting_hand
            .iter()
            .map(|e| {
                std::iter::once(&e.slot)
                    .chain(&e.alternatives)
                    .map(|s| s.display_name().to_string())
                    .collect::<Vec<_>>()
                    .join(" | ")
            })
            .collect::<Vec<_>>()
            .join(" + ");
        out.push_str(&format!("hand: {hand}\n"));
    }
    if !combo.plays_around.is_empty() {
        out.push_str(&format!("through: {}\n", names(&combo.plays_around)));
    }
    for step in &combo.steps {
        out.push('\n');
        for part in &step.parts {
            let e = &part.entry;
            out.push_str(&format!("{} {}", keyword(e.action), e.slot.display_name()));
            let default_zone = e.action == Action::Search && e.zone == Zone::Hand;
            if !default_zone && let Some((tag, _)) = ZONE_TAGS.iter().find(|(_, z)| *z == e.zone) {
                out.push_str(&format!(" [{tag}]"));
            }
            if !e.note.trim().is_empty() {
                out.push_str(&format!(" // {}", e.note.trim().replace('\n', " ")));
            }
            out.push('\n');
        }
        for l in step.note.lines().filter(|l| !l.trim().is_empty()) {
            out.push_str(&format!("note: {}\n", l.trim()));
        }
    }
    if !combo.endboard.is_empty() {
        out.push_str(&format!("\nend: {}\n", names(&combo.endboard)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::CardDbFile;

    fn card(id: u32, name: &str, kind: CardKind, type_line: &str, arch: Option<&str>) -> Card {
        Card {
            id,
            image_id: id,
            name: name.into(),
            name_de: None,
            names: Default::default(),
            kind,
            type_line: type_line.into(),
            archetype: arch.map(Into::into),
            desc: String::new(),
            in_master_duel: true,
            alt_ids: Vec::new(),
            konami_id: None,
        }
    }

    fn db() -> CardDb {
        let m = CardKind::Monster;
        CardDb::from_file(CardDbFile {
            version: "t".into(),
            cards: vec![
                card(
                    1,
                    "Lunalight Gold Leo",
                    m,
                    "Effect Monster",
                    Some("Lunalight"),
                ),
                card(
                    2,
                    "Lunalight Yellow Marten",
                    m,
                    "Effect Monster",
                    Some("Lunalight"),
                ),
                card(
                    3,
                    "Lunalight Wolf",
                    m,
                    "Pendulum Effect Monster",
                    Some("Lunalight"),
                ),
                card(4, "Silver Fang", m, "Normal Monster", None),
                card(5, "Wolf", m, "Normal Monster", None),
                card(6, "S:P Little Knight", m, "Link Monster", None),
                card(7, "Foolish Burial", CardKind::Spell, "Spell Card", None),
                card(
                    8,
                    "Lunalight Liger Dancer",
                    m,
                    "Fusion Monster",
                    Some("Lunalight"),
                ),
            ],
        })
    }

    #[test]
    fn reads_a_combo_with_short_names() {
        let text = "# Test\ndeck: Lunalight\nhand: Gold Leo + any\n\nns Gold Leo\n\
                    eff Gold Leo > send Yellow Martin // typo\n\n\
                    act Foolish Burial\nscale wolf [pl]\nlink S:P Little Knight\ncl2 Gold Leo\n";
        let p = parse(text, &db(), &[]);
        assert!(p.not_found.is_empty(), "{:?}", p.not_found);
        assert!(p.problems.is_empty(), "{:?}", p.problems);
        assert_eq!(p.combos.len(), 1);
        let c = &p.combos[0];
        assert_eq!(c.title, "Test");
        assert_eq!(c.starting_hand.len(), 2);
        assert_eq!(c.starting_hand[0].slot.card_id(), Some(1));
        assert_eq!(c.steps.len(), 2);
        let s0 = &c.steps[0].parts;
        assert_eq!(s0[0].entry.action, Action::NormalSummon);
        assert_eq!(s0[2].entry.action, Action::SendToGy);
        assert_eq!(s0[2].entry.slot.card_id(), Some(2));
        assert_eq!(s0[2].entry.note, "typo");
        let s1 = &c.steps[1].parts;
        // "wolf" means the Lunalight card because of `deck: Lunalight`.
        assert_eq!(s1[1].entry.slot.card_id(), Some(3));
        assert_eq!(s1[1].entry.zone, Zone::PendulumLeft);
        assert_eq!(s1[2].entry.slot.card_id(), Some(6));
        assert_eq!(s1[3].entry.action, Action::Chain(2));
        // End board comes from the simulated field.
        assert!(c.endboard.iter().any(|e| e.slot.card_id() == Some(6)));
    }

    #[test]
    fn reports_unknown_cards_keywords_and_impossible_moves() {
        let text =
            "ns Blue-Eyes Something Else Entirely\nfoo Gold Leo\ndestroy Lunalight Gold Leo [nope]";
        let p = parse(text, &db(), &[]);
        assert_eq!(p.not_found.len(), 1);
        assert_eq!(p.problems.len(), 2);
        assert_eq!(p.combos.len(), 1);
    }

    #[test]
    fn several_combos_and_round_trip() {
        let text = "# A\nhand: Lunalight Gold Leo\n\nns Lunalight Gold Leo\n\n# B\nnote: hello\n\
                    act Foolish Burial > send Lunalight Wolf\nend: Lunalight Liger Dancer\n";
        let p = parse(text, &db(), &[]);
        assert_eq!(p.combos.len(), 2);
        assert_eq!(p.combos[1].notes, "hello");
        assert!(p.assumed.is_empty());
        for c in &p.combos {
            let again = parse(&to_text(c), &db(), &[]);
            assert_eq!(again.combos.len(), 1);
            assert_eq!(again.combos[0].steps, c.steps);
            assert_eq!(again.combos[0].starting_hand, c.starting_hand);
            let slots = |v: &[Entry]| v.iter().map(|e| e.slot.clone()).collect::<Vec<_>>();
            assert_eq!(slots(&again.combos[0].endboard), slots(&c.endboard));
        }
    }

    #[test]
    fn alternatives_in_the_hand_line() {
        let text = "# T\nhand: Lunalight Gold Leo + Foolish Burial | Lunalight Wolf | Foolish Burial\n\nact Foolish Burial\n";
        let p = parse(text, &db(), &[]);
        let hand = &p.combos[0].starting_hand;
        assert_eq!(hand.len(), 2);
        assert!(hand[0].alternatives.is_empty());
        assert_eq!(hand[1].slot.card_id(), Some(7));
        // The repeated name is not stored twice.
        assert_eq!(hand[1].alternatives.len(), 1);
        assert_eq!(hand[1].alternatives[0].card_id(), Some(3));
        let again = parse(&to_text(&p.combos[0]), &db(), &[]);
        assert_eq!(again.combos[0].starting_hand, *hand);
        // Practising with the alternative replaces the card in the steps too.
        let variant = p.combos[0].with_hand_choice(&[0, 1]);
        assert_eq!(variant.starting_hand[1].slot.card_id(), Some(3));
        assert_eq!(variant.steps[0].parts[0].entry.slot.card_id(), Some(3));
    }

    #[test]
    fn example_from_the_help_is_valid() {
        let p = parse(EXAMPLE, &db(), &[]);
        assert_eq!(p.combos.len(), 1);
        assert!(
            p.problems.is_empty(),
            "example has problems: {:?}",
            p.problems
        );
    }
}
