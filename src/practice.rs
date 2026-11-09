//! Üben auf dem Spielfeld: Der Nutzer spielt die Combo Zug für Zug nach.
//! Jeder Zug wird mit dem nächsten gespeicherten Teil der Combo verglichen.
//!
//! Ein Zug ist richtig, wenn
//! - die Karte stimmt (ein Platzhalter in der Combo passt auf jede Karte) und
//! - die Aktion stimmt (ist in der Combo keine Aktion eingetragen, passt jede).
//!
//! Die Zone wird nicht geprüft: Ob ein Monster links oder rechts liegt, ist
//! für das Lernen der Combo meist egal.

use uuid::Uuid;

use crate::board::{self, Board, Move, MoveError};
use crate::model::{Action, CardKind, Combo, Entry, Slot};

#[derive(Clone, Debug, PartialEq)]
pub enum Verdict {
    Correct,
    Wrong,
}

pub fn move_matches(expected: &Entry, mv: &Move) -> bool {
    let card_ok = match (&expected.slot, &mv.slot) {
        (Slot::Placeholder { .. }, _) => true,
        (Slot::Card { id: a, .. }, Slot::Card { id: b, .. }) => a == b,
        _ => false,
    };
    let action_ok = expected.action == Action::None || expected.action == mv.action;
    card_ok && action_ok
}

pub struct Session {
    pub combo_id: Uuid,
    pub board: Board,
    /// (Schritt, Teil) aller Teile der Combo in Reihenfolge.
    flat: Vec<(usize, usize)>,
    /// Index in `flat` des nächsten erwarteten Teils.
    pos: usize,
    /// Pro erledigtem Teil: beim ersten Versuch richtig?
    pub results: Vec<bool>,
    /// Beim aktuellen Teil gab es schon einen Fehler.
    current_failed: bool,
    pub mistakes: u32,
    pub show_endboard: bool,
    pub stats_saved: bool,
}

impl Session {
    pub fn new(combo: &Combo) -> Self {
        Self {
            combo_id: combo.id,
            board: Board::with_hand(&combo.starting_hand),
            flat: board::flatten(&combo.steps),
            pos: 0,
            results: Vec::new(),
            current_failed: false,
            mistakes: 0,
            show_endboard: true,
            stats_saved: false,
        }
    }

    pub fn total(&self) -> usize {
        self.flat.len()
    }

    pub fn done(&self) -> usize {
        self.pos
    }

    pub fn finished(&self) -> bool {
        self.pos >= self.flat.len()
    }

    /// (Schritt, Teil) des nächsten erwarteten Zugs.
    pub fn current(&self) -> Option<(usize, usize)> {
        self.flat.get(self.pos).copied()
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

    fn expected<'a>(&self, combo: &'a Combo) -> Option<&'a Entry> {
        let (si, pi) = self.current()?;
        Some(&combo.steps.get(si)?.parts.get(pi)?.entry)
    }

    fn advance(&mut self, correct_first_try: bool) {
        self.results.push(correct_first_try);
        self.current_failed = false;
        self.pos += 1;
    }

    /// Prüft einen Zug. Ist er richtig, wird er auf dem Feld ausgeführt.
    /// Ein Fehler beim Ausführen (z. B. Zone belegt) zählt nicht als falsch.
    pub fn submit(
        &mut self,
        combo: &Combo,
        mv: &Move,
        kind: CardKind,
    ) -> Result<Verdict, MoveError> {
        let Some(expected) = self.expected(combo) else {
            return Ok(Verdict::Correct);
        };
        if !move_matches(expected, mv) {
            self.current_failed = true;
            self.mistakes += 1;
            return Ok(Verdict::Wrong);
        }
        self.board.apply(mv, kind)?;
        let ok = !self.current_failed;
        self.advance(ok);
        Ok(Verdict::Correct)
    }

    /// Zeigt den erwarteten Zug, führt ihn aus und zählt ihn als falsch.
    /// `card_info` liefert zu einer Karte die Kartenart und ob sie ins
    /// Extra Deck gehört.
    pub fn reveal(
        &mut self,
        combo: &Combo,
        card_info: impl Fn(&Slot) -> (CardKind, bool),
    ) -> Option<Move> {
        let expected = self.expected(combo)?.clone();
        let (kind, extra) = card_info(&expected.slot);
        let mv = board::part_to_move(&self.board, &expected, kind, extra);
        // Klappt der Zug nicht (z. B. Zone fehlt), geht es trotzdem weiter.
        let _ = self.board.apply(&mv, kind);
        self.advance(false);
        Some(mv)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::Area;
    use crate::model::{CardKind, Connector, Step, StepPart};

    fn card(id: u32) -> Slot {
        Slot::Card {
            id,
            image_id: id,
            name: format!("C{id}"),
        }
    }

    fn combo() -> Combo {
        let mut c = Combo::new("x");
        c.starting_hand = vec![Entry::new(card(1))];
        c.steps = vec![Step {
            parts: vec![
                StepPart {
                    connector: Connector::Then,
                    entry: Entry {
                        action: Action::NormalSummon,
                        ..Entry::new(card(1))
                    },
                },
                StepPart {
                    connector: Connector::Then,
                    entry: Entry {
                        action: Action::Search,
                        ..Entry::new(card(2))
                    },
                },
            ],
            note: String::new(),
        }];
        c
    }

    #[test]
    fn correct_and_wrong_moves() {
        let c = combo();
        let mut s = Session::new(&c);
        let wrong = Move {
            slot: card(1),
            from: Area::Hand,
            action: Action::SpecialSummon,
            to: Some(Area::Monster(2)),
        };
        assert_eq!(s.submit(&c, &wrong, CardKind::Monster), Ok(Verdict::Wrong));
        let right = Move {
            action: Action::NormalSummon,
            ..wrong
        };
        assert_eq!(
            s.submit(&c, &right, CardKind::Monster),
            Ok(Verdict::Correct)
        );
        assert_eq!(s.results, vec![false]);
        let search = Move {
            slot: card(2),
            from: Area::Deck,
            action: Action::Search,
            to: Some(Area::Hand),
        };
        assert_eq!(
            s.submit(&c, &search, CardKind::Monster),
            Ok(Verdict::Correct)
        );
        assert!(s.finished());
        assert_eq!(s.percent(), 50);
    }

    #[test]
    fn reveal_applies_expected_move() {
        let c = combo();
        let mut s = Session::new(&c);
        let mv = s.reveal(&c, |_| (CardKind::Monster, false)).unwrap();
        assert_eq!(mv.action, Action::NormalSummon);
        assert!(matches!(s.board.find(&card(1)), Some(Area::Monster(_))));
        s.reveal(&c, |_| (CardKind::Monster, false));
        assert_eq!(s.board.find(&card(2)), Some(Area::Hand));
        assert!(s.finished());
        assert_eq!(s.percent(), 0);
    }

    #[test]
    fn placeholder_and_missing_action_match_anything() {
        let e = Entry::new(Slot::Placeholder {
            kind: CardKind::Monster,
            label: "Any".into(),
        });
        let mv = Move {
            slot: card(9),
            from: Area::Hand,
            action: Action::Discard,
            to: None,
        };
        assert!(move_matches(&e, &mv));
    }
}
