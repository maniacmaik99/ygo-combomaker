//! Übungsmodus: Der Nutzer sieht die Starthand und rät Schritt für Schritt,
//! welche Karten im nächsten Schritt benutzt werden. Danach wird der echte
//! Schritt aufgedeckt und verglichen. Die Reihenfolge der Karten innerhalb
//! eines Schritts spielt (noch) keine Rolle.

use uuid::Uuid;

use crate::model::{Combo, Slot, Step};

#[derive(Clone, Debug, PartialEq)]
pub struct StepCheck {
    /// Pro Teil des Schritts: wurde die Karte erraten?
    /// Platzhalter zählen immer als erraten.
    pub part_hit: Vec<bool>,
    /// Indizes der geratenen Karten, die in diesem Schritt nicht vorkommen.
    pub wrong_guesses: Vec<usize>,
}

impl StepCheck {
    pub fn is_correct(&self) -> bool {
        self.part_hit.iter().all(|h| *h) && self.wrong_guesses.is_empty()
    }
}

pub fn check_step(step: &Step, guesses: &[Slot]) -> StepCheck {
    let mut used = vec![false; guesses.len()];
    let part_hit = step
        .parts
        .iter()
        .map(|part| match part.entry.slot.card_id() {
            None => true,
            Some(id) => {
                let found = guesses
                    .iter()
                    .enumerate()
                    .position(|(i, g)| !used[i] && g.card_id() == Some(id));
                if let Some(i) = found {
                    used[i] = true;
                    true
                } else {
                    false
                }
            }
        })
        .collect();
    let wrong_guesses = used
        .iter()
        .enumerate()
        .filter(|(_, u)| !**u)
        .map(|(i, _)| i)
        .collect();
    StepCheck {
        part_hit,
        wrong_guesses,
    }
}

pub struct Session {
    pub combo_id: Uuid,
    /// Index des aktuellen Schritts.
    pub current: usize,
    pub guesses: Vec<Slot>,
    /// `Some`, sobald der aktuelle Schritt geprüft oder aufgedeckt wurde.
    pub checked: Option<StepCheck>,
    /// Ergebnis pro bereits abgeschlossenem Schritt.
    pub results: Vec<bool>,
    pub show_endboard: bool,
    pub stats_saved: bool,
}

impl Session {
    pub fn new(combo: &Combo) -> Self {
        Self {
            combo_id: combo.id,
            current: 0,
            guesses: Vec::new(),
            checked: None,
            results: Vec::new(),
            show_endboard: true,
            stats_saved: false,
        }
    }

    pub fn finished(&self, combo: &Combo) -> bool {
        self.current >= combo.steps.len()
    }

    /// Prüft die geratenen Karten gegen den aktuellen Schritt.
    pub fn check(&mut self, combo: &Combo) {
        if let Some(step) = combo.steps.get(self.current) {
            self.checked = Some(check_step(step, &self.guesses));
        }
    }

    /// Deckt den Schritt ohne Raten auf. Zählt als falsch.
    pub fn reveal(&mut self, combo: &Combo) {
        if let Some(step) = combo.steps.get(self.current) {
            self.checked = Some(StepCheck {
                part_hit: vec![false; step.parts.len()],
                wrong_guesses: (0..self.guesses.len()).collect(),
            });
        }
    }

    pub fn next(&mut self) {
        if let Some(check) = self.checked.take() {
            self.results.push(check.is_correct());
            self.current += 1;
            self.guesses.clear();
        }
    }

    pub fn correct_count(&self) -> usize {
        self.results.iter().filter(|r| **r).count()
    }

    pub fn percent(&self) -> u8 {
        if self.results.is_empty() {
            return 0;
        }
        ((self.correct_count() * 100) / self.results.len()) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CardKind, Connector, Entry, StepPart};

    fn card(id: u32) -> Slot {
        Slot::Card {
            id,
            image_id: id,
            name: format!("Karte {id}"),
        }
    }

    fn step(slots: Vec<Slot>) -> Step {
        Step {
            parts: slots
                .into_iter()
                .map(|s| StepPart {
                    connector: Connector::Then,
                    entry: Entry::new(s),
                })
                .collect(),
            note: String::new(),
        }
    }

    #[test]
    fn correct_in_any_order() {
        let s = step(vec![card(1), card(2)]);
        let r = check_step(&s, &[card(2), card(1)]);
        assert!(r.is_correct());
    }

    #[test]
    fn missing_and_wrong_cards() {
        let s = step(vec![card(1), card(2)]);
        let r = check_step(&s, &[card(1), card(3)]);
        assert_eq!(r.part_hit, vec![true, false]);
        assert_eq!(r.wrong_guesses, vec![1]);
        assert!(!r.is_correct());
    }

    #[test]
    fn duplicates_need_two_guesses() {
        let s = step(vec![card(1), card(1)]);
        assert!(!check_step(&s, &[card(1)]).is_correct());
        assert!(check_step(&s, &[card(1), card(1)]).is_correct());
    }

    #[test]
    fn placeholders_always_hit() {
        let s = step(vec![
            card(1),
            Slot::Placeholder {
                kind: CardKind::Monster,
                label: "3 other lunas".into(),
            },
        ]);
        assert!(check_step(&s, &[card(1)]).is_correct());
    }

    #[test]
    fn session_flow() {
        let mut combo = Combo::new("x");
        combo.steps = vec![step(vec![card(1)]), step(vec![card(2)])];
        let mut s = Session::new(&combo);
        s.guesses.push(card(1));
        s.check(&combo);
        s.next();
        s.reveal(&combo);
        s.next();
        assert!(s.finished(&combo));
        assert_eq!(s.percent(), 50);
    }
}
