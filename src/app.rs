//! Oberfläche (egui). Aufbau:
//! - oben: Leiste mit Ansicht (Bearbeiten/Üben), Datenbank-Status, Einstellungen
//! - links: Liste der Combos
//! - Mitte: Editor oder Übungsmodus
//! - Fenster: Kartensuche, Import, Löschen bestätigen

use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use egui::{
    Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, TextEdit, Vec2,
};

use crate::cards::{self, CardDb, CardDbFile};
use crate::images::ImageCache;
use crate::model::{Action, CardKind, Combo, Connector, Entry, Section, Slot, Step, StepPart};
use crate::practice::Session;
use crate::storage::{self, Paths, Settings};
use crate::updater::{self, UpdateInfo};

const CARD_RATIO: f32 = 86.0 / 59.0;
const SELECT_COLOR: Color32 = Color32::from_rgb(90, 160, 255);
const GOOD_COLOR: Color32 = Color32::from_rgb(70, 200, 90);
const BAD_COLOR: Color32 = Color32::from_rgb(230, 70, 70);

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Edit,
    Practice,
}

/// Wohin eine Karte aus der Suche hinzugefügt wird.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    Section(Section),
    Step(usize),
    PracticeGuess,
}

/// Welche Karte im Editor ausgewählt ist.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Sel {
    Section(Section, usize),
    StepPart(usize, usize),
}

struct Picker {
    target: Target,
    query: String,
    placeholder_kind: CardKind,
    placeholder_label: String,
    focus_requested: bool,
}

/// Zustand der Update-Funktion.
enum UpdateState {
    Idle,
    Checking,
    UpToDate,
    Available(UpdateInfo),
    Installing(String),
    Installed(std::path::PathBuf, String),
    Failed(String),
}

enum UpdateMsg {
    Checked(Result<Option<UpdateInfo>, String>),
    Installed(Result<std::path::PathBuf, String>),
}

enum DbMsg {
    Progress(String),
    Loaded(CardDbFile),
    UpToDate,
    Failed(String),
}

pub struct ComboApp {
    paths: Paths,
    settings: Settings,
    saved_settings: Settings,
    combos: Vec<Combo>,
    selected: Option<usize>,
    view: View,
    db: CardDb,
    db_status: String,
    db_rx: Option<Receiver<DbMsg>>,
    images: ImageCache,
    picker: Option<Picker>,
    selection: Option<Sel>,
    session: Option<Session>,
    dirty: bool,
    last_change: Instant,
    import_text: Option<String>,
    confirm_delete: Option<usize>,
    message: Option<String>,
    update: UpdateState,
    update_rx: Option<Receiver<UpdateMsg>>,
    /// Automatische Prüfung beim Start: nur melden, wenn es ein Update gibt.
    update_silent: bool,
}

impl ComboApp {
    pub fn new(cc: &eframe::CreationContext<'_>, paths: Paths) -> Self {
        let settings = storage::load_settings(&paths);
        let (combos, message) = match storage::load_combos(&paths) {
            Ok(c) => (c, None),
            Err(e) => (
                Vec::new(),
                Some(format!("Combos konnten nicht geladen werden: {e:#}")),
            ),
        };
        let images = ImageCache::new(cc.egui_ctx.clone(), paths.image_dir());
        let mut app = Self {
            paths,
            saved_settings: settings.clone(),
            settings,
            selected: if combos.is_empty() { None } else { Some(0) },
            combos,
            view: View::Edit,
            db: CardDb::default(),
            db_status: String::new(),
            db_rx: None,
            images,
            picker: None,
            selection: None,
            session: None,
            dirty: false,
            last_change: Instant::now(),
            import_text: None,
            confirm_delete: None,
            message,
            update: UpdateState::Idle,
            update_rx: None,
            update_silent: false,
        };
        app.start_db_job(cc.egui_ctx.clone(), false);
        if app.settings.auto_update_check {
            app.start_update_check(cc.egui_ctx.clone(), true);
        }
        app
    }

    // -----------------------------------------------------------------------
    // Kartendatenbank im Hintergrund
    // -----------------------------------------------------------------------

    /// `check_online = false`: lokale Datei laden, nur bei Fehlen herunterladen.
    /// `check_online = true`: Version online prüfen und bei Bedarf neu laden.
    fn start_db_job(&mut self, ctx: egui::Context, check_online: bool) {
        let (tx, rx) = channel();
        self.db_rx = Some(rx);
        let path = self.paths.cards_file();
        let current_version = self.db.version.clone();
        let have_cards = !self.db.is_empty();
        std::thread::spawn(move || {
            let send = |m: DbMsg| {
                let _ = tx.send(m);
                ctx.request_repaint();
            };
            if !check_online {
                send(DbMsg::Progress(
                    "Lade Kartendatenbank von der Festplatte …".into(),
                ));
                if let Ok(file) = cards::load_from_disk(&path) {
                    send(DbMsg::Loaded(file));
                    return;
                }
            } else if have_cards {
                send(DbMsg::Progress("Prüfe, ob es neue Karten gibt …".into()));
                match cards::fetch_db_version() {
                    Ok(v) if !v.is_empty() && v == current_version => {
                        send(DbMsg::UpToDate);
                        return;
                    }
                    Ok(_) => {}
                    Err(e) => {
                        send(DbMsg::Failed(format!("{e:#}")));
                        return;
                    }
                }
            }
            let progress = |s: &str| send(DbMsg::Progress(s.to_string()));
            match cards::fetch_full_db(&progress) {
                Ok(file) => {
                    progress("Speichere Kartendatenbank …");
                    if let Err(e) = cards::save_to_disk(&path, &file) {
                        send(DbMsg::Failed(format!("Speichern fehlgeschlagen: {e:#}")));
                    }
                    send(DbMsg::Loaded(file));
                }
                Err(e) => send(DbMsg::Failed(format!("{e:#}"))),
            }
        });
    }

    fn poll_db(&mut self) {
        let Some(rx) = &self.db_rx else { return };
        let mut finished = false;
        while let Ok(msg) = rx.try_recv() {
            match msg {
                DbMsg::Progress(s) => self.db_status = s,
                DbMsg::Loaded(file) => {
                    self.db = CardDb::from_file(file);
                    self.db_status = format!("{} Karten geladen", self.db.cards.len());
                    finished = true;
                }
                DbMsg::UpToDate => {
                    self.db_status =
                        format!("{} Karten, Datenbank ist aktuell", self.db.cards.len());
                    finished = true;
                }
                DbMsg::Failed(e) => {
                    self.db_status = "Kartendatenbank: Fehler (siehe Meldung)".into();
                    self.message = Some(format!(
                        "Kartendatenbank konnte nicht geladen werden. Prüfe die Internetverbindung.\n\n{e}"
                    ));
                    finished = true;
                }
            }
        }
        if finished {
            self.db_rx = None;
        }
    }

    // -----------------------------------------------------------------------
    // Updates (GitHub Releases)
    // -----------------------------------------------------------------------

    fn start_update_check(&mut self, ctx: egui::Context, silent: bool) {
        let (tx, rx) = channel();
        self.update_rx = Some(rx);
        self.update = UpdateState::Checking;
        self.update_silent = silent;
        let token = self.settings.github_token.clone();
        std::thread::spawn(move || {
            let r = updater::check(&token).map_err(|e| format!("{e:#}"));
            let _ = tx.send(UpdateMsg::Checked(r));
            ctx.request_repaint();
        });
    }

    fn start_update_install(&mut self, ctx: egui::Context, info: UpdateInfo) {
        let (tx, rx) = channel();
        self.update_rx = Some(rx);
        self.update = UpdateState::Installing(info.version.clone());
        self.update_silent = false;
        let token = self.settings.github_token.clone();
        std::thread::spawn(move || {
            let r = updater::install(&info, &token).map_err(|e| format!("{e:#}"));
            let _ = tx.send(UpdateMsg::Installed(r));
            ctx.request_repaint();
        });
    }

    fn poll_update(&mut self) {
        let Some(rx) = &self.update_rx else { return };
        let Ok(msg) = rx.try_recv() else { return };
        self.update_rx = None;
        let silent = self.update_silent;
        self.update = match msg {
            UpdateMsg::Checked(Ok(Some(info))) => UpdateState::Available(info),
            UpdateMsg::Checked(Ok(None)) if silent => UpdateState::Idle,
            UpdateMsg::Checked(Ok(None)) => UpdateState::UpToDate,
            UpdateMsg::Checked(Err(_)) if silent => UpdateState::Idle,
            UpdateMsg::Checked(Err(e)) => UpdateState::Failed(e),
            UpdateMsg::Installed(Ok(exe)) => {
                let v = match &self.update {
                    UpdateState::Installing(v) => v.clone(),
                    _ => String::new(),
                };
                UpdateState::Installed(exe, v)
            }
            UpdateMsg::Installed(Err(e)) => UpdateState::Failed(e),
        };
    }

    fn update_window(&mut self, ctx: &egui::Context) {
        if matches!(self.update, UpdateState::Idle)
            || (self.update_silent && matches!(self.update, UpdateState::Checking))
        {
            return;
        }
        let mut close = false;
        let mut install = None;
        let mut restart = None;
        egui::Window::new("Update")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_TOP, [0.0, 60.0])
            .show(ctx, |ui| {
                ui.label(format!(
                    "Installierte Version: {}",
                    updater::CURRENT_VERSION
                ));
                ui.add_space(4.0);
                match &self.update {
                    UpdateState::Idle => {}
                    UpdateState::Checking => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Suche nach Updates …");
                        });
                    }
                    UpdateState::UpToDate => {
                        ui.label("Du hast die neueste Version.");
                        close = ui.button("OK").clicked();
                    }
                    UpdateState::Available(info) => {
                        ui.label(
                            RichText::new(format!("Version {} ist verfügbar.", info.version))
                                .strong(),
                        );
                        if !info.notes.trim().is_empty() {
                            egui::ScrollArea::vertical()
                                .max_height(200.0)
                                .show(ui, |ui| {
                                    ui.label(&info.notes);
                                });
                        }
                        ui.horizontal(|ui| {
                            if ui.button("Jetzt aktualisieren").clicked() {
                                install = Some(info.clone());
                            }
                            close = ui.button("Später").clicked();
                        });
                    }
                    UpdateState::Installing(v) => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(format!("Lade Version {v} herunter und installiere sie …"));
                        });
                    }
                    UpdateState::Installed(exe, v) => {
                        ui.label(format!(
                            "Version {v} ist installiert. Die App startet jetzt neu."
                        ));
                        ui.label("Deine Combos sind gespeichert.");
                        if ui.button("Neu starten").clicked() {
                            restart = Some(exe.clone());
                        }
                    }
                    UpdateState::Failed(e) => {
                        ui.label(RichText::new("Update fehlgeschlagen:").color(BAD_COLOR));
                        ui.label(e);
                        close = ui.button("OK").clicked();
                    }
                }
            });
        if close {
            self.update = UpdateState::Idle;
        }
        if let Some(info) = install {
            self.start_update_install(ctx.clone(), info);
        }
        if let Some(exe) = restart {
            self.save_now();
            let _ = storage::save_settings(&self.paths, &self.settings);
            updater::restart(&exe);
        }
    }

    fn db_busy(&self) -> bool {
        self.db_rx.is_some()
    }

    // -----------------------------------------------------------------------
    // Speichern
    // -----------------------------------------------------------------------

    fn mark_dirty(&mut self) {
        self.dirty = true;
        self.last_change = Instant::now();
    }

    fn autosave(&mut self, ctx: &egui::Context) {
        if self.dirty {
            if self.last_change.elapsed() >= Duration::from_millis(700) {
                self.save_now();
            } else {
                ctx.request_repaint_after(Duration::from_millis(750));
            }
        }
        if self.settings != self.saved_settings {
            let _ = storage::save_settings(&self.paths, &self.settings);
            self.saved_settings = self.settings.clone();
        }
    }

    fn save_now(&mut self) {
        match storage::save_combos(&self.paths, &self.combos) {
            Ok(()) => self.dirty = false,
            Err(e) => self.message = Some(format!("Speichern fehlgeschlagen: {e:#}")),
        }
    }

    // -----------------------------------------------------------------------
    // Aktionen
    // -----------------------------------------------------------------------

    fn select_combo(&mut self, idx: Option<usize>) {
        self.selected = idx;
        self.selection = None;
        self.picker = None;
        self.session = None;
    }

    fn open_picker(&mut self, target: Target) {
        self.picker = Some(Picker {
            target,
            query: String::new(),
            placeholder_kind: CardKind::Monster,
            placeholder_label: "Any Card".into(),
            focus_requested: false,
        });
    }

    fn add_to_target(&mut self, target: Target, slot: Slot) {
        if target == Target::PracticeGuess {
            if let Some(s) = &mut self.session {
                s.guesses.push(slot);
            }
            return;
        }
        let Some(idx) = self.selected else { return };
        let combo = &mut self.combos[idx];
        match target {
            Target::Section(sec) => sec.entries_mut(combo).push(Entry::new(slot)),
            Target::Step(si) => {
                if let Some(step) = combo.steps.get_mut(si) {
                    step.parts.push(StepPart {
                        connector: Connector::Then,
                        entry: Entry::new(slot),
                    });
                }
            }
            Target::PracticeGuess => {}
        }
        self.mark_dirty();
    }
}

impl Drop for ComboApp {
    fn drop(&mut self) {
        if self.dirty {
            let _ = storage::save_combos(&self.paths, &self.combos);
        }
    }
}

// ---------------------------------------------------------------------------
// Zeichnen von Karten (freie Funktionen, damit der Borrow-Checker mitspielt)
// ---------------------------------------------------------------------------

fn kind_color(kind: CardKind) -> Color32 {
    match kind {
        CardKind::Monster => Color32::from_rgb(190, 115, 40),
        CardKind::Spell => Color32::from_rgb(25, 145, 125),
        CardKind::Trap => Color32::from_rgb(175, 45, 125),
    }
}

fn shown_name(db: &CardDb, german: bool, slot: &Slot) -> String {
    if german
        && let Some(de) = slot
            .card_id()
            .and_then(|id| db.get(id))
            .and_then(|c| c.name_de.clone())
    {
        return de;
    }
    slot.display_name().to_string()
}

/// Zeichnet eine Karte (Bild oder Platzhalter) mit Aktions-Symbol.
#[allow(clippy::too_many_arguments)]
fn card_widget(
    ui: &mut egui::Ui,
    images: &mut ImageCache,
    db: &CardDb,
    german: bool,
    slot: &Slot,
    action: Action,
    width: f32,
    highlight: Option<Color32>,
) -> egui::Response {
    let size = Vec2::new(width, width * CARD_RATIO);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let small = FontId::proportional((width / 6.0).clamp(9.0, 14.0));
        let draw_text = |text: &str, fill: Color32| {
            painter.rect_filled(rect, 4.0, fill);
            let galley =
                painter.layout(text.to_string(), small.clone(), Color32::WHITE, width - 8.0);
            let pos = rect.center() - galley.size() / 2.0;
            painter.galley(pos, galley, Color32::WHITE);
        };
        match slot {
            Slot::Card { image_id, name, .. } => {
                if let Some(tex) = images.get(*image_id) {
                    let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
                    painter.image(tex.id(), rect, uv, Color32::WHITE);
                } else if images.failed(*image_id) {
                    draw_text(name, Color32::from_gray(55));
                } else {
                    draw_text("…", Color32::from_gray(40));
                }
            }
            Slot::Placeholder { kind, label } => draw_text(label, kind_color(*kind)),
        }
        if let Some(badge) = action.badge() {
            let r = (width / 7.0).clamp(9.0, 14.0);
            let c = rect.left_bottom() + Vec2::new(r + 2.0, -r - 2.0);
            painter.circle(
                c,
                r,
                Color32::from_black_alpha(220),
                Stroke::new(1.5, Color32::WHITE),
            );
            painter.text(
                c,
                Align2::CENTER_CENTER,
                badge,
                FontId::proportional(r * 0.95),
                Color32::WHITE,
            );
        }
        if let Some(color) = highlight {
            painter.rect_stroke(rect, 4.0, Stroke::new(3.0, color), StrokeKind::Outside);
        }
    }
    let name = shown_name(db, german, slot);
    resp.on_hover_ui(|ui| {
        ui.set_max_width(320.0);
        ui.label(RichText::new(&name).strong());
        if let Some(card) = slot.card_id().and_then(|id| db.get(id)) {
            if german && card.name_de.is_some() {
                ui.label(RichText::new(&card.name).weak());
            }
            ui.label(RichText::new(&card.type_line).italics());
            ui.label(&card.desc);
        }
        if action != Action::None {
            ui.label(format!("Aktion: {}", action.label()));
        }
    })
}

/// Karte mit Notiz darunter.
fn entry_view(
    ui: &mut egui::Ui,
    images: &mut ImageCache,
    db: &CardDb,
    german: bool,
    entry: &Entry,
    width: f32,
    highlight: Option<Color32>,
) -> egui::Response {
    ui.vertical(|ui| {
        ui.set_width(width + 6.0);
        let r = card_widget(
            ui,
            images,
            db,
            german,
            &entry.slot,
            entry.action,
            width,
            highlight,
        );
        if !entry.note.is_empty() {
            ui.add(egui::Label::new(RichText::new(&entry.note).small()).wrap());
        }
        r
    })
    .inner
}

// ---------------------------------------------------------------------------
// Hauptschleife
// ---------------------------------------------------------------------------

impl eframe::App for ComboApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_db();
        self.images.poll(&ctx);

        egui::Panel::top("top_bar").show(ui, |ui| self.top_bar(ui));
        egui::Panel::left("combo_list")
            .resizable(true)
            .default_size(230.0)
            .show(ui, |ui| self.combo_list(ui));
        egui::CentralPanel::default().show(ui, |ui| match self.view {
            View::Edit => self.editor(ui),
            View::Practice => self.practice(ui),
        });

        self.picker_window(&ctx);
        self.import_window(&ctx);
        self.delete_window(&ctx);
        self.message_window(&ctx);
        self.poll_update();
        self.update_window(&ctx);
        self.autosave(&ctx);
    }
}

impl ComboApp {
    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("YGO Combomaker");
            ui.separator();
            ui.selectable_value(&mut self.view, View::Edit, "Bearbeiten");
            ui.selectable_value(&mut self.view, View::Practice, "Üben");
            ui.separator();
            ui.menu_button("Einstellungen", |ui| {
                ui.checkbox(
                    &mut self.settings.german_names,
                    "Deutsche Kartennamen anzeigen",
                );
                ui.checkbox(&mut self.settings.md_only, "Suche: nur Master-Duel-Karten");
                ui.add(
                    egui::Slider::new(&mut self.settings.card_width, 48.0..=140.0)
                        .text("Kartengröße"),
                );
                ui.separator();
                if ui.button("Fehlgeschlagene Bilder neu laden").clicked() {
                    self.images.retry_failed();
                }
                ui.separator();
                ui.label(RichText::new("Updates").strong());
                ui.checkbox(&mut self.settings.auto_update_check, "Beim Start nach Updates suchen");
                ui.label("GitHub-Token (nötig, solange das Repo privat ist):");
                ui.add(
                    TextEdit::singleline(&mut self.settings.github_token)
                        .password(true)
                        .hint_text("github_pat_…")
                        .desired_width(260.0),
                );
            });
            let checking = matches!(self.update, UpdateState::Checking | UpdateState::Installing(_));
            if ui
                .add_enabled(!checking, egui::Button::new("Update"))
                .on_hover_text(format!(
                    "Sucht auf GitHub nach einer neuen Version und installiert sie.\nInstalliert: {}",
                    updater::CURRENT_VERSION
                ))
                .clicked()
            {
                let ctx = ui.ctx().clone();
                self.start_update_check(ctx, false);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let busy = self.db_busy();
                if ui
                    .add_enabled(!busy, egui::Button::new("Karten aktualisieren"))
                    .on_hover_text("Prüft bei YGOPRODeck, ob es neue Karten gibt, und lädt sie.")
                    .clicked()
                {
                    let ctx = ui.ctx().clone();
                    self.start_db_job(ctx, true);
                }
                if busy {
                    ui.spinner();
                }
                ui.label(&self.db_status);
            });
        });
    }

    fn combo_list(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("+ Neue Combo").clicked() {
                self.combos.push(Combo::new("Neue Combo"));
                self.select_combo(Some(self.combos.len() - 1));
                self.view = View::Edit;
                self.mark_dirty();
            }
            if ui
                .button("Importieren")
                .on_hover_text("Combo aus der Zwischenablage einfügen")
                .clicked()
            {
                self.import_text = Some(String::new());
            }
        });
        ui.separator();
        let mut new_sel = None;
        let mut duplicate = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (i, c) in self.combos.iter().enumerate() {
                let title = if c.title.is_empty() {
                    "(ohne Titel)"
                } else {
                    &c.title
                };
                let mut text = title.to_string();
                if !c.deck.is_empty() {
                    text.push_str(&format!("\n{}", c.deck));
                }
                let resp = ui.selectable_label(self.selected == Some(i), text);
                if resp.clicked() {
                    new_sel = Some(i);
                }
                resp.context_menu(|ui| {
                    if ui.button("Duplizieren").clicked() {
                        duplicate = Some(i);
                        ui.close();
                    }
                    if ui.button("Löschen …").clicked() {
                        self.confirm_delete = Some(i);
                        ui.close();
                    }
                });
            }
        });
        if let Some(i) = new_sel
            && self.selected != Some(i)
        {
            self.select_combo(Some(i));
        }
        if let Some(i) = duplicate {
            let mut c = self.combos[i].clone();
            c.id = uuid::Uuid::new_v4();
            c.title.push_str(" (Kopie)");
            c.practice = Default::default();
            self.combos.insert(i + 1, c);
            self.select_combo(Some(i + 1));
            self.mark_dirty();
        }
    }

    // -----------------------------------------------------------------------
    // Editor
    // -----------------------------------------------------------------------

    fn editor(&mut self, ui: &mut egui::Ui) {
        let Some(idx) = self.selected else {
            ui.centered_and_justified(|ui| {
                ui.label("Wähle links eine Combo aus oder lege mit „+ Neue Combo“ eine an.");
            });
            return;
        };

        if self.selection.is_some() {
            egui::Panel::right("entry_editor")
                .resizable(false)
                .exact_size(240.0)
                .show(ui, |ui| self.entry_editor(ui, idx));
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            let mut changed = false;
            {
                let c = &mut self.combos[idx];
                egui::Grid::new("meta")
                    .num_columns(2)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Titel");
                        changed |= ui
                            .add(TextEdit::singleline(&mut c.title).desired_width(400.0))
                            .changed();
                        ui.end_row();
                        ui.label("Deck");
                        changed |= ui
                            .add(
                                TextEdit::singleline(&mut c.deck)
                                    .hint_text("z. B. Lunalight")
                                    .desired_width(400.0),
                            )
                            .changed();
                        ui.end_row();
                        ui.label("Autor");
                        changed |= ui
                            .add(TextEdit::singleline(&mut c.author).desired_width(400.0))
                            .changed();
                        ui.end_row();
                        ui.label("Notizen");
                        changed |= ui
                            .add(
                                TextEdit::multiline(&mut c.notes)
                                    .desired_rows(2)
                                    .desired_width(400.0),
                            )
                            .changed();
                        ui.end_row();
                    });
            }
            if changed {
                self.mark_dirty();
            }

            ui.horizontal(|ui| {
                if ui.button("Üben starten").clicked() {
                    self.session = Some(Session::new(&self.combos[idx]));
                    self.view = View::Practice;
                }
                if ui
                    .button("Teilen (in Zwischenablage kopieren)")
                    .on_hover_text(
                        "Kopiert die Combo als Text. Andere können sie mit „Importieren“ einfügen.",
                    )
                    .clicked()
                    && let Ok(text) = storage::combo_to_share_text(&self.combos[idx])
                {
                    ui.ctx().copy_text(text);
                    self.message = Some("Combo wurde in die Zwischenablage kopiert.".into());
                }
            });
            ui.add_space(8.0);

            for section in Section::ALL {
                self.section_ui(ui, idx, section);
            }
            self.steps_ui(ui, idx);
        });
    }

    fn section_ui(&mut self, ui: &mut egui::Ui, idx: usize, section: Section) {
        ui.separator();
        ui.horizontal(|ui| {
            ui.heading(section.label());
            if section == Section::Required
                && ui
                    .small_button("Aus Schritten übernehmen")
                    .on_hover_text("Fügt alle Karten aus den Schritten hinzu, die nicht in der Starthand sind.")
                    .clicked()
            {
                let combo = &mut self.combos[idx];
                let have: Vec<u32> = combo.required.iter().filter_map(|e| e.slot.card_id()).collect();
                for e in combo.cards_used_in_steps_not_in_hand() {
                    if !e.slot.card_id().is_some_and(|id| have.contains(&id)) {
                        combo.required.push(e);
                    }
                }
                self.mark_dirty();
            }
        });
        let width = self.settings.card_width;
        let german = self.settings.german_names;
        let mut clicked = None;
        let mut open_picker = false;
        ui.horizontal_wrapped(|ui| {
            for (i, e) in section.entries(&self.combos[idx]).iter().enumerate() {
                let hl = (self.selection == Some(Sel::Section(section, i))).then_some(SELECT_COLOR);
                if entry_view(ui, &mut self.images, &self.db, german, e, width, hl).clicked() {
                    clicked = Some(i);
                }
            }
            let add_size = Vec2::new(width * 0.8, width * CARD_RATIO);
            if ui
                .add_sized(add_size, egui::Button::new(RichText::new("+").size(26.0)))
                .on_hover_text("Karte hinzufügen")
                .clicked()
            {
                open_picker = true;
            }
        });
        if let Some(i) = clicked {
            self.selection = Some(Sel::Section(section, i));
        }
        if open_picker {
            self.open_picker(Target::Section(section));
        }
    }

    fn steps_ui(&mut self, ui: &mut egui::Ui, idx: usize) {
        enum StepOp {
            Delete(usize),
            Up(usize),
            Down(usize),
            CycleConnector(usize, usize),
            Pick(usize),
            Select(usize, usize),
        }
        ui.separator();
        ui.heading("Schritte");
        let width = self.settings.card_width;
        let german = self.settings.german_names;
        let mut op = None;
        let n = self.combos[idx].steps.len();
        for si in 0..n {
            ui.horizontal(|ui| {
                ui.add_sized(
                    Vec2::new(40.0, width * CARD_RATIO),
                    egui::Label::new(RichText::new(format!("{}.", si + 1)).size(26.0).strong()),
                );
                ui.horizontal_wrapped(|ui| {
                    let step = &self.combos[idx].steps[si];
                    for (pi, part) in step.parts.iter().enumerate() {
                        if pi > 0 {
                            let b = egui::Button::new(
                                RichText::new(part.connector.symbol()).size(22.0),
                            )
                            .frame(false);
                            if ui
                                .add(b)
                                .on_hover_text("Klicken: Zeichen ändern (→ ← + =)")
                                .clicked()
                            {
                                op = Some(StepOp::CycleConnector(si, pi));
                            }
                        }
                        let hl =
                            (self.selection == Some(Sel::StepPart(si, pi))).then_some(SELECT_COLOR);
                        if entry_view(
                            ui,
                            &mut self.images,
                            &self.db,
                            german,
                            &part.entry,
                            width,
                            hl,
                        )
                        .clicked()
                        {
                            op = Some(StepOp::Select(si, pi));
                        }
                    }
                    let add_size = Vec2::new(width * 0.8, width * CARD_RATIO);
                    if ui
                        .add_sized(add_size, egui::Button::new(RichText::new("+").size(26.0)))
                        .on_hover_text("Karte zu diesem Schritt hinzufügen")
                        .clicked()
                    {
                        op = Some(StepOp::Pick(si));
                    }
                    ui.vertical(|ui| {
                        if ui
                            .small_button("↑")
                            .on_hover_text("Schritt nach oben")
                            .clicked()
                        {
                            op = Some(StepOp::Up(si));
                        }
                        if ui
                            .small_button("↓")
                            .on_hover_text("Schritt nach unten")
                            .clicked()
                        {
                            op = Some(StepOp::Down(si));
                        }
                        if ui.small_button("Löschen").clicked() {
                            op = Some(StepOp::Delete(si));
                        }
                    });
                });
            });
            let note = &mut self.combos[idx].steps[si].note;
            if ui
                .add(
                    TextEdit::singleline(note)
                        .hint_text("Notiz zu diesem Schritt (optional)")
                        .desired_width(500.0),
                )
                .changed()
            {
                self.mark_dirty();
            }
            ui.add_space(6.0);
        }
        if ui.button("+ Schritt hinzufügen").clicked() {
            self.combos[idx].steps.push(Step::default());
            let last = self.combos[idx].steps.len() - 1;
            self.mark_dirty();
            self.open_picker(Target::Step(last));
        }

        let Some(op) = op else { return };
        let steps = &mut self.combos[idx].steps;
        match op {
            StepOp::Select(si, pi) => {
                self.selection = Some(Sel::StepPart(si, pi));
                return;
            }
            StepOp::Pick(si) => {
                self.open_picker(Target::Step(si));
                return;
            }
            StepOp::CycleConnector(si, pi) => {
                let c = &mut steps[si].parts[pi].connector;
                let pos = Connector::ALL.iter().position(|x| x == c).unwrap_or(0);
                *c = Connector::ALL[(pos + 1) % Connector::ALL.len()];
            }
            StepOp::Delete(si) => {
                steps.remove(si);
                self.selection = None;
                self.picker = None;
            }
            StepOp::Up(si) if si > 0 => {
                steps.swap(si, si - 1);
                self.selection = None;
            }
            StepOp::Down(si) if si + 1 < steps.len() => {
                steps.swap(si, si + 1);
                self.selection = None;
            }
            _ => return,
        }
        self.mark_dirty();
    }

    /// Rechte Leiste: ausgewählte Karte bearbeiten.
    fn entry_editor(&mut self, ui: &mut egui::Ui, idx: usize) {
        enum EditOp {
            Left,
            Right,
            Remove,
            Close,
        }
        let Some(sel) = self.selection else { return };
        let german = self.settings.german_names;
        let combo = &mut self.combos[idx];
        let (entry, connector, len, pos) = match sel {
            Sel::Section(sec, i) => {
                let v = sec.entries_mut(combo);
                let len = v.len();
                match v.get_mut(i) {
                    Some(e) => (e, None, len, i),
                    None => {
                        self.selection = None;
                        return;
                    }
                }
            }
            Sel::StepPart(si, pi) => {
                let Some(step) = combo.steps.get_mut(si) else {
                    self.selection = None;
                    return;
                };
                let len = step.parts.len();
                match step.parts.get_mut(pi) {
                    Some(p) => (&mut p.entry, (pi > 0).then_some(&mut p.connector), len, pi),
                    None => {
                        self.selection = None;
                        return;
                    }
                }
            }
        };

        let mut changed = false;
        let mut op = None;
        ui.heading("Karte bearbeiten");
        ui.add_space(4.0);
        card_widget(
            ui,
            &mut self.images,
            &self.db,
            german,
            &entry.slot,
            entry.action,
            160.0,
            None,
        );
        ui.label(RichText::new(shown_name(&self.db, german, &entry.slot)).strong());
        ui.add_space(6.0);

        if let Slot::Placeholder { kind, label } = &mut entry.slot {
            ui.label("Platzhalter-Text");
            changed |= ui.text_edit_singleline(label).changed();
            egui::ComboBox::from_id_salt("ph_kind")
                .selected_text(kind.label())
                .show_ui(ui, |ui| {
                    for k in CardKind::ALL {
                        changed |= ui.selectable_value(kind, k, k.label()).changed();
                    }
                });
        }

        ui.label("Aktion");
        egui::ComboBox::from_id_salt("action")
            .selected_text(entry.action.label())
            .width(200.0)
            .show_ui(ui, |ui| {
                for a in Action::CHOICES {
                    changed |= ui
                        .selectable_value(&mut entry.action, a, a.label())
                        .changed();
                }
            });

        if let Some(conn) = connector {
            ui.label("Zeichen davor");
            ui.horizontal(|ui| {
                for c in Connector::ALL {
                    changed |= ui.selectable_value(conn, c, c.symbol()).changed();
                }
            });
        }

        ui.label("Notiz");
        changed |= ui
            .add(
                TextEdit::multiline(&mut entry.note)
                    .desired_rows(2)
                    .hint_text("z. B. Board Wipe"),
            )
            .changed();

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(pos > 0, egui::Button::new("← Links"))
                .clicked()
            {
                op = Some(EditOp::Left);
            }
            if ui
                .add_enabled(pos + 1 < len, egui::Button::new("Rechts →"))
                .clicked()
            {
                op = Some(EditOp::Right);
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Entfernen").clicked() {
                op = Some(EditOp::Remove);
            }
            if ui.button("Schließen").clicked() {
                op = Some(EditOp::Close);
            }
        });

        if changed {
            self.mark_dirty();
        }
        let Some(op) = op else { return };
        if let EditOp::Close = op {
            self.selection = None;
            return;
        }
        let combo = &mut self.combos[idx];
        // Hilfsfunktion: gleiche Bewegung für beide Listenarten.
        fn apply<T>(v: &mut Vec<T>, i: usize, op: &EditOp) -> Option<usize> {
            match op {
                EditOp::Left if i > 0 => {
                    v.swap(i, i - 1);
                    Some(i - 1)
                }
                EditOp::Right if i + 1 < v.len() => {
                    v.swap(i, i + 1);
                    Some(i + 1)
                }
                EditOp::Remove => {
                    v.remove(i);
                    None
                }
                _ => Some(i),
            }
        }
        self.selection = match sel {
            Sel::Section(sec, i) => {
                apply(sec.entries_mut(combo), i, &op).map(|n| Sel::Section(sec, n))
            }
            Sel::StepPart(si, pi) => {
                apply(&mut combo.steps[si].parts, pi, &op).map(|n| Sel::StepPart(si, n))
            }
        };
        self.mark_dirty();
    }

    // -----------------------------------------------------------------------
    // Übungsmodus
    // -----------------------------------------------------------------------

    fn practice(&mut self, ui: &mut egui::Ui) {
        let Some(idx) = self.selected else {
            ui.label("Wähle links eine Combo zum Üben aus.");
            return;
        };
        let width = self.settings.card_width;
        let german = self.settings.german_names;

        let active = self
            .session
            .as_ref()
            .is_some_and(|s| s.combo_id == self.combos[idx].id);
        if !active {
            let c = &self.combos[idx];
            ui.heading(&c.title);
            if c.steps.is_empty() {
                ui.label(
                    "Diese Combo hat noch keine Schritte. Trage sie zuerst unter „Bearbeiten“ ein.",
                );
                return;
            }
            ui.label(format!("{} Schritte", c.steps.len()));
            ui.label(format!(
                "Bisher geübt: {}-mal, bestes Ergebnis: {} %",
                c.practice.attempts, c.practice.best_percent
            ));
            ui.add_space(8.0);
            ui.label("Ablauf: Du siehst die Starthand. Für jeden Schritt wählst du die Karten aus, die benutzt werden. Danach prüft die App und zeigt den echten Schritt.");
            if ui.button("Übung starten").clicked() {
                self.session = Some(Session::new(&self.combos[idx]));
            }
            return;
        }

        // Ergebnis einmalig speichern, wenn die Übung fertig ist.
        {
            let combo = &self.combos[idx];
            let session = self.session.as_mut().unwrap();
            if session.finished(combo) && !session.stats_saved {
                session.stats_saved = true;
                let percent = session.percent();
                let stats = &mut self.combos[idx].practice;
                stats.attempts += 1;
                stats.best_percent = stats.best_percent.max(percent);
                self.mark_dirty();
            }
        }

        enum POp {
            Check,
            Reveal,
            Next,
            RemoveGuess(usize),
            Pick,
            Restart,
            Stop,
        }
        let mut op = None;
        let combo = &self.combos[idx];
        let session = self.session.as_mut().unwrap();
        let images = &mut self.images;
        let db = &self.db;

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(&combo.title);
                ui.separator();
                let total = combo.steps.len();
                let shown = (session.current + 1).min(total);
                ui.label(format!("Schritt {shown} von {total}"));
                ui.separator();
                ui.label(format!("Richtig: {} von {}", session.correct_count(), session.results.len()));
                ui.separator();
                ui.checkbox(&mut session.show_endboard, "Endboard als Ziel zeigen");
                if ui.button("Abbrechen").clicked() {
                    op = Some(POp::Stop);
                }
            });
            ui.separator();

            let row = |ui: &mut egui::Ui, images: &mut ImageCache, title: &str, entries: &[Entry]| {
                ui.label(RichText::new(title).strong());
                ui.horizontal_wrapped(|ui| {
                    for e in entries {
                        entry_view(ui, images, db, german, e, width, None);
                    }
                });
            };
            row(ui, images, "Starthand", &combo.starting_hand);
            if session.show_endboard && !combo.endboard.is_empty() {
                row(ui, images, "Ziel (Endboard)", &combo.endboard);
            }
            ui.separator();

            // Bereits gelöste Schritte
            for (si, ok) in session.results.iter().enumerate() {
                ui.horizontal(|ui| {
                    let (txt, col) = if *ok { ("Richtig", GOOD_COLOR) } else { ("Falsch", BAD_COLOR) };
                    ui.label(RichText::new(format!("{}.", si + 1)).size(20.0).strong());
                    ui.label(RichText::new(txt).color(col));
                    step_row(ui, images, db, german, &combo.steps[si], width * 0.75, None);
                });
            }

            if session.finished(combo) {
                ui.add_space(10.0);
                ui.heading(format!(
                    "Fertig: {} von {} Schritten richtig ({} %)",
                    session.correct_count(),
                    session.results.len(),
                    session.percent()
                ));
                ui.horizontal(|ui| {
                    if ui.button("Nochmal üben").clicked() {
                        op = Some(POp::Restart);
                    }
                    if ui.button("Beenden").clicked() {
                        op = Some(POp::Stop);
                    }
                });
                return;
            }

            let si = session.current;
            ui.add_space(8.0);
            ui.label(RichText::new(format!("Schritt {}", si + 1)).size(22.0).strong());
            match &session.checked {
                None => {
                    ui.label("Welche Karten werden in diesem Schritt benutzt? Klicke auf eine geratene Karte, um sie zu entfernen.");
                    ui.horizontal_wrapped(|ui| {
                        for (gi, g) in session.guesses.iter().enumerate() {
                            if card_widget(ui, images, db, german, g, Action::None, width, None).clicked() {
                                op = Some(POp::RemoveGuess(gi));
                            }
                        }
                        let add_size = Vec2::new(width * 0.8, width * CARD_RATIO);
                        if ui.add_sized(add_size, egui::Button::new(RichText::new("+").size(26.0))).on_hover_text("Karte raten").clicked() {
                            op = Some(POp::Pick);
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Prüfen").clicked() {
                            op = Some(POp::Check);
                        }
                        if ui.button("Aufdecken (zählt als falsch)").clicked() {
                            op = Some(POp::Reveal);
                        }
                    });
                }
                Some(check) => {
                    let (txt, col) = if check.is_correct() {
                        ("Richtig!", GOOD_COLOR)
                    } else {
                        ("Nicht ganz. Grün = erraten, Rot = fehlte.", BAD_COLOR)
                    };
                    ui.label(RichText::new(txt).color(col).size(18.0));
                    ui.label("Richtiger Schritt:");
                    step_row(ui, images, db, german, &combo.steps[si], width, Some(&check.part_hit));
                    if !combo.steps[si].note.is_empty() {
                        ui.label(RichText::new(&combo.steps[si].note).italics());
                    }
                    if !check.wrong_guesses.is_empty() {
                        ui.label("Falsch geraten:");
                        ui.horizontal_wrapped(|ui| {
                            for &gi in &check.wrong_guesses {
                                card_widget(ui, images, db, german, &session.guesses[gi], Action::None, width, Some(BAD_COLOR));
                            }
                        });
                    }
                    if ui.button("Weiter").clicked() {
                        op = Some(POp::Next);
                    }
                }
            }
        });

        let Some(op) = op else { return };
        let combo = &self.combos[idx];
        match op {
            POp::Check => self.session.as_mut().unwrap().check(combo),
            POp::Reveal => self.session.as_mut().unwrap().reveal(combo),
            POp::Next => self.session.as_mut().unwrap().next(),
            POp::RemoveGuess(i) => {
                self.session.as_mut().unwrap().guesses.remove(i);
            }
            POp::Pick => self.open_picker(Target::PracticeGuess),
            POp::Restart => self.session = Some(Session::new(combo)),
            POp::Stop => self.session = None,
        }
    }

    // -----------------------------------------------------------------------
    // Fenster
    // -----------------------------------------------------------------------

    fn picker_window(&mut self, ctx: &egui::Context) {
        let Some(picker) = &mut self.picker else {
            return;
        };
        let mut open = true;
        let mut chosen: Option<Slot> = None;
        let german = self.settings.german_names;
        let md_only = &mut self.settings.md_only;
        let images = &mut self.images;
        let db = &self.db;
        let title = match picker.target {
            Target::PracticeGuess => "Karte raten",
            _ => "Karte hinzufügen",
        };
        egui::Window::new(title)
            .open(&mut open)
            .default_size([420.0, 520.0])
            .pivot(Align2::CENTER_TOP)
            .default_pos(ctx.content_rect().center_top() + Vec2::new(0.0, 60.0))
            .collapsible(false)
            .show(ctx, |ui| {
                let resp = ui.add(
                    TextEdit::singleline(&mut picker.query)
                        .hint_text("Kartenname (englisch oder deutsch) …")
                        .desired_width(f32::INFINITY),
                );
                // Fokus so lange anfordern, bis das Feld ihn wirklich hat
                // (im ersten Frame ist das Fenster noch unsichtbar).
                if !picker.focus_requested {
                    if resp.has_focus() {
                        picker.focus_requested = true;
                    } else {
                        resp.request_focus();
                    }
                }
                ui.checkbox(md_only, "Nur Master-Duel-Karten");

                let results = db.search(&picker.query, *md_only, 60);
                if db.is_empty() {
                    ui.label("Die Kartendatenbank ist noch nicht geladen.");
                } else if picker.query.trim().is_empty() {
                    ui.label("Tippe einen Namen ein. Die Liste zeigt bis zu 60 Treffer. Eingabetaste = ersten Treffer übernehmen.");
                } else if results.is_empty() {
                    ui.label("Keine Treffer.");
                }
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    && let Some(c) = results.first() {
                        chosen = Some(c.to_slot());
                        picker.query.clear();
                        picker.focus_requested = false;
                    }

                egui::ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
                    for card in &results {
                        ui.horizontal(|ui| {
                            let slot = card.to_slot();
                            let thumb = card_widget(ui, images, db, german, &slot, Action::None, 40.0, None);
                            let name = match (&card.name_de, german) {
                                (Some(de), true) => format!("{de}\n{}", card.name),
                                _ => card.name.clone(),
                            };
                            let label = ui.selectable_label(false, format!("{name}\n{}", card.type_line));
                            if thumb.clicked() || label.clicked() {
                                chosen = Some(slot);
                            }
                        });
                    }
                });

                if picker.target != Target::PracticeGuess {
                    ui.separator();
                    ui.label("Oder Platzhalter (z. B. „Any Card“, „discard“):");
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("ph_new_kind")
                            .selected_text(picker.placeholder_kind.label())
                            .show_ui(ui, |ui| {
                                for k in CardKind::ALL {
                                    ui.selectable_value(&mut picker.placeholder_kind, k, k.label());
                                }
                            });
                        ui.add(TextEdit::singleline(&mut picker.placeholder_label).desired_width(140.0));
                        if ui.button("Hinzufügen").clicked() {
                            chosen = Some(Slot::Placeholder {
                                kind: picker.placeholder_kind,
                                label: picker.placeholder_label.clone(),
                            });
                        }
                    });
                }
            });
        let target = picker.target;
        if let Some(slot) = chosen {
            self.add_to_target(target, slot);
        }
        if !open {
            self.picker = None;
        }
    }

    fn import_window(&mut self, ctx: &egui::Context) {
        let Some(text) = &mut self.import_text else {
            return;
        };
        let mut open = true;
        let mut do_import = false;
        egui::Window::new("Combo importieren")
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("Füge hier den kopierten Combo-Text ein (Strg+V):");
                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .show(ui, |ui| {
                        ui.add(
                            TextEdit::multiline(text)
                                .desired_rows(10)
                                .desired_width(400.0)
                                .code_editor(),
                        );
                    });
                if ui.button("Importieren").clicked() {
                    do_import = true;
                }
            });
        if do_import {
            match storage::combo_from_share_text(text) {
                Ok(c) => {
                    self.combos.push(c);
                    self.select_combo(Some(self.combos.len() - 1));
                    self.view = View::Edit;
                    self.mark_dirty();
                    self.import_text = None;
                }
                Err(e) => self.message = Some(format!("Import fehlgeschlagen: {e:#}")),
            }
        }
        if !open {
            self.import_text = None;
        }
    }

    fn delete_window(&mut self, ctx: &egui::Context) {
        let Some(i) = self.confirm_delete else { return };
        let Some(combo) = self.combos.get(i) else {
            self.confirm_delete = None;
            return;
        };
        let title = combo.title.clone();
        let mut answer = None;
        egui::Window::new("Combo löschen?")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(format!("„{title}“ wird endgültig gelöscht."));
                ui.horizontal(|ui| {
                    if ui.button("Löschen").clicked() {
                        answer = Some(true);
                    }
                    if ui.button("Abbrechen").clicked() {
                        answer = Some(false);
                    }
                });
            });
        match answer {
            Some(true) => {
                self.combos.remove(i);
                let new_sel = if self.combos.is_empty() {
                    None
                } else {
                    Some(i.min(self.combos.len() - 1))
                };
                self.select_combo(new_sel);
                self.confirm_delete = None;
                self.mark_dirty();
            }
            Some(false) => self.confirm_delete = None,
            None => {}
        }
    }

    fn message_window(&mut self, ctx: &egui::Context) {
        let Some(msg) = &self.message else { return };
        let mut close = false;
        egui::Window::new("Hinweis")
            .collapsible(false)
            .anchor(Align2::CENTER_TOP, [0.0, 60.0])
            .show(ctx, |ui| {
                ui.label(msg);
                if ui.button("OK").clicked() {
                    close = true;
                }
            });
        if close {
            self.message = None;
        }
    }
}

/// Zeigt einen Schritt (Karten mit Zeichen dazwischen). `hits` färbt die
/// Karten grün (erraten) oder rot (fehlte).
fn step_row(
    ui: &mut egui::Ui,
    images: &mut ImageCache,
    db: &CardDb,
    german: bool,
    step: &Step,
    width: f32,
    hits: Option<&Vec<bool>>,
) {
    ui.horizontal_wrapped(|ui| {
        for (pi, part) in step.parts.iter().enumerate() {
            if pi > 0 {
                ui.label(RichText::new(part.connector.symbol()).size(20.0));
            }
            let hl = hits.map(|h| {
                if h.get(pi).copied().unwrap_or(false) {
                    GOOD_COLOR
                } else {
                    BAD_COLOR
                }
            });
            entry_view(ui, images, db, german, &part.entry, width, hl);
        }
    });
}
