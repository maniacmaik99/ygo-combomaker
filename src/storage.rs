//! Speicherorte und Laden/Speichern von Combos und Einstellungen.
//!
//! Linux:   ~/.local/share/ygo-combomaker (Combos), ~/.cache/ygo-combomaker (Karten, Bilder)
//! Windows: %APPDATA%\ygo-combomaker\data bzw. %LOCALAPPDATA%\ygo-combomaker\cache

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::model::{Combo, Deck};

#[derive(Clone)]
pub struct Paths {
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
}

impl Paths {
    pub fn detect() -> Result<Self> {
        let dirs = directories::ProjectDirs::from("", "", "ygo-combomaker")
            .context(crate::i18n::tr("err.no_user_dir"))?;
        let p = Paths {
            data_dir: dirs.data_dir().to_path_buf(),
            cache_dir: dirs.cache_dir().to_path_buf(),
        };
        std::fs::create_dir_all(&p.data_dir)?;
        std::fs::create_dir_all(p.image_dir())?;
        Ok(p)
    }

    pub fn combos_file(&self) -> PathBuf {
        self.data_dir.join("combos.json")
    }
    pub fn decks_file(&self) -> PathBuf {
        self.data_dir.join("decks.json")
    }
    pub fn settings_file(&self) -> PathBuf {
        self.data_dir.join("settings.json")
    }
    pub fn cards_file(&self) -> PathBuf {
        self.cache_dir.join("cards.json")
    }
    pub fn image_dir(&self) -> PathBuf {
        self.cache_dir.join("images")
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Sprache der Oberfläche. `None` = beim ersten Start aus dem System.
    pub lang: Option<crate::i18n::Lang>,
    /// Kartennamen in der Sprache der Oberfläche zeigen (falls vorhanden).
    pub localized_card_names: bool,
    /// In der Suche nur Karten zeigen, die es in Master Duel gibt.
    pub md_only: bool,
    /// Kartenbreite in Pixeln im Editor.
    pub card_width: f32,
    /// Beim Start automatisch nach Updates suchen.
    pub auto_update_check: bool,
    /// Spielfeld: freie Zone automatisch wählen statt anklicken.
    pub auto_zone: bool,
    /// Alt: GitHub-Token für Updates aus dem damals privaten Repo.
    /// Wird nicht mehr angezeigt, aber noch benutzt, falls vorhanden.
    pub github_token: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            lang: None,
            localized_card_names: false,
            md_only: true,
            card_width: 72.0,
            auto_update_check: true,
            auto_zone: false,
            github_token: String::new(),
        }
    }
}

/// Schreibt zuerst in eine temporäre Datei und benennt sie dann um.
/// So bleibt bei einem Absturz die alte Datei erhalten.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn load_json<T: for<'de> Deserialize<'de> + Default>(path: &Path) -> Result<T> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .with_context(|| crate::i18n::trf("err.file_corrupt", &[&path.display()])),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(e.into()),
    }
}

pub fn load_combos(paths: &Paths) -> Result<Vec<Combo>> {
    load_json(&paths.combos_file())
}

pub fn save_combos(paths: &Paths, combos: &[Combo]) -> Result<()> {
    write_atomic(&paths.combos_file(), &serde_json::to_vec_pretty(combos)?)
}

pub fn load_decks(paths: &Paths) -> Result<Vec<Deck>> {
    load_json(&paths.decks_file())
}

pub fn save_decks(paths: &Paths, decks: &[Deck]) -> Result<()> {
    write_atomic(&paths.decks_file(), &serde_json::to_vec_pretty(decks)?)
}

pub fn load_settings(paths: &Paths) -> Settings {
    load_json(&paths.settings_file()).unwrap_or_default()
}

pub fn save_settings(paths: &Paths, settings: &Settings) -> Result<()> {
    write_atomic(
        &paths.settings_file(),
        &serde_json::to_vec_pretty(settings)?,
    )
}

/// Eine einzelne Combo als JSON-Text (zum Teilen per Zwischenablage).
pub fn combo_to_share_text(combo: &Combo) -> Result<String> {
    Ok(serde_json::to_string_pretty(combo)?)
}

/// Liest eine geteilte Combo ein und gibt ihr eine neue ID,
/// damit sie keine vorhandene Combo überschreibt.
pub fn combo_from_share_text(text: &str) -> Result<Combo> {
    let mut combo: Combo =
        serde_json::from_str(text.trim()).context(crate::i18n::tr("err.invalid_combo_text"))?;
    combo.id = uuid::Uuid::new_v4();
    combo.practice = Default::default();
    Ok(combo)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_roundtrip_gets_new_id() {
        let c = Combo::new("Test");
        let text = combo_to_share_text(&c).unwrap();
        let back = combo_from_share_text(&text).unwrap();
        assert_eq!(back.title, "Test");
        assert_ne!(back.id, c.id);
    }

    #[test]
    fn atomic_write_and_missing_file() {
        let dir = std::env::temp_dir().join(format!("ygo-test-{}", uuid::Uuid::new_v4()));
        let paths = Paths {
            data_dir: dir.clone(),
            cache_dir: dir.clone(),
        };
        assert!(load_combos(&paths).unwrap().is_empty());
        save_combos(&paths, &[Combo::new("A")]).unwrap();
        assert_eq!(load_combos(&paths).unwrap()[0].title, "A");
        std::fs::remove_dir_all(dir).ok();
    }
}
