//! Spielfeld-Oberfläche. Wird zum Aufnehmen (Editor) und zum Üben benutzt.
//!
//! Ablauf eines Zugs:
//! 1. Karte anklicken (Hand, Feld, Stapel) oder aus Deck/Extra Deck holen.
//! 2. Aktion in der Aktionsleiste wählen.
//! 3. Falls nötig: Zielzone auf dem Feld anklicken (bzw. Hand/Friedhof).

use egui::{Align2, Color32, FontId, RichText, Sense, Stroke, StrokeKind, Vec2};

use super::widgets::{
    CARD_RATIO, Marks, SELECT_COLOR, TARGET_COLOR, View, card_widget, kind_of, shown_name,
};
use crate::board::{Area, Board, Dest, FieldCard, Move, dest_for};
use crate::i18n::{tr, trf};
use crate::images::ImageCache;
use crate::model::{Action, Slot};

#[derive(Clone, Debug, Default)]
pub enum Pending {
    #[default]
    None,
    Card {
        slot: Slot,
        from: Area,
    },
    Zone {
        slot: Slot,
        from: Area,
        action: Action,
        dest: Dest,
    },
}

/// Zustand der Bedienung des Spielfelds.
#[derive(Default)]
pub struct FieldUi {
    pub pending: Pending,
    pub chain_no: u8,
    /// Welcher Stapel aufgeklappt ist.
    pub open_pile: Option<Area>,
}

/// Was die Spielfeld-Oberfläche in diesem Frame ausgelöst hat.
pub enum FieldEvent {
    Move(Move),
    /// Der Nutzer will eine Karte aus Deck oder Extra Deck holen.
    Pick(Area),
}

pub fn area_label(a: Area) -> String {
    match a {
        Area::Hand => tr("area.hand").into(),
        Area::Deck => tr("area.deck").into(),
        Area::ExtraDeck => tr("area.extra_deck").into(),
        Area::Gy => tr("area.gy").into(),
        Area::Banished => tr("area.banished").into(),
        Area::Overlay => tr("area.overlay").into(),
        Area::Monster(i) => trf("area.monster_n", &[&(i + 1)]),
        Area::Emz(i) => trf("area.emz_n", &[&(i + 1)]),
        Area::SpellTrap(i) => trf("area.spell_trap_n", &[&(i + 1)]),
        Area::PendulumLeft => tr("area.pendulum_left").into(),
        Area::PendulumRight => tr("area.pendulum_right").into(),
        Area::Field => tr("area.field").into(),
    }
}

fn short_label(a: Area) -> &'static str {
    match a {
        Area::Monster(_) => tr("cell.monster"),
        Area::Emz(_) => tr("cell.emz"),
        Area::SpellTrap(_) => tr("cell.spell_trap"),
        Area::PendulumLeft | Area::PendulumRight => tr("cell.pendulum"),
        Area::Field => tr("cell.field"),
        Area::Gy => tr("cell.gy"),
        Area::Banished => tr("cell.banished"),
        Area::Overlay => tr("cell.overlay"),
        Area::ExtraDeck => tr("cell.extra"),
        _ => "",
    }
}

/// Leeres Feld mit Umriss und Beschriftung.
fn empty_cell(
    ui: &mut egui::Ui,
    w: f32,
    label: &str,
    highlight: Option<Color32>,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, w * CARD_RATIO), Sense::click());
    let p = ui.painter();
    let fill = if resp.hovered() {
        Color32::from_white_alpha(14)
    } else {
        Color32::from_white_alpha(6)
    };
    p.rect(
        rect,
        6.0,
        fill,
        Stroke::new(1.0, Color32::from_white_alpha(40)),
        StrokeKind::Inside,
    );
    p.text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional((w / 6.0).clamp(9.0, 13.0)),
        Color32::from_white_alpha(90),
    );
    if let Some(c) = highlight {
        p.rect_stroke(rect, 6.0, Stroke::new(2.5, c), StrokeKind::Outside);
    }
    resp
}

impl FieldUi {
    pub fn reset(&mut self) {
        self.pending = Pending::None;
        self.open_pile = None;
    }

    pub fn select(&mut self, slot: Slot, from: Area) {
        self.pending = Pending::Card { slot, from };
    }

    fn selected(&self) -> Option<(&Slot, Area)> {
        match &self.pending {
            Pending::Card { slot, from } | Pending::Zone { slot, from, .. } => Some((slot, *from)),
            Pending::None => None,
        }
    }

    fn target_dest(&self) -> Option<Dest> {
        match &self.pending {
            Pending::Zone { dest, .. } => Some(*dest),
            _ => None,
        }
    }

    /// Zeichnet Spielfeld und Aktionsleiste. `width` = Kartenbreite.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        board: &Board,
        images: &mut ImageCache,
        v: View,
        width: f32,
    ) -> Option<FieldEvent> {
        let mut event = None;
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                if let Some(e) = self.board_ui(ui, board, images, v, width) {
                    event = Some(e);
                }
            });
            ui.add_space(8.0);
            ui.vertical(|ui| {
                ui.set_width(270.0);
                if let Some(e) = self.action_panel(ui, images, v) {
                    event = Some(e);
                }
            });
        });
        if let Some(FieldEvent::Move(_)) = &event {
            self.pending = Pending::None;
        }
        event
    }

    fn board_ui(
        &mut self,
        ui: &mut egui::Ui,
        board: &Board,
        images: &mut ImageCache,
        v: View,
        w: f32,
    ) -> Option<FieldEvent> {
        let mut event = None;
        let gap = ui.spacing().item_spacing.x;
        let selected_from = self.selected().map(|(_, a)| a);
        let dest = self.target_dest();

        // Reihe 1: Extra-Monsterzonen über Monsterzone 2 und 4, rechts: Verbannt.
        ui.horizontal(|ui| {
            ui.add_space(2.0 * (w + gap));
            if let Some(e) = self.zone_cell(ui, board, images, v, w, Area::Emz(0)) {
                event = Some(e);
            }
            ui.add_space(w + gap);
            if let Some(e) = self.zone_cell(ui, board, images, v, w, Area::Emz(1)) {
                event = Some(e);
            }
            ui.add_space(2.0 * (w + gap));
            if let Some(e) = self.pile(ui, board, images, v, w, Area::Banished, &board.banished) {
                event = Some(e);
            }
        });
        // Reihe 2: Feldzauber, 5 Monsterzonen, Friedhof, Xyz-Material.
        ui.horizontal(|ui| {
            if let Some(e) = self.zone_cell(ui, board, images, v, w, Area::Field) {
                event = Some(e);
            }
            for i in 0..5 {
                if let Some(e) = self.zone_cell(ui, board, images, v, w, Area::Monster(i)) {
                    event = Some(e);
                }
            }
            if let Some(e) = self.pile(ui, board, images, v, w, Area::Gy, &board.gy) {
                event = Some(e);
            }
            if let Some(e) = self.pile(ui, board, images, v, w, Area::Overlay, &board.overlay) {
                event = Some(e);
            }
        });
        // Reihe 3: Pendel links, 5 Zauber/Fallen, Pendel rechts, Extra Deck (offen).
        ui.horizontal(|ui| {
            if let Some(e) = self.zone_cell(ui, board, images, v, w, Area::PendulumLeft) {
                event = Some(e);
            }
            for i in 0..5 {
                if let Some(e) = self.zone_cell(ui, board, images, v, w, Area::SpellTrap(i)) {
                    event = Some(e);
                }
            }
            if let Some(e) = self.zone_cell(ui, board, images, v, w, Area::PendulumRight) {
                event = Some(e);
            }
            if let Some(e) = self.pile(
                ui,
                board,
                images,
                v,
                w,
                Area::ExtraDeck,
                &board.extra_face_up,
            ) {
                event = Some(e);
            }
        });

        // Aufgeklappter Stapel
        if let Some(area) = self.open_pile {
            let cards = match area {
                Area::Gy => &board.gy,
                Area::Banished => &board.banished,
                Area::Overlay => &board.overlay,
                _ => &board.extra_face_up,
            };
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(area_label(area)).strong());
                if ui.small_button(tr("field.close_pile")).clicked() {
                    self.open_pile = None;
                }
            });
            ui.horizontal_wrapped(|ui| {
                for c in cards.iter() {
                    if card_widget(ui, images, v, &c.slot, Marks::default(), w * 0.8, None)
                        .clicked()
                    {
                        self.select(c.slot.clone(), area);
                    }
                }
                if cards.is_empty() {
                    ui.label(RichText::new(tr("field.pile_empty")).weak());
                }
            });
        }

        // Hand
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr("area.hand")).strong());
            let hand_target = dest == Some(Dest::ChooseHandOrGy);
            if hand_target && ui.button(tr("field.to_hand")).clicked() {
                event = self.finish_zone(Area::Hand);
            }
            if hand_target && ui.button(tr("field.to_gy")).clicked() {
                event = self.finish_zone(Area::Gy);
            }
        });
        ui.horizontal_wrapped(|ui| {
            for c in &board.hand {
                let hl = (selected_from == Some(Area::Hand)
                    && self.selected().is_some_and(|(s, _)| s == &c.slot))
                .then_some(SELECT_COLOR);
                if card_widget(ui, images, v, &c.slot, Marks::default(), w, hl).clicked() {
                    self.select(c.slot.clone(), Area::Hand);
                }
            }
            if board.hand.is_empty() {
                ui.label(RichText::new(tr("field.hand_empty")).weak());
            }
        });
        ui.horizontal(|ui| {
            if ui
                .button(tr("field.from_deck"))
                .on_hover_text(tr("field.from_deck_hint"))
                .clicked()
            {
                event = Some(FieldEvent::Pick(Area::Deck));
            }
            if ui.button(tr("field.from_extra")).clicked() {
                event = Some(FieldEvent::Pick(Area::ExtraDeck));
            }
        });
        event
    }

    /// Eine Zone des Felds zeichnen und Klicks auswerten.
    fn zone_cell(
        &mut self,
        ui: &mut egui::Ui,
        board: &Board,
        images: &mut ImageCache,
        v: View,
        w: f32,
        area: Area,
    ) -> Option<FieldEvent> {
        let card = board.zone(area);
        let dest = self.target_dest();
        let is_target = dest.is_some_and(|d| d.accepts(area)) && card.is_none();
        let hl = if self.selected().map(|(_, a)| a) == Some(area) {
            Some(SELECT_COLOR)
        } else if is_target {
            Some(TARGET_COLOR)
        } else {
            None
        };
        let resp = match card {
            Some(c) => card_widget(ui, images, v, &c.slot, Marks::face_down(c.face_down), w, hl),
            None => empty_cell(ui, w, short_label(area), hl),
        };
        if resp.clicked() {
            if is_target {
                return self.finish_zone(area);
            } else if let Some(c) = card {
                self.select(c.slot.clone(), area);
            }
        }
        None
    }

    fn finish_zone(&mut self, area: Area) -> Option<FieldEvent> {
        if let Pending::Zone {
            slot, from, action, ..
        } = &self.pending
        {
            return Some(FieldEvent::Move(Move {
                slot: slot.clone(),
                from: *from,
                action: *action,
                to: Some(area),
            }));
        }
        None
    }

    /// Stapel (Friedhof, Verbannt, …): oberste Karte und Anzahl.
    #[allow(clippy::too_many_arguments)]
    fn pile(
        &mut self,
        ui: &mut egui::Ui,
        _board: &Board,
        images: &mut ImageCache,
        v: View,
        w: f32,
        area: Area,
        cards: &[FieldCard],
    ) -> Option<FieldEvent> {
        let hl = (self.open_pile == Some(area)).then_some(SELECT_COLOR);
        let resp = match cards.last() {
            Some(c) => card_widget(ui, images, v, &c.slot, Marks::default(), w, hl),
            None => empty_cell(ui, w, short_label(area), hl),
        };
        if !cards.is_empty() {
            super::widgets::pill(
                ui.painter(),
                resp.rect.center_bottom() + Vec2::new(0.0, -3.0),
                Align2::CENTER_BOTTOM,
                &format!("{} {}", short_label(area), cards.len()),
                11.0,
                Color32::from_black_alpha(220),
            );
        }
        if resp.clicked() {
            self.open_pile = if self.open_pile == Some(area) {
                None
            } else {
                Some(area)
            };
        }
        None
    }

    fn action_panel(
        &mut self,
        ui: &mut egui::Ui,
        images: &mut ImageCache,
        v: View,
    ) -> Option<FieldEvent> {
        let mut event = None;
        match self.pending.clone() {
            Pending::None => {
                ui.label(RichText::new(tr("field.how_title")).strong());
                ui.label(tr("field.how_1"));
                ui.label(tr("field.how_2"));
                ui.label(tr("field.how_3"));
            }
            Pending::Card { slot, from } => {
                ui.horizontal(|ui| {
                    card_widget(ui, images, v, &slot, Marks::default(), 48.0, None);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(shown_name(v, &slot)).strong());
                        ui.label(trf("field.from", &[&area_label(from)]));
                    });
                });
                ui.add_space(4.0);
                ui.label(RichText::new(tr("field.choose_action")).strong());
                let kind = kind_of(v.db, &slot);
                egui::ScrollArea::vertical()
                    .max_height(520.0)
                    .show(ui, |ui| {
                        for g in Action::GROUPS {
                            ui.label(RichText::new(tr(g.key)).color(SELECT_COLOR).small());
                            ui.horizontal_wrapped(|ui| {
                                for a in g.actions {
                                    let a = match a {
                                        Action::Chain(_) => Action::Chain(self.chain_no.max(1)),
                                        other => *other,
                                    };
                                    let text = match a {
                                        Action::Chain(_) => tr("action.chain").to_string(),
                                        other => other.label(),
                                    };
                                    if ui.small_button(text).clicked() {
                                        let dest = dest_for(a, kind, from);
                                        if dest.needs_choice() {
                                            self.pending = Pending::Zone {
                                                slot: slot.clone(),
                                                from,
                                                action: a,
                                                dest,
                                            };
                                        } else {
                                            event = Some(FieldEvent::Move(Move {
                                                slot: slot.clone(),
                                                from,
                                                action: a,
                                                to: None,
                                            }));
                                        }
                                    }
                                    if matches!(a, Action::Chain(_)) {
                                        if self.chain_no == 0 {
                                            self.chain_no = 1;
                                        }
                                        ui.add(
                                            egui::DragValue::new(&mut self.chain_no).range(1..=99),
                                        )
                                        .on_hover_text(tr("action.chain_number"));
                                    }
                                }
                            });
                        }
                    });
                if ui.button(tr("common.cancel")).clicked() {
                    self.pending = Pending::None;
                }
            }
            Pending::Zone {
                slot, action, dest, ..
            } => {
                ui.label(RichText::new(shown_name(v, &slot)).strong());
                ui.label(action.label());
                ui.add_space(4.0);
                let hint = if dest == Dest::ChooseHandOrGy {
                    tr("field.choose_hand_or_gy")
                } else {
                    tr("field.choose_zone")
                };
                ui.label(RichText::new(hint).color(TARGET_COLOR).strong());
                if dest == Dest::ChooseHandOrGy {
                    ui.horizontal(|ui| {
                        if ui.button(tr("field.to_hand")).clicked() {
                            event = self.finish_zone(Area::Hand);
                        }
                        if ui.button(tr("field.to_gy")).clicked() {
                            event = self.finish_zone(Area::Gy);
                        }
                    });
                }
                if ui.button(tr("common.back")).clicked()
                    && let Pending::Zone { slot, from, .. } = self.pending.clone()
                {
                    self.pending = Pending::Card { slot, from };
                }
            }
        }
        event
    }
}
