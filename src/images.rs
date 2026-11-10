//! Kartenbilder: Werden einmal von YGOPRODeck geladen, auf der Festplatte
//! gespeichert und danach nur noch lokal gelesen. Zwei Hintergrund-Threads
//! laden die Bilder, damit die Oberfläche nicht hängt.
//!
//! Es gibt zwei Größen: das kleine Bild (reicht für normale Kartengrößen)
//! und das große Bild, das nur geladen wird, wenn eine Karte groß
//! angezeigt wird. Bis das große Bild da ist, wird das kleine gezeigt.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use egui::{ColorImage, TextureHandle, TextureOptions};

const SMALL_URL: &str = "https://images.ygoprodeck.com/images/cards_small";
const LARGE_URL: &str = "https://images.ygoprodeck.com/images/cards";
/// Ab dieser Breite in Bildschirm-Pixeln wird das große Bild benutzt.
/// Das kleine Bild ist 168 Pixel breit.
const LARGE_FROM_PX: f32 = 176.0;
/// Große Bilder werden auf diese Breite verkleinert, bevor sie in den
/// Grafikspeicher kommen (das Original ist rund 810 Pixel breit).
const LARGE_TEXTURE_WIDTH: u32 = 480;

/// (Bild-ID, großes Bild?)
type Key = (u32, bool);
/// Pause nach jedem Download pro Thread. 2 Threads × 10/s = max. 20 Bilder/s.
const DOWNLOAD_PAUSE: Duration = Duration::from_millis(100);
const WORKERS: usize = 2;

enum State {
    Loading,
    Ready(TextureHandle),
    Failed,
}

pub struct ImageCache {
    states: HashMap<Key, State>,
    request_tx: Sender<Key>,
    done_rx: Receiver<(Key, Result<ColorImage>)>,
}

impl ImageCache {
    pub fn new(ctx: egui::Context, image_dir: PathBuf) -> Self {
        let (request_tx, request_rx) = channel::<Key>();
        let (done_tx, done_rx) = channel();
        let request_rx = Arc::new(Mutex::new(request_rx));
        for _ in 0..WORKERS {
            let rx = Arc::clone(&request_rx);
            let tx = done_tx.clone();
            let dir = image_dir.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                loop {
                    let id = match rx.lock().map(|r| r.recv()) {
                        Ok(Ok(id)) => id,
                        _ => return,
                    };
                    let result = load_image(&dir, id);
                    if tx.send((id, result)).is_err() {
                        return;
                    }
                    ctx.request_repaint();
                }
            });
        }
        Self {
            states: HashMap::new(),
            request_tx,
            done_rx,
        }
    }

    /// Übernimmt fertig geladene Bilder. Einmal pro Frame aufrufen.
    pub fn poll(&mut self, ctx: &egui::Context) {
        while let Ok((id, result)) = self.done_rx.try_recv() {
            let state = match result {
                Ok(img) => State::Ready(ctx.load_texture(
                    format!("card-{}-{}", id.0, if id.1 { "l" } else { "s" }),
                    img,
                    TextureOptions::LINEAR,
                )),
                Err(_) => State::Failed,
            };
            self.states.insert(id, state);
        }
    }

    /// Fordert das Bild an, falls das noch nicht geschehen ist.
    fn request(&mut self, key: Key) {
        if let std::collections::hash_map::Entry::Vacant(e) = self.states.entry(key) {
            e.insert(State::Loading);
            let _ = self.request_tx.send(key);
        }
    }

    fn ready(&self, key: Key) -> Option<&TextureHandle> {
        match self.states.get(&key) {
            Some(State::Ready(t)) => Some(t),
            _ => None,
        }
    }

    /// Gibt das kleine Bild zurück, falls es fertig ist. Sonst wird es angefordert.
    pub fn get(&mut self, image_id: u32) -> Option<&TextureHandle> {
        self.request((image_id, false));
        self.ready((image_id, false))
    }

    /// Gibt das passende Bild für die Anzeigebreite `px` (Bildschirm-Pixel)
    /// zurück: bei großen Karten das große Bild. Solange es noch lädt oder
    /// nicht verfügbar ist, das kleine.
    pub fn get_for_width(&mut self, image_id: u32, px: f32) -> Option<&TextureHandle> {
        if px >= LARGE_FROM_PX {
            self.request((image_id, true));
            if self.ready((image_id, true)).is_some() {
                return self.ready((image_id, true));
            }
        }
        self.get(image_id)
    }

    pub fn failed(&self, image_id: u32) -> bool {
        matches!(self.states.get(&(image_id, false)), Some(State::Failed))
    }

    /// Fehlgeschlagene Bilder beim nächsten Anzeigen erneut versuchen.
    pub fn retry_failed(&mut self) {
        self.states.retain(|_, s| !matches!(s, State::Failed));
    }
}

fn load_image(dir: &Path, (id, large): Key) -> Result<ColorImage> {
    let path = if large {
        dir.join("large").join(format!("{id}.jpg"))
    } else {
        dir.join(format!("{id}.jpg"))
    };
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(_) => {
            let bytes = download(id, large)?;
            crate::storage::write_atomic(&path, &bytes)?;
            std::thread::sleep(DOWNLOAD_PAUSE);
            bytes
        }
    };
    let mut img = image::load_from_memory(&bytes)?;
    if large && img.width() > LARGE_TEXTURE_WIDTH {
        let h = (img.height() as u64 * LARGE_TEXTURE_WIDTH as u64 / img.width() as u64) as u32;
        img = img.resize_exact(
            LARGE_TEXTURE_WIDTH,
            h.max(1),
            image::imageops::FilterType::Triangle,
        );
    }
    let img = img.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    Ok(ColorImage::from_rgba_unmultiplied(size, img.as_raw()))
}

fn download(id: u32, large: bool) -> Result<Vec<u8>> {
    let base = if large { LARGE_URL } else { SMALL_URL };
    let mut resp = ureq::get(&format!("{base}/{id}.jpg"))
        .header("User-Agent", crate::cards::USER_AGENT)
        .call()?;
    Ok(resp
        .body_mut()
        .with_config()
        .limit(8 * 1024 * 1024)
        .read_to_vec()?)
}
