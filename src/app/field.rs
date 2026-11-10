//! Spielfeld-Oberfläche. Wird zum Aufnehmen (Editor) und zum Üben benutzt.
//! Die Bedienung orientiert sich an digitalen Kartenspielen:
//!
//! - Karte anklicken: Direkt an der Karte erscheint ein Menü mit den
//!   Aktionen, die an diesem Ort üblich sind.
//! - Karte ziehen: Karte mit gedrückter Maustaste auf eine Zone, den
//!   Friedhof, den Verbannt-Stapel oder die Hand ziehen. Passt nur eine
//!   Aktion, wird sie sofort ausgeführt, sonst erscheint ein kurzes Menü.
//! - Deck und Extra Deck sind Stapel auf dem Feld. Ein Klick öffnet die
//!   Kartenauswahl.
//! - Esc oder Rechtsklick bricht ab.

use egui::{Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Vec2};

use super::widgets::{
    CARD_RATIO, GOOD_COLOR, Marks, SELECT_COLOR, TARGET_COLOR, View, card_widget_sense, is_extra,
    shown_name, traits_of,
};
use crate::board::{Area, Board, Dest, FieldCard, Move, action_reaches, context_actions, dest_for};
use crate::i18n::{tr, trf};
use crate::images::ImageCache;
use crate::model::{Action, Slot, Zone};

#[derive(Clone, Debug, Default)]
pub enum Pending {
    #[default]
    None,
    /// Karte gewählt, Menü ist offen.
    Card {
        slot: Slot,
        from: Area,
        /// Position des Menüs.
        pos: Pos2,
        /// Zielzone steht schon fest (Karte wurde dorthin gezogen).
        to: Option<Area>,
        /// Nur diese Aktionen anbieten (nach dem Ziehen).
        only: Option<Vec<Action>>,
        show_all: bool,
    },
    /// Aktion gewählt, Zielzone fehlt noch.
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
    /// Tipp im Übungsmodus: Diese Karte wird grün markiert.
    pub hint: Option<Slot>,
    /// Karte, die gerade mit der Maus gezogen wird.
    drag: Option<(Slot, Area)>,
    /// Zone unter dem Mauszeiger während des Ziehens.
    hover: Option<Area>,
    /// Das Menü wurde in diesem Frame geöffnet (Klick nicht als „daneben“ werten).
    menu_fresh: bool,
    deck_rect: Option<Rect>,
    extra_rect: Option<Rect>,
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
        Area::SpellTrap(i @ (0 | 4)) => format!(
            "{} ({})",
            trf("area.spell_trap_n", &[&(i + 1)]),
            tr("area.pendulum_zone")
        ),
        Area::SpellTrap(i) => trf("area.spell_trap_n", &[&(i + 1)]),
        Area::Field => tr("area.field").into(),
    }
}

fn short_label(a: Area) -> &'static str {
    match a {
        Area::Monster(_) => tr("cell.monster"),
        Area::Emz(_) => tr("cell.emz"),
        Area::SpellTrap(0 | 4) => tr("cell.spell_trap_pendulum"),
        Area::SpellTrap(_) => tr("cell.spell_trap"),
        Area::Field => tr("cell.field"),
        Area::Gy => tr("cell.gy"),
        Area::Banished => tr("cell.banished"),
        Area::Overlay => tr("cell.overlay"),
        Area::ExtraDeck => tr("cell.extra"),
        Area::Deck => tr("cell.deck"),
        Area::Hand => "",
    }
}

/// Leeres Feld mit Umriss und Beschriftung. `solid` zeichnet einen
/// Kartenrücken (für Deck und Extra Deck).
fn empty_cell(
    ui: &mut egui::Ui,
    w: f32,
    label: &str,
    highlight: Option<Color32>,
    solid: bool,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, w * CARD_RATIO), Sense::click());
    let p = ui.painter();
    let fill = match (solid, resp.hovered()) {
        (true, true) => Color32::from_rgb(86, 62, 150),
        (true, false) => Color32::from_rgb(66, 48, 120),
        (false, true) => Color32::from_white_alpha(14),
        (false, false) => Color32::from_white_alpha(6),
    };
    p.rect(
        rect,
        6.0,
        fill,
        Stroke::new(1.0, Color32::from_white_alpha(if solid { 110 } else { 40 })),
        StrokeKind::Inside,
    );
    if solid {
        p.rect_stroke(
            rect.shrink(5.0),
            4.0,
            Stroke::new(1.0, Color32::from_white_alpha(60)),
            StrokeKind::Inside,
        );
    }
    p.text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional((w / 6.0).clamp(9.0, 13.0)),
        Color32::from_white_alpha(if solid { 230 } else { 90 }),
    );
    if let Some(c) = highlight {
        p.rect_stroke(rect, 6.0, Stroke::new(2.5, c), StrokeKind::Outside);
    }
    resp
}

/// Gemeinsame Daten eines Frames.
struct Cx<'a, 'b> {
    board: &'a Board,
    images: &'a mut ImageCache,
    v: View<'b>,
    w: f32,
    auto_zone: bool,
    event: Option<FieldEvent>,
    hint_area: Option<Area>,
}

impl FieldUi {
    pub fn reset(&mut self) {
        self.pending = Pending::None;
        self.open_pile = None;
        self.drag = None;
        self.hint = None;
    }

    /// Wählt eine Karte, die aus der Kartenauswahl kommt (Deck/Extra Deck).
    pub fn select(&mut self, slot: Slot, from: Area) {
        let rect = if from == Area::ExtraDeck {
            self.extra_rect
        } else {
            self.deck_rect
        };
        let pos = rect
            .map(|r| r.right_top() + Vec2::new(6.0, 0.0))
            .unwrap_or(Pos2::new(320.0, 320.0));
        self.select_at(slot, from, pos);
    }

    fn select_at(&mut self, slot: Slot, from: Area, pos: Pos2) {
        self.pending = Pending::Card {
            slot,
            from,
            pos,
            to: None,
            only: None,
            show_all: false,
        };
        self.menu_fresh = true;
    }

    fn selected(&self) -> Option<(&Slot, Area)> {
        match &self.pending {
            Pending::Card { slot, from, .. } | Pending::Zone { slot, from, .. } => {
                Some((slot, *from))
            }
            Pending::None => self.drag.as_ref().map(|(s, a)| (s, *a)),
        }
    }

    /// Zielart, für die gerade Zonen markiert werden.
    fn target_dest(&self) -> Option<Dest> {
        match &self.pending {
            Pending::Zone { dest, .. } => Some(*dest),
            _ => None,
        }
    }

    /// Zeichnet das Spielfeld. `width` = Kartenbreite.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        board: &Board,
        images: &mut ImageCache,
        v: View,
        width: f32,
        auto_zone: bool,
    ) -> Option<FieldEvent> {
        let ctx = ui.ctx().clone();
        // Abbrechen mit Esc oder Rechtsklick.
        if ctx.input(|i| i.key_pressed(egui::Key::Escape) || i.pointer.secondary_clicked()) {
            self.pending = Pending::None;
            self.drag = None;
        }
        self.hover = None;
        let hint_area = self.hint.as_ref().map(|s| {
            board.find(s).unwrap_or(if is_extra(v.db, s) {
                Area::ExtraDeck
            } else {
                Area::Deck
            })
        });
        // Kartenbreite: so groß, dass 7 Spalten und 4 Reihen in den
        // sichtbaren Bereich passen. `width` ist nur die Untergrenze.
        let gap = ui.spacing().item_spacing;
        let avail_w = ui.available_width();
        let by_width = (avail_w - 6.0 * gap.x) / 7.0;
        let screen_h = ctx.content_rect().height();
        let by_height = (screen_h - 430.0 - 3.0 * gap.y) / (4.0 * CARD_RATIO);
        let w = by_width.min(by_height).clamp(width.min(56.0), 170.0);
        let board_w = 7.0 * w + 6.0 * gap.x;
        let pad = ((avail_w - board_w) / 2.0).max(0.0);
        let mut cx = Cx {
            board,
            images,
            v,
            w,
            auto_zone,
            event: None,
            hint_area,
        };

        ui.horizontal_top(|ui| {
            ui.add_space(pad);
            ui.vertical(|ui| {
                ui.set_width(board_w);
                self.status_line(ui);
                self.board_ui(ui, &mut cx);
            });
        });
        self.menu(&ctx, &mut cx);
        self.drag_and_drop(&ctx, &mut cx);

        self.menu_fresh = false;
        let event = cx.event;
        if let Some(FieldEvent::Move(mv)) = &event {
            if let Action::Chain(n) = mv.action {
                // Das nächste Kettenglied hat die nächste Nummer.
                self.chain_no = n.saturating_add(1);
            }
            self.pending = Pending::None;
            self.hint = None;
        }
        event
    }

    fn status_line(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| match &self.pending {
            Pending::Zone {
                slot: _, action, ..
            } => {
                ui.label(
                    RichText::new(format!("{}: {}", action.label(), tr("field.choose_zone")))
                        .color(TARGET_COLOR)
                        .strong(),
                );
                if ui.small_button(tr("common.cancel")).clicked() {
                    self.pending = Pending::None;
                }
            }
            _ => {
                ui.label(RichText::new(tr("field.status_idle")).weak());
            }
        });
    }

    fn board_ui(&mut self, ui: &mut egui::Ui, cx: &mut Cx) {
        let w = cx.w;
        let gap = ui.spacing().item_spacing.x;
        let board = cx.board;

        // Aufbau wie auf dem echten Spielfeld (Master Rule 5):
        // Reihe 1: 2 Extra-Monsterzonen über Monsterzone 2 und 4; rechts Verbannt.
        // Xyz-Material ist keine echte Zone und erscheint nur, wenn es welches gibt.
        ui.horizontal(|ui| {
            if board.overlay.is_empty() {
                ui.add_space(w + gap);
            } else {
                self.pile(ui, cx, Area::Overlay, &board.overlay);
            }
            ui.add_space(w + gap);
            self.zone_cell(ui, cx, Area::Emz(0));
            ui.add_space(w + gap);
            self.zone_cell(ui, cx, Area::Emz(1));
            ui.add_space(w + gap);
            self.pile(ui, cx, Area::Banished, &board.banished);
        });
        // Reihe 2: Feldzauberzone, 5 Monsterzonen, Friedhof.
        ui.horizontal(|ui| {
            self.zone_cell(ui, cx, Area::Field);
            for i in 0..5 {
                self.zone_cell(ui, cx, Area::Monster(i));
            }
            self.pile(ui, cx, Area::Gy, &board.gy);
        });
        // Reihe 3: Extra Deck, 5 Zauber-/Fallenzonen (die äußeren sind
        // zugleich die Pendelzonen), Deck.
        ui.horizontal(|ui| {
            self.deck_pile(ui, cx, Area::ExtraDeck);
            for i in 0..5 {
                self.zone_cell(ui, cx, Area::SpellTrap(i));
            }
            self.deck_pile(ui, cx, Area::Deck);
        });
        ui.add_space(6.0);
        // Reihe 4: Hand über die ganze Breite.
        {
            let hand_w = 7.0 * (w + gap) - gap;
            let inner = ui.allocate_ui_with_layout(
                Vec2::new(hand_w, w * CARD_RATIO),
                egui::Layout::left_to_right(egui::Align::Min).with_main_wrap(true),
                |ui| {
                    ui.set_min_size(Vec2::new(hand_w, w * CARD_RATIO));
                    // Hand in der Mitte ausrichten.
                    let n = board.hand.len().min(7) as f32;
                    let used = n * (w + gap) - gap;
                    if n > 0.0 && used < hand_w {
                        ui.add_space((hand_w - used) / 2.0);
                    }
                    for c in &board.hand {
                        self.card(ui, cx, c, Area::Hand, w, Marks::default());
                    }
                },
            );
            let rect = inner.response.rect;
            let droppable = self
                .drag
                .as_ref()
                .is_some_and(|(_, from)| *from != Area::Hand);
            let over = droppable && ui.rect_contains_pointer(rect);
            if over {
                self.hover = Some(Area::Hand);
            }
            let stroke = if over {
                Stroke::new(2.5, TARGET_COLOR)
            } else {
                Stroke::new(1.0, Color32::from_white_alpha(25))
            };
            ui.painter()
                .rect_stroke(rect.expand(3.0), 6.0, stroke, StrokeKind::Outside);
            if board.hand.is_empty() {
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    tr("area.hand"),
                    FontId::proportional(13.0),
                    Color32::from_white_alpha(90),
                );
            }
        }

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
                if area == Area::ExtraDeck && ui.small_button(tr("field.from_extra")).clicked() {
                    cx.event = Some(FieldEvent::Pick(Area::ExtraDeck));
                }
                if ui.small_button(tr("field.close_pile")).clicked() {
                    self.open_pile = None;
                }
            });
            ui.horizontal_wrapped(|ui| {
                for c in cards.iter() {
                    self.card(ui, cx, c, area, w * 0.8, Marks::default());
                }
                if cards.is_empty() {
                    ui.label(RichText::new(tr("field.pile_empty")).weak());
                }
            });
        }
    }

    /// Zeichnet eine Karte, die man anklicken und ziehen kann.
    fn card(
        &mut self,
        ui: &mut egui::Ui,
        cx: &mut Cx,
        c: &FieldCard,
        area: Area,
        w: f32,
        marks: Marks,
    ) -> egui::Response {
        let is_selected = self
            .selected()
            .is_some_and(|(s, a)| a == area && s == &c.slot);
        let is_hint = cx.hint_area == Some(area) && self.hint.as_ref() == Some(&c.slot);
        let hl = if is_selected {
            Some(SELECT_COLOR)
        } else if is_hint {
            Some(GOOD_COLOR)
        } else {
            None
        };
        let resp = card_widget_sense(
            ui,
            cx.images,
            cx.v,
            &c.slot,
            marks,
            w,
            hl,
            Sense::click_and_drag(),
        );
        if resp.drag_started() {
            self.drag = Some((c.slot.clone(), area));
            self.pending = Pending::None;
        }
        // Während auf eine Zielzone gewartet wird, wählt ein Klick keine neue Karte.
        if resp.clicked() && self.target_dest().is_none() {
            self.select_at(
                c.slot.clone(),
                area,
                resp.rect.right_top() + Vec2::new(6.0, 0.0),
            );
        }
        resp
    }

    /// Eine Zone des Felds zeichnen und Klicks auswerten.
    fn zone_cell(&mut self, ui: &mut egui::Ui, cx: &mut Cx, area: Area) {
        let card = cx.board.zone(area);
        let is_target = card.is_none() && self.target_dest().is_some_and(|d| d.accepts(area));
        let resp = match card {
            Some(c) => {
                let c = c.clone();
                self.card(ui, cx, &c, area, cx.w, Marks::face_down(c.face_down))
            }
            None => empty_cell(
                ui,
                cx.w,
                short_label(area),
                is_target.then_some(TARGET_COLOR),
                false,
            ),
        };
        self.drop_target(ui, cx, area, resp.rect, card.is_none());
        if resp.clicked() && is_target {
            cx.event = self.finish_zone(area);
        }
    }

    /// Merkt sich die Zone unter dem Mauszeiger, wenn die gezogene Karte
    /// mit irgendeiner üblichen Aktion dorthin kann, und markiert sie.
    fn drop_target(&mut self, ui: &mut egui::Ui, cx: &mut Cx, area: Area, rect: Rect, free: bool) {
        let Some((slot, from)) = &self.drag else {
            return;
        };
        if *from == area || !free {
            return;
        }
        let t = traits_of(cx.v.db, slot);
        let possible = Action::GROUPS
            .iter()
            .flat_map(|g| g.actions)
            .any(|a| action_reaches(*a, t.kind, *from, area));
        if !possible {
            return;
        }
        let over = ui.rect_contains_pointer(rect);
        let color = if over {
            TARGET_COLOR
        } else {
            TARGET_COLOR.gamma_multiply(0.35)
        };
        ui.painter()
            .rect_stroke(rect, 6.0, Stroke::new(2.5, color), StrokeKind::Outside);
        if over {
            self.hover = Some(area);
        }
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

    /// Stapel (Friedhof, Verbannt, Xyz-Material): oberste Karte und Anzahl.
    /// Ein Klick klappt den Stapel auf.
    fn pile(&mut self, ui: &mut egui::Ui, cx: &mut Cx, area: Area, cards: &[FieldCard]) {
        let hl = if self.open_pile == Some(area) {
            Some(SELECT_COLOR)
        } else if cx.hint_area == Some(area) {
            Some(GOOD_COLOR)
        } else {
            None
        };
        let resp = match cards.last() {
            Some(c) => super::widgets::card_widget(
                ui,
                cx.images,
                cx.v,
                &c.slot,
                Marks::default(),
                cx.w,
                hl,
            ),
            None => empty_cell(ui, cx.w, short_label(area), hl, false),
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
        self.drop_target(ui, cx, area, resp.rect, true);
        if resp.clicked() {
            self.open_pile = if self.open_pile == Some(area) {
                None
            } else {
                Some(area)
            };
        }
    }

    /// Deck oder Extra Deck als Kartenstapel. Ein Klick öffnet die Kartenauswahl.
    fn deck_pile(&mut self, ui: &mut egui::Ui, cx: &mut Cx, area: Area) {
        let hl = (cx.hint_area == Some(area)).then_some(GOOD_COLOR);
        let resp =
            empty_cell(ui, cx.w, short_label(area), hl, true).on_hover_text(tr("field.deck_hint"));
        let face_up = cx.board.extra_face_up.len();
        if area == Area::ExtraDeck {
            self.extra_rect = Some(resp.rect);
            if face_up > 0 {
                super::widgets::pill(
                    ui.painter(),
                    resp.rect.center_bottom() + Vec2::new(0.0, -3.0),
                    Align2::CENTER_BOTTOM,
                    &format!("+{face_up}"),
                    11.0,
                    Color32::from_black_alpha(220),
                );
            }
        } else {
            self.deck_rect = Some(resp.rect);
        }
        self.drop_target(ui, cx, area, resp.rect, true);
        if resp.clicked() {
            if area == Area::ExtraDeck && face_up > 0 {
                // Offene Karten im Extra Deck zeigen; von dort geht es zur Auswahl.
                self.open_pile = Some(Area::ExtraDeck);
            } else {
                cx.event = Some(FieldEvent::Pick(area));
            }
        }
    }

    /// Führt eine gewählte Aktion aus oder wartet auf die Zielzone.
    fn choose(&mut self, cx: &mut Cx, slot: Slot, from: Area, action: Action, to: Option<Area>) {
        let t = traits_of(cx.v.db, &slot);
        let dest = dest_for(action, t.kind, from);
        let mv = |to: Option<Area>| {
            Some(FieldEvent::Move(Move {
                slot: slot.clone(),
                from,
                action,
                to,
            }))
        };
        if !dest.needs_choice() {
            cx.event = mv(None);
            return;
        }
        if dest == Dest::ChooseHandOrGy {
            let target = to.filter(|a| dest.accepts(*a)).unwrap_or(Area::Hand);
            cx.event = mv(Some(target));
            return;
        }
        if let Some(to) = to.filter(|a| dest.accepts(*a) && cx.board.zone(*a).is_none()) {
            cx.event = mv(Some(to));
            return;
        }
        let free: Vec<Area> = Board::field_areas()
            .into_iter()
            .filter(|a| dest.accepts(*a) && cx.board.zone(*a).is_none())
            .collect();
        if free.len() == 1 {
            cx.event = mv(Some(free[0]));
        } else if cx.auto_zone && !free.is_empty() {
            let prefer = if t.extra_summon.is_some() {
                Zone::ExtraMonsterZone
            } else {
                Zone::None
            };
            cx.event = mv(cx.board.first_free(dest, prefer));
        } else {
            self.pending = Pending::Zone {
                slot,
                from,
                action,
                dest,
            };
        }
    }

    /// Menü direkt an der gewählten Karte.
    fn menu(&mut self, ctx: &egui::Context, cx: &mut Cx) {
        let Pending::Card {
            slot,
            from,
            pos,
            to,
            only,
            show_all,
        } = self.pending.clone()
        else {
            return;
        };
        let t = traits_of(cx.v.db, &slot);
        let face_down = cx.board.zone(from).is_some_and(|c| c.face_down);
        let actions = only
            .clone()
            .unwrap_or_else(|| context_actions(&t, from, face_down));
        let chain_no = self.chain_no.max(1);
        let mut chosen: Option<Action> = None;
        let mut toggle_all = false;
        let mut new_chain_no = chain_no;

        let area = egui::Area::new(egui::Id::unique("field_action_menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .constrain(true)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    let mut action_button = |ui: &mut egui::Ui, a: Action| {
                        let a = match a {
                            Action::Chain(_) => Action::Chain(new_chain_no),
                            other => other,
                        };
                        ui.horizontal(|ui| {
                            let is_chain = matches!(a, Action::Chain(_));
                            let text = if is_chain {
                                tr("action.chain").to_string()
                            } else {
                                a.label()
                            };
                            let width = if is_chain { 150.0 } else { 210.0 };
                            let button = egui::Button::new(text).min_size(Vec2::new(width, 0.0));
                            if ui.add(button).clicked() {
                                chosen = Some(a);
                            }
                            if is_chain {
                                ui.add(egui::DragValue::new(&mut new_chain_no).range(1..=99))
                                    .on_hover_text(tr("action.chain_number"));
                            }
                        });
                    };
                    ui.horizontal_top(|ui| {
                        // Linke Spalte: die üblichen Aktionen.
                        ui.vertical(|ui| {
                            ui.set_width(210.0);
                            ui.label(RichText::new(shown_name(cx.v, &slot)).strong());
                            let place = match to {
                                Some(to) => format!("{} → {}", area_label(from), area_label(to)),
                                None => trf("field.from", &[&area_label(from)]),
                            };
                            ui.label(RichText::new(place).weak().small());
                            ui.separator();
                            for a in &actions {
                                action_button(ui, *a);
                            }
                            if only.is_none() {
                                ui.separator();
                                if ui
                                    .selectable_label(show_all, tr("field.more_actions"))
                                    .clicked()
                                {
                                    toggle_all = true;
                                }
                            }
                        });
                        // Weitere Aktionen: alle übrigen, in 3 Spalten daneben
                        // und in voller Größe (kein Scrollen).
                        if show_all && only.is_none() {
                            let columns: [&[usize]; 3] = [&[0, 2], &[1], &[3, 4]];
                            for groups in columns {
                                ui.separator();
                                ui.vertical(|ui| {
                                    ui.set_width(210.0);
                                    for gi in groups {
                                        let g = &Action::GROUPS[*gi];
                                        let rest: Vec<Action> = g
                                            .actions
                                            .iter()
                                            .copied()
                                            .filter(|a| !actions.iter().any(|b| b.same_kind(*a)))
                                            .collect();
                                        if rest.is_empty() {
                                            continue;
                                        }
                                        ui.label(
                                            RichText::new(tr(g.key)).color(SELECT_COLOR).strong(),
                                        );
                                        for a in rest {
                                            action_button(ui, a);
                                        }
                                        ui.add_space(6.0);
                                    }
                                });
                            }
                        }
                    });
                });
            });

        self.chain_no = new_chain_no;
        if toggle_all && let Pending::Card { show_all, .. } = &mut self.pending {
            *show_all = !*show_all;
        }
        if let Some(a) = chosen {
            self.pending = Pending::None;
            self.choose(cx, slot, from, a, to);
            return;
        }
        // Klick neben das Menü schließt es.
        let rect = area.response.rect;
        let pressed_outside = ctx.input(|i| {
            i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !rect.contains(p))
        });
        if pressed_outside && !self.menu_fresh {
            self.pending = Pending::None;
        }
    }

    /// Zeichnet die gezogene Karte am Mauszeiger und wertet das Loslassen aus.
    fn drag_and_drop(&mut self, ctx: &egui::Context, cx: &mut Cx) {
        let Some((slot, from)) = self.drag.clone() else {
            return;
        };
        let pointer = ctx.input(|i| i.pointer.interact_pos());
        if let Some(p) = pointer {
            let size = Vec2::new(cx.w, cx.w * CARD_RATIO) * 0.9;
            let rect = Rect::from_center_size(p, size);
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Tooltip,
                egui::Id::unique("field_drag_ghost"),
            ));
            let tex = match &slot {
                Slot::Card { image_id, .. } => cx.images.get(*image_id).map(|t| t.id()),
                Slot::Placeholder { .. } => None,
            };
            match tex {
                Some(id) => {
                    let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
                    painter.image(id, rect, uv, Color32::from_white_alpha(215));
                }
                None => {
                    painter.rect_filled(rect, 5.0, Color32::from_rgb(90, 70, 150));
                }
            }
            painter.rect_stroke(
                rect,
                5.0,
                Stroke::new(2.0, SELECT_COLOR),
                StrokeKind::Outside,
            );
            ctx.request_repaint();
        }
        if !ctx.input(|i| i.pointer.any_released()) {
            return;
        }
        self.drag = None;
        let Some(to) = self.hover else { return };
        let t = traits_of(cx.v.db, &slot);
        let face_down = cx.board.zone(from).is_some_and(|c| c.face_down);
        let fits: Vec<Action> = context_actions(&t, from, face_down)
            .into_iter()
            .filter(|a| action_reaches(*a, t.kind, from, to))
            .collect();
        match fits.len() {
            0 => {
                // Keine übliche Aktion: alle passenden Aktionen anbieten.
                let all: Vec<Action> = Action::GROUPS
                    .iter()
                    .flat_map(|g| g.actions.iter().copied())
                    .filter(|a| action_reaches(*a, t.kind, from, to))
                    .collect();
                if !all.is_empty() {
                    self.open_drop_menu(slot, from, to, all, pointer);
                }
            }
            1 => self.choose(cx, slot, from, fits[0], Some(to)),
            _ => self.open_drop_menu(slot, from, to, fits, pointer),
        }
    }

    fn open_drop_menu(
        &mut self,
        slot: Slot,
        from: Area,
        to: Area,
        actions: Vec<Action>,
        pointer: Option<Pos2>,
    ) {
        self.pending = Pending::Card {
            slot,
            from,
            pos: pointer.unwrap_or(Pos2::new(320.0, 320.0)) + Vec2::new(8.0, 8.0),
            to: Some(to),
            only: Some(actions),
            show_all: false,
        };
        self.menu_fresh = true;
    }
}
