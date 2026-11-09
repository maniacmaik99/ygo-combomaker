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
}

impl Action {
    /// Auswahl im Editor. Ketten-Glieder 1 bis 6 reichen für die Praxis.
    pub const CHOICES: [Action; 13] = [
        Action::None,
        Action::Activate,
        Action::NormalSummon,
        Action::SpecialSummon,
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
        })
    }

    pub fn label(self) -> String {
        match self {
            Action::None => "Keine".into(),
            Action::Activate => "Effekt aktivieren".into(),
            Action::NormalSummon => "Normalbeschwörung".into(),
            Action::SpecialSummon => "Spezialbeschwörung".into(),
            Action::Set => "Setzen".into(),
            Action::SendToGy => "Auf den Friedhof legen".into(),
            Action::Banish => "Verbannen".into(),
            Action::Chain(n) => format!("Kettenglied {n}"),
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
}

impl Entry {
    pub fn new(slot: Slot) -> Self {
        Self {
            slot,
            action: Action::None,
            note: String::new(),
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
