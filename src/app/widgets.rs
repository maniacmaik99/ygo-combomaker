//! Gemeinsame Bausteine zum Zeichnen von Karten.

use egui::{Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Vec2};

use crate::cards::CardDb;
use crate::i18n::tr;
use crate::images::ImageCache;
use crate::model::{Action, CardKind, Entry, Facing, Slot, Step, Zone};

pub const CARD_RATIO: f32 = 86.0 / 59.0;
pub const SELECT_COLOR: Color32 = crate::theme::ACCENT_GOLD;
pub const GOOD_COLOR: Color32 = crate::theme::GOOD;
pub const BAD_COLOR: Color32 = crate::theme::BAD;
pub const TARGET_COLOR: Color32 = crate::theme::ACCENT;

/// Gemeinsame Anzeige-Einstellungen für Karten.
#[derive(Clone, Copy)]
pub struct View<'a> {
    pub db: &'a CardDb,
    /// Sprachcode für Kartennamen, `None` = Englisch.
    pub name_lang: Option<&'static str>,
    /// Karten mit Beschränkung in der gewählten Spielversion (nach Karten-ID).
    pub ban: &'a std::collections::HashMap<u32, crate::banlist::Limit>,
}

/// Markierungen, die auf eine Karte gezeichnet werden.
#[derive(Clone, Copy, Default)]
pub struct Marks {
    pub action: Action,
    pub zone: Zone,
    pub facing: Facing,
}

impl Marks {
    pub fn of(e: &Entry) -> Self {
        Self {
            action: e.action,
            zone: e.zone,
            facing: e.facing,
        }
    }

    pub fn face_down(face_down: bool) -> Self {
        Self {
            facing: if face_down {
                Facing::FaceDown
            } else {
                Facing::Unspecified
            },
            ..Default::default()
        }
    }
}

pub fn kind_color(kind: CardKind) -> Color32 {
    match kind {
        CardKind::Monster => Color32::from_rgb(190, 115, 40),
        CardKind::Spell => Color32::from_rgb(25, 145, 125),
        CardKind::Trap => Color32::from_rgb(175, 45, 125),
    }
}

pub fn shown_name(v: View, slot: &Slot) -> String {
    match slot.card_id().and_then(|id| v.db.get(id)) {
        Some(card) => card.localized_name(v.name_lang).to_string(),
        None => slot.display_name().to_string(),
    }
}

/// Kartenart einer Karte (Platzhalter: eigene Art, unbekannt: Monster).
pub fn kind_of(db: &CardDb, slot: &Slot) -> CardKind {
    match slot {
        Slot::Placeholder { kind, .. } => *kind,
        Slot::Card { id, .. } => db.get(*id).map(|c| c.kind).unwrap_or_default(),
    }
}

/// Eigenschaften der Karte für das Aktionsmenü.
pub fn traits_of(db: &CardDb, slot: &Slot) -> crate::board::CardTraits {
    let mut t = crate::board::CardTraits {
        kind: kind_of(db, slot),
        ..Default::default()
    };
    if let Some(c) = slot.card_id().and_then(|id| db.get(id)) {
        let ty = &c.type_line;
        t.pendulum = ty.contains("Pendulum");
        t.ritual = ty.contains("Ritual");
        t.extra_summon = if ty.contains("Fusion") {
            Some(Action::FusionSummon)
        } else if ty.contains("Synchro") {
            Some(Action::SynchroSummon)
        } else if ty.contains("XYZ") {
            Some(Action::XyzSummon)
        } else if ty.contains("Link") {
            Some(Action::LinkSummon)
        } else {
            None
        };
    }
    t
}

pub fn is_extra(db: &CardDb, slot: &Slot) -> bool {
    slot.card_id()
        .and_then(|id| db.get(id))
        .is_some_and(|c| c.is_extra_deck())
}

/// Zeichnet eine abgerundete Plakette mit Text. `anchor` legt fest, an
/// welcher Ecke der Plakette `pos` liegt.
pub fn pill(
    painter: &egui::Painter,
    pos: Pos2,
    anchor: Align2,
    text: &str,
    size: f32,
    fill: Color32,
) {
    let galley =
        painter.layout_no_wrap(text.to_string(), FontId::proportional(size), Color32::WHITE);
    let pad = Vec2::new(4.0, 1.5);
    let rect = anchor.anchor_size(pos, galley.size() + pad * 2.0);
    painter.rect(
        rect,
        rect.height() / 2.0,
        fill,
        Stroke::new(1.0, Color32::from_white_alpha(200)),
        StrokeKind::Inside,
    );
    painter.galley(rect.min + pad, galley, Color32::WHITE);
}

/// Zeichnet eine Karte (Bild oder Platzhalter) mit Plaketten.
pub fn card_widget(
    ui: &mut egui::Ui,
    images: &mut ImageCache,
    v: View,
    slot: &Slot,
    marks: Marks,
    width: f32,
    highlight: Option<Color32>,
) -> egui::Response {
    card_widget_sense(ui, images, v, slot, marks, width, highlight, Sense::click())
}

/// Wie [`card_widget`], aber mit wählbarer Eingabe-Art (z. B. Ziehen).
#[allow(clippy::too_many_arguments)]
pub fn card_widget_sense(
    ui: &mut egui::Ui,
    images: &mut ImageCache,
    v: View,
    slot: &Slot,
    marks: Marks,
    width: f32,
    highlight: Option<Color32>,
    sense: Sense,
) -> egui::Response {
    let size = Vec2::new(width, width * CARD_RATIO);
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    let limit = slot.card_id().and_then(|id| v.ban.get(&id)).copied();
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let small = FontId::proportional((width / 6.0).clamp(9.0, 14.0));
        let draw_text = |text: &str, fill: Color32| {
            painter.rect_filled(rect, 5.0, fill);
            let galley =
                painter.layout(text.to_string(), small.clone(), Color32::WHITE, width - 8.0);
            let pos = rect.center() - galley.size() / 2.0;
            painter.galley(pos, galley, Color32::WHITE);
        };
        match slot {
            Slot::Card { image_id, .. } => {
                let px = width * ui.ctx().pixels_per_point();
                if let Some(tex) = images.get_for_width(*image_id, px) {
                    let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
                    painter.image(tex.id(), rect, uv, Color32::WHITE);
                } else if images.failed(*image_id) {
                    draw_text(&shown_name(v, slot), Color32::from_gray(55));
                } else {
                    draw_text("…", Color32::from_gray(40));
                }
            }
            Slot::Placeholder { kind, label } => draw_text(label, kind_color(*kind)),
        }
        if marks.facing == Facing::FaceDown {
            painter.rect_filled(rect, 5.0, Color32::from_black_alpha(150));
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                tr("card.face_down"),
                FontId::proportional((width / 6.0).clamp(9.0, 16.0)),
                Color32::WHITE,
            );
        }
        let font_size = (width / 7.5).clamp(9.0, 14.0);
        if let Some(badge) = marks.action.badge() {
            pill(
                painter,
                rect.left_bottom() + Vec2::new(2.0, -2.0),
                Align2::LEFT_BOTTOM,
                &badge,
                font_size,
                Color32::from_black_alpha(220),
            );
        }
        let mut zone_text = marks.zone.badge().unwrap_or("").to_string();
        if marks.facing == Facing::FaceUp {
            if !zone_text.is_empty() {
                zone_text.push(' ');
            }
            zone_text.push_str(tr("card.face_up_short"));
        }
        if !zone_text.is_empty() {
            pill(
                painter,
                rect.right_top() + Vec2::new(-2.0, 2.0),
                Align2::RIGHT_TOP,
                &zone_text,
                font_size,
                Color32::from_rgba_unmultiplied(40, 30, 110, 235),
            );
        }
        if let Some(limit) = limit {
            ban_badge(painter, rect, width, limit);
        }
        if let Some(color) = highlight {
            painter.rect_stroke(rect, 5.0, Stroke::new(3.0, color), StrokeKind::Outside);
        } else if resp.hovered() {
            painter.rect_stroke(
                rect,
                5.0,
                Stroke::new(1.5, Color32::from_white_alpha(120)),
                StrokeKind::Outside,
            );
        }
    }
    let name = shown_name(v, slot);
    resp.on_hover_ui(|ui| {
        ui.set_max_width(340.0);
        ui.label(RichText::new(&name).strong());
        if let Some(card) = slot.card_id().and_then(|id| v.db.get(id)) {
            if name != card.name {
                ui.label(RichText::new(&card.name).weak());
            }
            ui.label(RichText::new(&card.type_line).italics());
            if let Some(limit) = limit {
                ui.label(
                    RichText::new(limit.label())
                        .color(ban_color(limit))
                        .strong(),
                );
            }
            ui.label(&card.desc);
        }
        if marks.action != Action::None {
            ui.label(format!("{}: {}", tr("field.action"), marks.action.label()));
        }
        if marks.zone != Zone::None {
            ui.label(format!("{}: {}", tr("field.zone"), marks.zone.label()));
        }
        if marks.facing != Facing::Unspecified {
            ui.label(format!("{}: {}", tr("field.facing"), marks.facing.label()));
        }
    })
}

/// Karte mit Notiz darunter.
pub fn entry_view(
    ui: &mut egui::Ui,
    images: &mut ImageCache,
    v: View,
    entry: &Entry,
    width: f32,
    highlight: Option<Color32>,
) -> egui::Response {
    // Feste Größe anfordern, damit umbrechende Reihen vor der Karte umbrechen.
    let size = Vec2::new(width + 6.0, width * CARD_RATIO);
    ui.allocate_ui_with_layout(size, egui::Layout::top_down(egui::Align::Min), |ui| {
        ui.set_width(width + 6.0);
        let r = card_widget(
            ui,
            images,
            v,
            &entry.slot,
            Marks::of(entry),
            width,
            highlight,
        );
        if let Some(only) = &entry.only_with {
            let text = crate::i18n::trf("entry.only_with_short", &[&shown_name(v, only)]);
            ui.add(egui::Label::new(RichText::new(text).small().color(SELECT_COLOR)).wrap());
        }
        if !entry.note.is_empty() {
            ui.add(egui::Label::new(RichText::new(&entry.note).small()).wrap());
        }
        r
    })
    .inner
}

/// Zeigt einen Schritt (Karten mit Zeichen dazwischen). `colors` färbt
/// einzelne Karten ein.
pub fn step_row(
    ui: &mut egui::Ui,
    images: &mut ImageCache,
    v: View,
    step: &Step,
    width: f32,
    colors: &dyn Fn(usize) -> Option<Color32>,
) {
    ui.horizontal_wrapped(|ui| {
        for (pi, part) in step.parts.iter().enumerate() {
            if pi > 0 {
                ui.label(RichText::new(part.connector.symbol()).size(20.0));
            }
            entry_view(ui, images, v, &part.entry, width, colors(pi));
        }
    });
}

/// Großer „+“-Knopf in Kartengröße.
pub fn add_button(ui: &mut egui::Ui, width: f32, hover: &str) -> bool {
    ui.add_sized(
        Vec2::new(width * 0.8, width * CARD_RATIO),
        egui::Button::new(RichText::new("+").size(26.0)),
    )
    .on_hover_text(hover)
    .clicked()
}

/// Auswahl einer Aktion, gruppiert. Bei „Kettenglied“ erscheint ein Feld
/// für die Nummer. Gibt `true` zurück, wenn sich etwas geändert hat.
pub fn action_combo(ui: &mut egui::Ui, id: &str, action: &mut Action) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt(id)
        .selected_text(action.label())
        .width(220.0)
        .height(520.0)
        .show_ui(ui, |ui| {
            changed |= ui
                .selectable_value(action, Action::None, Action::None.label())
                .changed();
            for g in Action::GROUPS {
                ui.separator();
                ui.label(RichText::new(tr(g.key)).strong().color(SELECT_COLOR));
                for a in g.actions {
                    let selected = action.same_kind(*a);
                    if ui.selectable_label(selected, tr(a.key())).clicked() && !selected {
                        *action = *a;
                        changed = true;
                    }
                }
            }
        });
    if let Action::Chain(n) = action {
        ui.horizontal(|ui| {
            ui.label(tr("action.chain_number"));
            changed |= ui.add(egui::DragValue::new(n).range(1..=99)).changed();
        });
    }
    changed
}

/// Farbe einer Beschränkung: verboten rot, limitiert orange, halb-limitiert gelb.
pub fn ban_color(limit: crate::banlist::Limit) -> Color32 {
    use crate::banlist::Limit;
    match limit {
        Limit::Forbidden => Color32::from_rgb(214, 48, 49),
        Limit::Limited => Color32::from_rgb(235, 130, 30),
        Limit::SemiLimited => Color32::from_rgb(235, 200, 40),
    }
}

/// Runde Marke oben links auf der Karte: Verbotszeichen, 1 oder 2.
fn ban_badge(painter: &egui::Painter, rect: Rect, width: f32, limit: crate::banlist::Limit) {
    use crate::banlist::Limit;
    let r = (width / 6.5).clamp(7.0, 16.0);
    let c = rect.left_top() + Vec2::splat(r + 2.0);
    painter.circle_filled(c, r + 1.5, Color32::from_black_alpha(230));
    painter.circle_filled(c, r, ban_color(limit));
    match limit {
        Limit::Forbidden => {
            painter.circle_filled(c, r * 0.62, Color32::from_black_alpha(230));
            let d = Vec2::splat(r * 0.6);
            painter.line_segment([c - d, c + d], Stroke::new(r * 0.38, ban_color(limit)));
        }
        _ => {
            painter.text(
                c,
                Align2::CENTER_CENTER,
                limit.max_copies().to_string(),
                FontId::proportional(r * 1.5),
                Color32::BLACK,
            );
        }
    }
}
