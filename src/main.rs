//! YGO Combomaker – Combos eintragen und üben.
//! Kartendaten und Bilder: YGOPRODeck-API (https://ygoprodeck.com/api-guide/).

// Unter Windows kein zusätzliches Konsolenfenster im Release-Build.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod cards;
mod images;
mod model;
mod practice;
mod storage;
mod updater;

fn main() -> eframe::Result {
    let paths = match storage::Paths::detect() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Fehler: {e:#}");
            std::process::exit(1);
        }
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("YGO Combomaker")
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([800.0, 500.0]),
        ..Default::default()
    };
    eframe::run_native(
        "YGO Combomaker",
        options,
        Box::new(|cc| Ok(Box::new(app::ComboApp::new(cc, paths)))),
    )
}
