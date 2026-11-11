//! YGO Combomaker – Combos eintragen und üben.
//! Kartendaten und Bilder: YGOPRODeck-API (https://ygoprodeck.com/api-guide/).

// Unter Windows kein zusätzliches Konsolenfenster im Release-Build.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod board;
mod cards;
mod combotext;
mod decks;
mod i18n;
mod images;
mod model;
mod practice;
mod storage;
mod theme;
mod updater;

/// App-Icon (PNG, 256x256), in die Binärdatei eingebettet.
const ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");

fn load_icon() -> Option<egui::IconData> {
    let img = image::load_from_memory(ICON_PNG).ok()?.into_rgba8();
    let (width, height) = img.dimensions();
    Some(egui::IconData {
        rgba: img.into_raw(),
        width,
        height,
    })
}

fn main() -> eframe::Result {
    let paths = match storage::Paths::detect() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Fehler: {e:#}");
            std::process::exit(1);
        }
    };
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("YGO Combomaker")
        // Unter Wayland ordnet der Compositor das Fenster über die App-ID
        // der Desktop-Datei (ygo-combomaker.desktop) und deren Icon zu.
        .with_app_id("ygo-combomaker")
        .with_inner_size([1280.0, 820.0])
        .with_min_inner_size([800.0, 500.0]);
    if let Some(icon) = load_icon() {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "YGO Combomaker",
        options,
        Box::new(|cc| {
            theme::apply(&cc.egui_ctx);
            Ok(Box::new(app::ComboApp::new(cc, paths)))
        }),
    )
}
