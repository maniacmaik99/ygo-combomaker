//! Kartenbilder: Werden einmal von YGOPRODeck geladen, auf der Festplatte
//! gespeichert und danach nur noch lokal gelesen. Zwei Hintergrund-Threads
//! laden die Bilder, damit die Oberfläche nicht hängt.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use egui::{ColorImage, TextureHandle, TextureOptions};

const IMAGE_URL: &str = "https://images.ygoprodeck.com/images/cards_small";
/// Pause nach jedem Download pro Thread. 2 Threads × 10/s = max. 20 Bilder/s.
const DOWNLOAD_PAUSE: Duration = Duration::from_millis(100);
const WORKERS: usize = 2;

enum State {
    Loading,
    Ready(TextureHandle),
    Failed,
}

pub struct ImageCache {
    states: HashMap<u32, State>,
    request_tx: Sender<u32>,
    done_rx: Receiver<(u32, Result<ColorImage>)>,
}

impl ImageCache {
    pub fn new(ctx: egui::Context, image_dir: PathBuf) -> Self {
        let (request_tx, request_rx) = channel::<u32>();
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
                    format!("card-{id}"),
                    img,
                    TextureOptions::LINEAR,
                )),
                Err(_) => State::Failed,
            };
            self.states.insert(id, state);
        }
    }

    /// Gibt das Bild zurück, falls es fertig ist. Sonst wird es angefordert.
    pub fn get(&mut self, image_id: u32) -> Option<&TextureHandle> {
        if let std::collections::hash_map::Entry::Vacant(e) = self.states.entry(image_id) {
            e.insert(State::Loading);
            let _ = self.request_tx.send(image_id);
            return None;
        }
        match self.states.get(&image_id) {
            Some(State::Ready(t)) => Some(t),
            _ => None,
        }
    }

    pub fn failed(&self, image_id: u32) -> bool {
        matches!(self.states.get(&image_id), Some(State::Failed))
    }

    /// Fehlgeschlagene Bilder beim nächsten Anzeigen erneut versuchen.
    pub fn retry_failed(&mut self) {
        self.states.retain(|_, s| !matches!(s, State::Failed));
    }
}

fn load_image(dir: &Path, id: u32) -> Result<ColorImage> {
    let path = dir.join(format!("{id}.jpg"));
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(_) => {
            let bytes = download(id)?;
            crate::storage::write_atomic(&path, &bytes)?;
            std::thread::sleep(DOWNLOAD_PAUSE);
            bytes
        }
    };
    let img = image::load_from_memory(&bytes)?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    Ok(ColorImage::from_rgba_unmultiplied(size, img.as_raw()))
}

fn download(id: u32) -> Result<Vec<u8>> {
    let mut resp = ureq::get(&format!("{IMAGE_URL}/{id}.jpg"))
        .header("User-Agent", crate::cards::USER_AGENT)
        .call()?;
    Ok(resp
        .body_mut()
        .with_config()
        .limit(5 * 1024 * 1024)
        .read_to_vec()?)
}
