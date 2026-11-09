//! App-weites Farbschema (dunkles Violett/Navy mit Gold-Akzenten).
//!
//! `apply` einmal beim Start aufrufen (vor dem Erzeugen der App).
//! Die `pub const`-Farben können überall in der UI verwendet werden.

use egui::{Color32, CornerRadius, FontId, Margin, Shadow, Stroke, TextStyle, Vec2};

/// Hauptakzent (Violett): Auswahl, Fokus, Hover-Rahmen.
pub const ACCENT: Color32 = Color32::from_rgb(124, 92, 255);
/// Zweiter Akzent (Gold/Bernstein): aktive/ausgewählte Elemente, Hervorhebungen.
pub const ACCENT_GOLD: Color32 = Color32::from_rgb(242, 184, 75);
/// Hintergrund von Panels (Seitenleisten, Hauptfläche).
pub const PANEL_BG: Color32 = Color32::from_rgb(21, 18, 42);
/// Etwas hellerer Hintergrund für Karten/Kacheln/Gruppen.
pub const CARD_BG: Color32 = Color32::from_rgb(33, 28, 62);
/// Erfolg / richtig.
#[allow(dead_code)] // für die UI bereitgestellt, evtl. noch nicht überall genutzt
pub const GOOD: Color32 = Color32::from_rgb(76, 209, 138);
/// Fehler / falsch.
pub const BAD: Color32 = Color32::from_rgb(255, 92, 108);
/// Gedämpfte Schrift für Nebeninfos.
pub const TEXT_WEAK: Color32 = Color32::from_rgb(150, 144, 182);

// Interne Abstufungen.
const WINDOW_BG: Color32 = Color32::from_rgb(27, 23, 52);
const EXTREME_BG: Color32 = Color32::from_rgb(13, 11, 27);
const FAINT_BG: Color32 = Color32::from_rgb(26, 22, 50);
const BORDER: Color32 = Color32::from_rgb(52, 45, 94);
const WIDGET_BG: Color32 = Color32::from_rgb(42, 36, 80);
const WIDGET_BG_STRONG: Color32 = Color32::from_rgb(52, 44, 96);
const HOVER_BG: Color32 = Color32::from_rgb(58, 49, 112);
const ACCENT_DEEP: Color32 = Color32::from_rgb(75, 52, 184);
const TEXT: Color32 = Color32::from_rgb(208, 204, 230);
const TEXT_STRONG: Color32 = Color32::from_rgb(240, 238, 250);

const RADIUS: u8 = 6;

/// Setzt das Theme für den gegebenen Kontext (dunkel, unabhängig vom System-Theme).
pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(egui::ThemePreference::Dark);
    // Für beide Themes setzen, damit ein späterer Theme-Wechsel nichts zurücksetzt.
    ctx.all_styles_mut(|style| {
        style.visuals = visuals();
        style_spacing(&mut style.spacing);
        style.text_styles = [
            (TextStyle::Small, FontId::proportional(11.5)),
            (TextStyle::Body, FontId::proportional(15.0)),
            (TextStyle::Button, FontId::proportional(15.0)),
            (TextStyle::Heading, FontId::proportional(22.0)),
            (TextStyle::Monospace, FontId::monospace(14.0)),
        ]
        .into();
        style.animation_time = 0.12;
    });
}

fn style_spacing(s: &mut egui::style::Spacing) {
    s.item_spacing = Vec2::new(8.0, 6.0);
    s.button_padding = Vec2::new(10.0, 5.0);
    s.window_margin = Margin::same(12);
    s.menu_margin = Margin::same(8);
    s.indent = 20.0;
    s.interact_size = Vec2::new(40.0, 26.0);
    s.combo_height = 260.0;
    s.icon_width = 16.0;
    s.icon_width_inner = 9.0;
    s.icon_spacing = 6.0;
    s.scroll.bar_width = 8.0;
}

fn visuals() -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    let r = CornerRadius::same(RADIUS);

    v.panel_fill = PANEL_BG;
    v.window_fill = WINDOW_BG;
    v.window_stroke = Stroke::new(1.0, BORDER);
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(8);
    v.window_shadow = Shadow {
        offset: [0, 8],
        blur: 24,
        spread: 0,
        color: Color32::from_black_alpha(120),
    };
    v.popup_shadow = Shadow {
        offset: [0, 4],
        blur: 12,
        spread: 0,
        color: Color32::from_black_alpha(110),
    };

    v.faint_bg_color = FAINT_BG;
    v.extreme_bg_color = EXTREME_BG;
    v.code_bg_color = WIDGET_BG;
    v.hyperlink_color = Color32::from_rgb(170, 150, 255);
    v.warn_fg_color = ACCENT_GOLD;
    v.error_fg_color = BAD;
    v.weak_text_color = Some(TEXT_WEAK);
    v.slider_trailing_fill = true;

    v.selection.bg_fill = ACCENT_DEEP;
    v.selection.stroke = Stroke::new(1.0, ACCENT_GOLD);
    v.text_cursor.stroke = Stroke::new(2.0, ACCENT_GOLD);

    let w = &mut v.widgets;
    // Labels, Separatoren, Rahmen nicht-interaktiver Elemente.
    w.noninteractive.bg_fill = PANEL_BG;
    w.noninteractive.weak_bg_fill = CARD_BG;
    w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    w.noninteractive.corner_radius = r;

    // Buttons, Checkboxen usw. im Ruhezustand.
    w.inactive.weak_bg_fill = WIDGET_BG;
    w.inactive.bg_fill = WIDGET_BG_STRONG;
    w.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(60, 52, 108));
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT_STRONG);
    w.inactive.corner_radius = r;

    // Maus darüber.
    w.hovered.weak_bg_fill = HOVER_BG;
    w.hovered.bg_fill = HOVER_BG;
    w.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    w.hovered.fg_stroke = Stroke::new(1.5, Color32::WHITE);
    w.hovered.corner_radius = r;
    w.hovered.expansion = 1.0;

    // Gedrückt / gezogen.
    w.active.weak_bg_fill = ACCENT_DEEP;
    w.active.bg_fill = ACCENT_DEEP;
    w.active.bg_stroke = Stroke::new(1.5, ACCENT_GOLD);
    w.active.fg_stroke = Stroke::new(2.0, Color32::WHITE);
    w.active.corner_radius = r;
    w.active.expansion = 1.0;

    // Geöffnete Combo-Boxen, Menüs.
    w.open.weak_bg_fill = WIDGET_BG_STRONG;
    w.open.bg_fill = WINDOW_BG;
    w.open.bg_stroke = Stroke::new(1.0, ACCENT);
    w.open.fg_stroke = Stroke::new(1.0, TEXT_STRONG);
    w.open.corner_radius = r;

    v
}
