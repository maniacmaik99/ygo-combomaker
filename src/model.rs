//! Datenmodell für Combos. Alles ist serialisierbar (JSON), damit Combos
//! gespeichert und später auch geteilt werden können.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Grobe Kartenart. Wird für Platzhalter und für die Farbe genutzt.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum CardKind {
    #[default]
    Monster,
    Spell,
    Trap,
}

impl CardKind {
    pub const ALL: [CardKind; 3] = [CardKind::Monster, CardKind::Spell, CardKind::Trap];

    pub fn label(self) -> &'static str {
        match self {
            CardKind::Monster => "Monster",
            CardKind::Spell => "Zauber",
            CardKind::Trap => "Falle",
        }
    }
}

/// Ein Platz in einer Combo: entweder eine echte Karte aus der Datenbank
/// oder ein Platzhalter wie "Any Card" oder "discard".
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub enum Slot {
    Card {
        /// Passcode der Karte.
        id: u32,
        /// ID des Bildes (meist identisch mit `id`).
        image_id: u32,
        /// Englischer Name, damit die Combo auch ohne Datenbank lesbar bleibt.
        name: String,
    },
    Placeholder {
        kind: CardKind,
        label: String,
    },
}

impl Slot {
    pub fn card_id(&self) -> Option<u32> {
        match self {
            Slot::Card { id, .. } => Some(*id),
            Slot::Placeholder { .. } => None,
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            Slot::Card { name, .. } => name,
            Slot::Placeholder { label, .. } => label,
        }
    }
}

/// Was mit der Karte in diesem Moment passiert. Wird als kleines Symbol
/// auf der Karte angezeigt.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Action {
    #[default]
    None,
    Activate,
    NormalSummon,
    SpecialSummon,
    Set,
    SendToGy,
    Banish,
    Chain(u8),
    Search,
    Discard,
    GyToHand,
}

impl Action {
    /// Auswahl im Editor. Ketten-Glieder 1 bis 6 reichen für die Praxis.
    pub const CHOICES: [Action; 16] = [
        Action::None,
        Action::Activate,
        Action::NormalSummon,
        Action::SpecialSummon,
        Action::Search,
        Action::Discard,
        Action::GyToHand,
        Action::Set,
        Action::SendToGy,
        Action::Banish,
        Action::Chain(1),
        Action::Chain(2),
        Action::Chain(3),
        Action::Chain(4),
        Action::Chain(5),
        Action::Chain(6),
    ];

    /// Kurzes Kürzel für das Symbol auf der Karte.
    pub fn badge(self) -> Option<String> {
        Some(match self {
            Action::None => return None,
            Action::Activate => "A".into(),
            Action::NormalSummon => "NS".into(),
            Action::SpecialSummon => "SS".into(),
            Action::Set => "Set".into(),
            Action::SendToGy => "GY".into(),
            Action::Banish => "Ban".into(),
            Action::Chain(n) => format!("K{n}"),
            Action::Search => "🔍".into(),
            Action::Discard => "Ab".into(),
            Action::GyToHand => "GY→Hand".into(),
        })
    }

    pub fn label(self) -> String {
        match self {
            Action::None => "Keine".into(),
            Action::Activate => "Effekt aktivieren".into(),
            Action::NormalSummon => "Normalbeschwörung".into(),
            Action::SpecialSummon => "Spezialbeschwörung".into(),
            Action::Set => "Setzen (verdeckt)".into(),
            Action::SendToGy => "Auf den Friedhof legen".into(),
            Action::Banish => "Verbannen".into(),
            Action::Chain(n) => format!("Kettenglied {n}"),
            Action::Search => "Suchen (auf die Hand nehmen)".into(),
            Action::Discard => "Abwerfen".into(),
            Action::GyToHand => "Vom Friedhof auf die Hand".into(),
        }
    }
}

/// Wohin die Karte gelegt wird.
#[allow(clippy::enum_variant_names)]
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Zone {
    #[default]
    None,
    MonsterZone,
    /// Extra-Monsterzone, in der Link- und Extra-Deck-Monster landen.
    ExtraMonsterZone,
    SpellTrapZone,
    PendulumLeft,
    PendulumRight,
    FieldZone,
}

impl Zone {
    pub const ALL: [Zone; 7] = [
        Zone::None,
        Zone::MonsterZone,
        Zone::ExtraMonsterZone,
        Zone::SpellTrapZone,
        Zone::PendulumLeft,
        Zone::PendulumRight,
        Zone::FieldZone,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Zone::None => "Keine Angabe",
            Zone::MonsterZone => "Monsterzone",
            Zone::ExtraMonsterZone => "Extra-Monsterzone (Link-Zone)",
            Zone::SpellTrapZone => "Zauber-/Fallenzone",
            Zone::PendulumLeft => "Pendelzone links",
            Zone::PendulumRight => "Pendelzone rechts",
            Zone::FieldZone => "Feldzauberzone",
        }
    }

    /// Kurzes Kürzel für die Anzeige oben rechts auf der Karte.
    pub fn badge(self) -> Option<&'static str> {
        Some(match self {
            Zone::None => return None,
            Zone::MonsterZone => "MZ",
            Zone::ExtraMonsterZone => "EMZ",
            Zone::SpellTrapZone => "Z/F",
            Zone::PendulumLeft => "P-L",
            Zone::PendulumRight => "P-R",
            Zone::FieldZone => "Feld",
        })
    }
}

/// Offen oder verdeckt.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Facing {
    #[default]
    Unspecified,
    FaceUp,
    FaceDown,
}

impl Facing {
    pub const ALL: [Facing; 3] = [Facing::Unspecified, Facing::FaceUp, Facing::FaceDown];

    pub fn label(self) -> &'static str {
        match self {
            Facing::Unspecified => "Keine Angabe",
            Facing::FaceUp => "Offen",
            Facing::FaceDown => "Verdeckt",
        }
    }
}

/// Eine Karte (oder ein Platzhalter) mit Aktion und kurzer Notiz.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Entry {
    pub slot: Slot,
    #[serde(default)]
    pub action: Action,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub zone: Zone,
    #[serde(default)]
    pub facing: Facing,
}

impl Entry {
    pub fn new(slot: Slot) -> Self {
        Self {
            slot,
            action: Action::None,
            note: String::new(),
            zone: Zone::None,
            facing: Facing::Unspecified,
        }
    }
}

/// Zeichen zwischen zwei Karten in einem Schritt.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Connector {
    #[default]
    Then,
    From,
    Plus,
    Equals,
}

impl Connector {
    pub const ALL: [Connector; 4] = [
        Connector::Then,
        Connector::From,
        Connector::Plus,
        Connector::Equals,
    ];

    pub fn symbol(self) -> &'static str {
        match self {
            Connector::Then => "→",
            Connector::From => "←",
            Connector::Plus => "+",
            Connector::Equals => "=",
        }
    }
}

/// Ein Teil eines Schritts. `connector` steht VOR der Karte; beim ersten
/// Teil eines Schritts wird er ignoriert.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct StepPart {
    #[serde(default)]
    pub connector: Connector,
    pub entry: Entry,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug, Default)]
pub struct Step {
    pub parts: Vec<StepPart>,
    #[serde(default)]
    pub note: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug, Default)]
pub struct PracticeStats {
    pub attempts: u32,
    /// Bester Anteil richtig gelöster Schritte in Prozent.
    pub best_percent: u8,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Combo {
    pub id: Uuid,
    pub title: String,
    #[serde(default)]
    pub deck: String,
    /// Verknüpfte Deckliste. Ihre Karten stehen in der Kartensuche oben.
    #[serde(default)]
    pub deck_id: Option<Uuid>,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub starting_hand: Vec<Entry>,
    #[serde(default)]
    pub endboard: Vec<Entry>,
    #[serde(default)]
    pub plays_around: Vec<Entry>,
    #[serde(default)]
    pub required: Vec<Entry>,
    #[serde(default)]
    pub steps: Vec<Step>,
    #[serde(default)]
    pub practice: PracticeStats,
}

impl Combo {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            title: title.into(),
            deck: String::new(),
            deck_id: None,
            author: String::new(),
            notes: String::new(),
            starting_hand: Vec::new(),
            endboard: Vec::new(),
            plays_around: Vec::new(),
            required: Vec::new(),
            steps: Vec::new(),
            practice: PracticeStats::default(),
        }
    }

    /// Sammelt alle echten Karten aus den Schritten, die nicht in der
    /// Starthand liegen. Jede Karte kommt nur einmal vor.
    pub fn cards_used_in_steps_not_in_hand(&self) -> Vec<Entry> {
        let hand: Vec<u32> = self
            .starting_hand
            .iter()
            .filter_map(|e| e.slot.card_id())
            .collect();
        let mut seen: Vec<u32> = Vec::new();
        let mut out = Vec::new();
        for part in self.steps.iter().flat_map(|s| &s.parts) {
            if let Slot::Card { id, .. } = &part.entry.slot
                && !hand.contains(id)
                && !seen.contains(id)
            {
                seen.push(*id);
                out.push(Entry::new(part.entry.slot.clone()));
            }
        }
        out
    }
}

/// Eine Karte in einer Deckliste.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct DeckCard {
    pub id: u32,
    pub image_id: u32,
    pub name: String,
    pub count: u8,
}

impl DeckCard {
    pub fn to_slot(&self) -> Slot {
        Slot::Card {
            id: self.id,
            image_id: self.image_id,
            name: self.name.clone(),
        }
    }
}

/// Deckliste. Wird beim Eintragen von Combos benutzt, damit die passenden
/// Karten mit einem Klick auswählbar sind.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Deck {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub cards: Vec<DeckCard>,
}

impl Deck {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            cards: Vec::new(),
        }
    }

    /// Fügt eine Karte hinzu oder erhöht ihre Anzahl (höchstens 3).
    pub fn add(&mut self, id: u32, image_id: u32, name: &str, count: u8) {
        if let Some(c) = self.cards.iter_mut().find(|c| c.id == id) {
            c.count = (c.count + count).min(3);
        } else {
            self.cards.push(DeckCard {
                id,
                image_id,
                name: name.to_string(),
                count: count.clamp(1, 3),
            });
        }
    }

    /// Verringert die Anzahl um 1 und entfernt die Karte bei 0.
    pub fn remove_one(&mut self, id: u32) {
        if let Some(i) = self.cards.iter().position(|c| c.id == id) {
            self.cards[i].count -= 1;
            if self.cards[i].count == 0 {
                self.cards.remove(i);
            }
        }
    }

    pub fn total(&self) -> u32 {
        self.cards.iter().map(|c| c.count as u32).sum()
    }
}

/// Die Bereiche einer Combo, die aus einer einfachen Kartenliste bestehen.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Section {
    StartingHand,
    Endboard,
    PlaysAround,
    Required,
}

impl Section {
    pub const ALL: [Section; 4] = [
        Section::StartingHand,
        Section::Endboard,
        Section::PlaysAround,
        Section::Required,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Section::StartingHand => "Starthand",
            Section::Endboard => "Endboard",
            Section::PlaysAround => "Spielt um (Handtraps)",
            Section::Required => "Benötigte Karten",
        }
    }

    pub fn entries(self, combo: &Combo) -> &Vec<Entry> {
        match self {
            Section::StartingHand => &combo.starting_hand,
            Section::Endboard => &combo.endboard,
            Section::PlaysAround => &combo.plays_around,
            Section::Required => &combo.required,
        }
    }

    pub fn entries_mut(self, combo: &mut Combo) -> &mut Vec<Entry> {
        match self {
            Section::StartingHand => &mut combo.starting_hand,
            Section::Endboard => &mut combo.endboard,
            Section::PlaysAround => &mut combo.plays_around,
            Section::Required => &mut combo.required,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(id: u32, name: &str) -> Slot {
        Slot::Card {
            id,
            image_id: id,
            name: name.into(),
        }
    }

    #[test]
    fn combo_roundtrip_json() {
        let mut c = Combo::new("Gold Leo + Any Card");
        c.starting_hand
            .push(Entry::new(card(1, "Lunalight Gold Leo")));
        c.starting_hand.push(Entry::new(Slot::Placeholder {
            kind: CardKind::Monster,
            label: "Any Card".into(),
        }));
        c.steps.push(Step {
            parts: vec![
                StepPart {
                    connector: Connector::Then,
                    entry: Entry {
                        slot: card(1, "Lunalight Gold Leo"),
                        action: Action::NormalSummon,
                        note: "NS".into(),
                        zone: Zone::MonsterZone,
                        facing: Facing::FaceUp,
                    },
                },
                StepPart {
                    connector: Connector::Then,
                    entry: Entry::new(card(2, "Lunalight Silver Hound")),
                },
            ],
            note: String::new(),
        });
        let json = serde_json::to_string(&c).unwrap();
        let back: Combo = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn deck_add_and_remove() {
        let mut d = Deck::new("Lunalight");
        d.add(1, 1, "A", 2);
        d.add(1, 1, "A", 2);
        assert_eq!(d.cards[0].count, 3);
        d.add(2, 2, "B", 1);
        assert_eq!(d.total(), 4);
        d.remove_one(2);
        assert_eq!(d.cards.len(), 1);
    }

    #[test]
    fn required_cards_skip_hand_and_duplicates() {
        let mut c = Combo::new("x");
        c.starting_hand.push(Entry::new(card(1, "A")));
        for id in [1, 2, 2, 3] {
            c.steps.push(Step {
                parts: vec![StepPart {
                    connector: Connector::Then,
                    entry: Entry::new(card(id, "?")),
                }],
                note: String::new(),
            });
        }
        let ids: Vec<u32> = c
            .cards_used_in_steps_not_in_hand()
            .iter()
            .filter_map(|e| e.slot.card_id())
            .collect();
        assert_eq!(ids, vec![2, 3]);
    }
}
