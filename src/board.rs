//! Spielfeld-Logik (ohne Oberfläche): Zonen, Züge und wohin eine Karte
//! bei welcher Aktion wandert. Wird zum Aufnehmen und zum Üben benutzt.
//!
//! Aufbau wie in Master Duel / Master Rule 5: 5 Monsterzonen,
//! 2 Extra-Monsterzonen, 5 Zauber-/Fallenzonen, Feldzauberzone.
//! Die Pendelzonen werden der Übersicht halber als eigene Felder gezeigt.

use crate::model::{Action, CardKind, Entry, Facing, Slot, Step, StepPart, Zone};

/// Ein Ort, an dem eine Karte sein kann.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Area {
    Hand,
    Deck,
    ExtraDeck,
    Gy,
    Banished,
    /// Xyz-Material (vereinfacht als eigener Stapel).
    Overlay,
    Monster(u8),
    Emz(u8),
    SpellTrap(u8),
    Field,
}

impl Area {
    pub fn is_field_zone(self) -> bool {
        matches!(
            self,
            Area::Monster(_) | Area::Emz(_) | Area::SpellTrap(_) | Area::Field
        )
    }

    pub fn zone(self) -> Zone {
        match self {
            Area::Monster(_) => Zone::MonsterZone,
            Area::Emz(_) => Zone::ExtraMonsterZone,
            Area::SpellTrap(_) => Zone::SpellTrapZone,
            Area::Field => Zone::FieldZone,
            _ => Zone::None,
        }
    }
}

/// Wohin eine Karte durch eine Aktion kommt.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dest {
    /// Karte bleibt, wo sie ist.
    Stay,
    Hand,
    Gy,
    Banished,
    Deck,
    ExtraDeck,
    Overlay,
    /// Der Nutzer wählt eine Monsterzone oder Extra-Monsterzone.
    ChooseMonster,
    /// Der Nutzer wählt eine Zauber-/Fallenzone.
    ChooseSpellTrap,
    /// Zauber-/Fallenzone oder Feldzauberzone.
    ChooseSpellTrapOrField,
    ChoosePendulum,
    /// Hand oder Friedhof (z. B. Suchen).
    ChooseHandOrGy,
}

impl Dest {
    pub fn needs_choice(self) -> bool {
        matches!(
            self,
            Dest::ChooseMonster
                | Dest::ChooseSpellTrap
                | Dest::ChooseSpellTrapOrField
                | Dest::ChoosePendulum
                | Dest::ChooseHandOrGy
        )
    }

    /// Passt die gewählte Zone zu dieser Zielart?
    pub fn accepts(self, area: Area) -> bool {
        match self {
            Dest::ChooseMonster => matches!(area, Area::Monster(_) | Area::Emz(_)),
            Dest::ChooseSpellTrap => matches!(area, Area::SpellTrap(_)),
            Dest::ChooseSpellTrapOrField => matches!(area, Area::SpellTrap(_) | Area::Field),
            // Die Pendelzonen sind die äußerste linke und rechte Zauber-/Fallenzone.
            Dest::ChoosePendulum => matches!(area, Area::SpellTrap(0) | Area::SpellTrap(4)),
            Dest::ChooseHandOrGy => matches!(area, Area::Hand | Area::Gy),
            _ => false,
        }
    }
}

/// Bestimmt das Ziel einer Aktion. `kind` ist die Kartenart, `from` der
/// aktuelle Ort.
pub fn dest_for(action: Action, kind: CardKind, from: Area) -> Dest {
    use Action::*;
    match action {
        None | FlipSummon | Reveal | Negate | Target | ChangePosition => Dest::Stay,
        Activate | Chain(_) => {
            if matches!(from, Area::Hand | Area::Deck) && kind != CardKind::Monster {
                Dest::ChooseSpellTrapOrField
            } else {
                Dest::Stay
            }
        }
        PlacePendulum => Dest::ChoosePendulum,
        SetSpellTrap => Dest::ChooseSpellTrap,
        PlaceOnField => Dest::ChooseSpellTrapOrField,
        Equip => Dest::ChooseSpellTrap,
        // „Setzen“ einer Zauber-/Fallenkarte (ältere Combos nutzen dafür `Set`).
        Set if kind != CardKind::Monster => Dest::ChooseSpellTrap,
        NormalSummon | TributeSummon | Set | SpecialSummon | FusionSummon | RitualSummon
        | SynchroSummon | XyzSummon | PendulumSummon | LinkSummon => Dest::ChooseMonster,
        Tribute | UseAsMaterial | DetachMaterial | Discard | SendToGy | Destroy | BanishedToGy => {
            Dest::Gy
        }
        AttachMaterial => Dest::Overlay,
        Search => Dest::ChooseHandOrGy,
        Draw | ReturnToHand | GyToHand | BanishedToHand => Dest::Hand,
        Banish => Dest::Banished,
        GyToDeck | ReturnToDeck => Dest::Deck,
        ReturnToExtraDeck => Dest::ExtraDeck,
    }
}

/// Eigenschaften einer Karte, die bestimmen, welche Aktionen sinnvoll sind.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CardTraits {
    pub kind: CardKind,
    /// Passende Beschwörung aus dem Extra Deck (Fusion, Synchro, Xyz, Link).
    pub extra_summon: Option<Action>,
    pub pendulum: bool,
    pub ritual: bool,
}

/// Festes Ziel einer Aktion (ohne Auswahl), falls es eins gibt.
pub fn fixed_area(dest: Dest) -> Option<Area> {
    match dest {
        Dest::Hand => Some(Area::Hand),
        Dest::Gy => Some(Area::Gy),
        Dest::Banished => Some(Area::Banished),
        Dest::Deck => Some(Area::Deck),
        Dest::ExtraDeck => Some(Area::ExtraDeck),
        Dest::Overlay => Some(Area::Overlay),
        _ => None,
    }
}

/// Kann die Aktion die Karte nach `to` bringen?
pub fn action_reaches(action: Action, kind: CardKind, from: Area, to: Area) -> bool {
    let dest = dest_for(action, kind, from);
    if dest.needs_choice() {
        dest.accepts(to)
    } else {
        fixed_area(dest) == Some(to)
    }
}

/// Die Aktionen, die für diese Karte an diesem Ort üblich sind – wie das
/// Menü, das in Master Duel beim Anklicken einer Karte erscheint.
/// Die wichtigste Aktion steht zuerst.
pub fn context_actions(t: &CardTraits, from: Area, face_down: bool) -> Vec<Action> {
    use Action::*;
    let monster = t.kind == CardKind::Monster;
    let mut v: Vec<Action> = match from {
        Area::Hand if monster => {
            let mut v = vec![NormalSummon, SpecialSummon, Set, Activate];
            if t.pendulum {
                v.push(PlacePendulum);
                v.push(PendulumSummon);
            }
            if t.ritual {
                v.push(RitualSummon);
            }
            v.extend([
                Discard,
                UseAsMaterial,
                TributeSummon,
                SendToGy,
                Banish,
                Reveal,
                ReturnToDeck,
            ]);
            v
        }
        Area::Hand if t.kind == CardKind::Spell => {
            vec![
                Activate,
                SetSpellTrap,
                Discard,
                SendToGy,
                Banish,
                Reveal,
                ReturnToDeck,
            ]
        }
        Area::Hand => vec![
            SetSpellTrap,
            Activate,
            Discard,
            SendToGy,
            Banish,
            Reveal,
            ReturnToDeck,
        ],
        Area::Deck if monster => {
            let mut v = vec![Search, SendToGy, SpecialSummon, Draw, Banish];
            if t.pendulum {
                v.push(PlacePendulum);
                v.push(ReturnToExtraDeck);
            }
            v
        }
        Area::Deck => vec![Search, SendToGy, Activate, SetSpellTrap, Draw, Banish],
        Area::ExtraDeck => {
            let mut v = Vec::new();
            v.extend(t.extra_summon);
            if t.pendulum {
                v.push(PendulumSummon);
            }
            v.extend([SpecialSummon, SendToGy, Banish, Reveal]);
            v
        }
        Area::Gy if monster => vec![
            SpecialSummon,
            GyToHand,
            Activate,
            Banish,
            GyToDeck,
            AttachMaterial,
            Target,
        ],
        Area::Gy => vec![Activate, GyToHand, Banish, GyToDeck, SetSpellTrap, Target],
        Area::Banished if monster => vec![
            SpecialSummon,
            BanishedToHand,
            BanishedToGy,
            Activate,
            ReturnToDeck,
        ],
        Area::Banished => vec![BanishedToHand, BanishedToGy, Activate, ReturnToDeck],
        Area::Overlay => vec![DetachMaterial],
        Area::Monster(_) | Area::Emz(_) => {
            let mut v = Vec::new();
            if face_down {
                v.push(FlipSummon);
            }
            v.extend([
                Activate,
                UseAsMaterial,
                SendToGy,
                Tribute,
                ReturnToHand,
                Banish,
                Destroy,
                AttachMaterial,
            ]);
            if t.extra_summon.is_some() || t.pendulum {
                v.push(ReturnToExtraDeck);
            }
            v.extend([ReturnToDeck, ChangePosition, Negate, Target]);
            v
        }
        Area::SpellTrap(_) | Area::Field => {
            let mut v = vec![Activate];
            if monster {
                // Pendelmonster in der Pendelzone.
                v.extend([SpecialSummon, ReturnToExtraDeck]);
            }
            v.extend([
                SendToGy,
                Destroy,
                ReturnToHand,
                Banish,
                ReturnToDeck,
                Negate,
                Target,
            ]);
            v
        }
    };
    // „Kettenglied“ überall dort, wo man einen Effekt aktivieren kann.
    if let Some(i) = v.iter().position(|a| *a == Activate) {
        v.insert(i + 1, Chain(1));
    }
    v
}

/// Wird die Karte bei dieser Aktion verdeckt hingelegt?
pub fn placed_face_down(action: Action) -> bool {
    matches!(action, Action::Set | Action::SetSpellTrap)
}

#[derive(Clone, Debug, PartialEq)]
pub struct FieldCard {
    pub slot: Slot,
    pub face_down: bool,
}

/// Ein Zug: Karte, Ort vorher, Aktion, Ort nachher.
#[derive(Clone, Debug, PartialEq)]
pub struct Move {
    pub slot: Slot,
    pub from: Area,
    pub action: Action,
    /// Gewählte Zielzone. `None` = Ziel ergibt sich aus der Aktion.
    pub to: Option<Area>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Board {
    pub hand: Vec<FieldCard>,
    pub gy: Vec<FieldCard>,
    pub banished: Vec<FieldCard>,
    pub overlay: Vec<FieldCard>,
    pub extra_face_up: Vec<FieldCard>,
    pub monsters: [Option<FieldCard>; 5],
    pub emz: [Option<FieldCard>; 2],
    pub spell_trap: [Option<FieldCard>; 5],
    pub field: Option<FieldCard>,
}

#[derive(Debug, PartialEq)]
pub enum MoveError {
    CardNotFound,
    ZoneOccupied,
    ZoneMissing,
    WrongZone,
}

impl MoveError {
    pub fn key(&self) -> &'static str {
        match self {
            MoveError::CardNotFound => "board.err.card_not_found",
            MoveError::ZoneOccupied => "board.err.zone_occupied",
            MoveError::ZoneMissing => "board.err.zone_missing",
            MoveError::WrongZone => "board.err.wrong_zone",
        }
    }
}

impl Board {
    pub fn with_hand(hand: &[Entry]) -> Self {
        Board {
            hand: hand
                .iter()
                .map(|e| FieldCard {
                    slot: e.slot.clone(),
                    face_down: false,
                })
                .collect(),
            ..Default::default()
        }
    }

    fn zone_mut(&mut self, area: Area) -> Option<&mut Option<FieldCard>> {
        match area {
            Area::Monster(i) => self.monsters.get_mut(i as usize),
            Area::Emz(i) => self.emz.get_mut(i as usize),
            Area::SpellTrap(i) => self.spell_trap.get_mut(i as usize),
            Area::Field => Some(&mut self.field),
            _ => None,
        }
    }

    pub fn zone(&self, area: Area) -> Option<&FieldCard> {
        match area {
            Area::Monster(i) => self.monsters.get(i as usize)?.as_ref(),
            Area::Emz(i) => self.emz.get(i as usize)?.as_ref(),
            Area::SpellTrap(i) => self.spell_trap.get(i as usize)?.as_ref(),
            Area::Field => self.field.as_ref(),
            _ => None,
        }
    }

    fn pile_mut(&mut self, area: Area) -> Option<&mut Vec<FieldCard>> {
        match area {
            Area::Hand => Some(&mut self.hand),
            Area::Gy => Some(&mut self.gy),
            Area::Banished => Some(&mut self.banished),
            Area::Overlay => Some(&mut self.overlay),
            Area::ExtraDeck => Some(&mut self.extra_face_up),
            _ => None,
        }
    }

    /// Alle Zonen auf dem Feld in fester Reihenfolge.
    pub fn field_areas() -> Vec<Area> {
        let mut v = vec![Area::Field];
        v.extend((0..2).map(Area::Emz));
        v.extend((0..5).map(Area::Monster));
        v.extend((0..5).map(Area::SpellTrap));
        v
    }

    /// Erste freie Zone, die zur Zielart passt. Bevorzugt `prefer`.
    pub fn first_free(&self, dest: Dest, prefer: Zone) -> Option<Area> {
        let mut candidates: Vec<Area> = Self::field_areas()
            .into_iter()
            .filter(|a| dest.accepts(*a) && self.zone(*a).is_none())
            .collect();
        // Bevorzugte Zone zuerst, Monsterzonen von der Mitte aus.
        candidates.sort_by_key(|a| {
            let preferred = a.zone() == prefer
                || (prefer == Zone::PendulumLeft && *a == Area::SpellTrap(0))
                || (prefer == Zone::PendulumRight && *a == Area::SpellTrap(4));
            let center = match a {
                Area::Monster(i) | Area::SpellTrap(i) => (*i as i32 - 2).abs(),
                _ => 0,
            };
            (!preferred, matches!(a, Area::Emz(_)), center)
        });
        candidates.into_iter().next()
    }

    /// Wo liegt diese Karte? Sucht Hand, Feld, Friedhof, Verbannt, Material.
    pub fn find(&self, slot: &Slot) -> Option<Area> {
        let same = |c: &FieldCard| match (slot, &c.slot) {
            (Slot::Card { id: a, .. }, Slot::Card { id: b, .. }) => a == b,
            (a, b) => a == b,
        };
        if self.hand.iter().any(same) {
            return Some(Area::Hand);
        }
        for a in Self::field_areas() {
            if self.zone(a).is_some_and(same) {
                return Some(a);
            }
        }
        for (area, pile) in [
            (Area::Gy, &self.gy),
            (Area::Banished, &self.banished),
            (Area::Overlay, &self.overlay),
            (Area::ExtraDeck, &self.extra_face_up),
        ] {
            if pile.iter().any(same) {
                return Some(area);
            }
        }
        None
    }

    /// Nimmt die Karte vom Ort `from`. Aus Deck/Extra Deck wird sie
    /// „neu“ geholt (die Decks werden nicht vollständig nachgebildet).
    fn take(&mut self, slot: &Slot, from: Area) -> Result<FieldCard, MoveError> {
        if matches!(from, Area::Deck) {
            return Ok(FieldCard {
                slot: slot.clone(),
                face_down: false,
            });
        }
        if let Some(pile) = self.pile_mut(from) {
            let pos = pile.iter().position(|c| {
                c.slot.card_id() == slot.card_id() && (slot.card_id().is_some() || c.slot == *slot)
            });
            return match pos {
                Some(i) => Ok(pile.remove(i)),
                // Extra Deck: Karte, die nicht offen dort liegt, kommt verdeckt aus dem Extra Deck.
                None if from == Area::ExtraDeck => Ok(FieldCard {
                    slot: slot.clone(),
                    face_down: false,
                }),
                None => Err(MoveError::CardNotFound),
            };
        }
        let zone = self.zone_mut(from).ok_or(MoveError::CardNotFound)?;
        match zone.take() {
            Some(c) => Ok(c),
            None => Err(MoveError::CardNotFound),
        }
    }

    /// Führt einen Zug aus. Bei einem Fehler bleibt das Feld unverändert.
    pub fn apply(&mut self, mv: &Move, kind: CardKind) -> Result<(), MoveError> {
        let dest = dest_for(mv.action, kind, mv.from);
        let target: Option<Area> = match dest {
            Dest::Stay => None,
            Dest::Hand => Some(Area::Hand),
            Dest::Gy => Some(Area::Gy),
            Dest::Banished => Some(Area::Banished),
            Dest::Overlay => Some(Area::Overlay),
            Dest::ExtraDeck => Some(Area::ExtraDeck),
            Dest::Deck => Some(Area::Deck),
            d => {
                let to = mv.to.ok_or(MoveError::ZoneMissing)?;
                if !d.accepts(to) {
                    return Err(MoveError::WrongZone);
                }
                Some(to)
            }
        };
        let Some(to) = target else {
            // Karte bleibt liegen. Bei Flippen/Aufdecken wird sie offen.
            if mv.from.is_field_zone()
                && matches!(
                    mv.action,
                    Action::FlipSummon | Action::Activate | Action::Chain(_)
                )
                && let Some(Some(c)) = self.zone_mut(mv.from)
            {
                c.face_down = false;
            }
            if mv.from == Area::Deck {
                // Effekt aus dem Deck (selten): nichts zu bewegen.
            } else if self.find(&mv.slot).is_none() {
                return Err(MoveError::CardNotFound);
            }
            return Ok(());
        };
        if to.is_field_zone() && self.zone(to).is_some() && to != mv.from {
            return Err(MoveError::ZoneOccupied);
        }
        let mut card = self.take(&mv.slot, mv.from)?;
        card.face_down = to.is_field_zone() && placed_face_down(mv.action);
        if to == Area::Deck {
            return Ok(());
        }
        if let Some(pile) = self.pile_mut(to) {
            pile.push(card);
        } else if let Some(zone) = self.zone_mut(to) {
            *zone = Some(card);
        }
        Ok(())
    }

    /// Alle Karten auf dem Feld als Einträge (für „Endboard übernehmen“).
    pub fn field_entries(&self) -> Vec<Entry> {
        Self::field_areas()
            .into_iter()
            .filter_map(|a| {
                self.zone(a).map(|c| Entry {
                    zone: a.zone(),
                    facing: if c.face_down {
                        Facing::FaceDown
                    } else {
                        Facing::FaceUp
                    },
                    ..Entry::new(c.slot.clone())
                })
            })
            .collect()
    }
}

/// Wandelt einen Zug in einen Teil eines Schritts um (für die Aufnahme).
pub fn move_to_part(mv: &Move, kind: CardKind) -> StepPart {
    let dest_area = mv
        .to
        .or(if dest_for(mv.action, kind, mv.from) == Dest::Stay {
            Some(mv.from)
        } else {
            None
        });
    let zone = if dest_for(mv.action, kind, mv.from) == Dest::ChooseHandOrGy {
        if mv.to == Some(Area::Gy) {
            Zone::Graveyard
        } else {
            Zone::Hand
        }
    } else if mv.action == Action::PlacePendulum {
        if mv.to == Some(Area::SpellTrap(4)) {
            Zone::PendulumRight
        } else {
            Zone::PendulumLeft
        }
    } else {
        dest_area.map(Area::zone).unwrap_or(Zone::None)
    };
    let facing = if matches!(zone, Zone::None | Zone::Hand | Zone::Graveyard) {
        Facing::Unspecified
    } else if placed_face_down(mv.action) {
        Facing::FaceDown
    } else {
        Facing::FaceUp
    };
    StepPart {
        connector: crate::model::Connector::Then,
        entry: Entry {
            action: mv.action,
            zone,
            facing,
            ..Entry::new(mv.slot.clone())
        },
    }
}

/// Erzeugt den Zug für einen gespeicherten Schritt-Teil auf dem aktuellen
/// Feld (für „Lösung zeigen“ und zum Wiederaufbau beim Aufnehmen).
/// `is_extra` sagt, ob die Karte ein Extra-Deck-Monster ist.
pub fn part_to_move(board: &Board, entry: &Entry, kind: CardKind, is_extra: bool) -> Move {
    let from = board.find(&entry.slot).unwrap_or(if is_extra {
        Area::ExtraDeck
    } else {
        Area::Deck
    });
    let dest = dest_for(entry.action, kind, from);
    let to = if dest.needs_choice() {
        if dest == Dest::ChooseHandOrGy {
            Some(if entry.zone == Zone::Graveyard {
                Area::Gy
            } else {
                Area::Hand
            })
        } else {
            // Gespeicherte Zone bevorzugen; Extra-Deck-Monster in die Extra-Monsterzone.
            let prefer = if entry.zone != Zone::None {
                entry.zone
            } else if is_extra {
                Zone::ExtraMonsterZone
            } else {
                Zone::None
            };
            board.first_free(dest, prefer)
        }
    } else {
        None
    };
    Move {
        slot: entry.slot.clone(),
        from,
        action: entry.action,
        to,
    }
}

/// Alle Teile aller Schritte der Reihe nach, mit (Schritt, Teil)-Index.
pub fn flatten(steps: &[Step]) -> Vec<(usize, usize)> {
    steps
        .iter()
        .enumerate()
        .flat_map(|(si, s)| (0..s.parts.len()).map(move |pi| (si, pi)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_menu_matches_card_and_place() {
        let monster = CardTraits::default();
        let hand = context_actions(&monster, Area::Hand, false);
        assert_eq!(hand[0], Action::NormalSummon);
        assert!(!hand.contains(&Action::PlacePendulum));
        let link = CardTraits {
            extra_summon: Some(Action::LinkSummon),
            ..Default::default()
        };
        assert_eq!(
            context_actions(&link, Area::ExtraDeck, false)[0],
            Action::LinkSummon
        );
        let trap = CardTraits {
            kind: CardKind::Trap,
            ..Default::default()
        };
        assert_eq!(
            context_actions(&trap, Area::Hand, false)[0],
            Action::SetSpellTrap
        );
        assert!(context_actions(&monster, Area::Monster(0), true).contains(&Action::FlipSummon));
        // Ziehen auf eine Zone: nur passende Aktionen.
        assert!(action_reaches(
            Action::NormalSummon,
            CardKind::Monster,
            Area::Hand,
            Area::Monster(1)
        ));
        assert!(!action_reaches(
            Action::NormalSummon,
            CardKind::Monster,
            Area::Hand,
            Area::SpellTrap(1)
        ));
        assert!(action_reaches(
            Action::Discard,
            CardKind::Monster,
            Area::Hand,
            Area::Gy
        ));
    }

    #[test]
    fn activating_a_set_card_turns_it_face_up() {
        let mut b = Board::with_hand(&[Entry::new(card(6))]);
        let set = Move {
            slot: card(6),
            from: Area::Hand,
            action: Action::SetSpellTrap,
            to: Some(Area::SpellTrap(0)),
        };
        b.apply(&set, CardKind::Trap).unwrap();
        let act = Move {
            slot: card(6),
            from: Area::SpellTrap(0),
            action: Action::Activate,
            to: None,
        };
        b.apply(&act, CardKind::Trap).unwrap();
        assert!(!b.zone(Area::SpellTrap(0)).unwrap().face_down);
    }

    fn card(id: u32) -> Slot {
        Slot::Card {
            id,
            image_id: id,
            name: format!("C{id}"),
        }
    }

    #[test]
    fn normal_summon_then_send_to_gy() {
        let mut b = Board::with_hand(&[Entry::new(card(1))]);
        let mv = Move {
            slot: card(1),
            from: Area::Hand,
            action: Action::NormalSummon,
            to: Some(Area::Monster(2)),
        };
        b.apply(&mv, CardKind::Monster).unwrap();
        assert!(b.hand.is_empty());
        assert!(b.zone(Area::Monster(2)).is_some());
        let mv = Move {
            slot: card(1),
            from: Area::Monster(2),
            action: Action::UseAsMaterial,
            to: None,
        };
        b.apply(&mv, CardKind::Monster).unwrap();
        assert_eq!(b.gy.len(), 1);
        assert_eq!(b.find(&card(1)), Some(Area::Gy));
    }

    #[test]
    fn occupied_zone_and_missing_zone_are_errors() {
        let mut b = Board::with_hand(&[Entry::new(card(1)), Entry::new(card(2))]);
        let mut mv = Move {
            slot: card(1),
            from: Area::Hand,
            action: Action::SpecialSummon,
            to: Some(Area::Monster(0)),
        };
        b.apply(&mv, CardKind::Monster).unwrap();
        mv.slot = card(2);
        assert_eq!(
            b.apply(&mv, CardKind::Monster),
            Err(MoveError::ZoneOccupied)
        );
        mv.to = None;
        assert_eq!(b.apply(&mv, CardKind::Monster), Err(MoveError::ZoneMissing));
        mv.to = Some(Area::SpellTrap(0));
        assert_eq!(b.apply(&mv, CardKind::Monster), Err(MoveError::WrongZone));
        assert_eq!(b.hand.len(), 1);
    }

    #[test]
    fn spell_activation_and_set_trap() {
        let mut b = Board::with_hand(&[Entry::new(card(5)), Entry::new(card(6))]);
        b.apply(
            &Move {
                slot: card(5),
                from: Area::Hand,
                action: Action::Activate,
                to: Some(Area::SpellTrap(1)),
            },
            CardKind::Spell,
        )
        .unwrap();
        b.apply(
            &Move {
                slot: card(6),
                from: Area::Hand,
                action: Action::SetSpellTrap,
                to: Some(Area::SpellTrap(2)),
            },
            CardKind::Trap,
        )
        .unwrap();
        assert!(!b.zone(Area::SpellTrap(1)).unwrap().face_down);
        assert!(b.zone(Area::SpellTrap(2)).unwrap().face_down);
        let entries = b.field_entries();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].facing, Facing::FaceDown);
    }

    #[test]
    fn search_from_deck_and_pendulum_scale() {
        let mut b = Board::default();
        b.apply(
            &Move {
                slot: card(7),
                from: Area::Deck,
                action: Action::Search,
                to: Some(Area::Hand),
            },
            CardKind::Monster,
        )
        .unwrap();
        b.apply(
            &Move {
                slot: card(7),
                from: Area::Hand,
                action: Action::PlacePendulum,
                to: Some(Area::SpellTrap(0)),
            },
            CardKind::Monster,
        )
        .unwrap();
        assert_eq!(b.find(&card(7)), Some(Area::SpellTrap(0)));
    }

    #[test]
    fn record_and_replay_roundtrip() {
        let mut b = Board::with_hand(&[Entry::new(card(1))]);
        let mv = Move {
            slot: card(1),
            from: Area::Hand,
            action: Action::NormalSummon,
            to: Some(Area::Monster(1)),
        };
        let part = move_to_part(&mv, CardKind::Monster);
        assert_eq!(part.entry.zone, Zone::MonsterZone);
        let replay = part_to_move(&b, &part.entry, CardKind::Monster, false);
        assert_eq!(replay.from, Area::Hand);
        b.apply(&replay, CardKind::Monster).unwrap();
        assert_eq!(b.find(&card(1)).map(|a| a.zone()), Some(Zone::MonsterZone));
    }

    #[test]
    fn extra_deck_monster_goes_to_emz() {
        let b = Board::default();
        let e = Entry {
            action: Action::LinkSummon,
            ..Entry::new(card(9))
        };
        let mv = part_to_move(&b, &e, CardKind::Monster, true);
        assert_eq!(mv.from, Area::ExtraDeck);
        assert!(matches!(mv.to, Some(Area::Emz(_))));
    }
}
