//! Oberfläche (egui). Aufbau:
//! - oben: Leiste mit Ansicht (Bearbeiten/Üben/Decks), Einstellungen, Update
//! - links: Liste der Combos
//! - Mitte: Editor, Übungsmodus oder Decks
//! - Fenster: Kartensuche, Import, Löschen bestätigen, Update

mod decks_view;
mod editor;
mod field;
mod hand_test;
mod picker;
mod practice_view;
mod widgets;

use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use egui::{Align2, RichText, TextEdit};

use self::field::FieldUi;
use self::widgets::BAD_COLOR;
use crate::board::Board;
use crate::cards::{self, CardDb, CardDbFile};
use crate::i18n::{self, Lang, tr, trf};
use crate::images::ImageCache;
use crate::model::{CardKind, Combo, Deck, Section};
use crate::practice::Session;
use crate::storage::{self, Paths, Settings};
use crate::updater::{self, UpdateInfo};

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Edit,
    Practice,
    Decks,
    /// Zufällige Starthand aus einem Deck ziehen.
    HandTest,
}

/// Wie Combos im Editor bearbeitet werden.
#[derive(Clone, Copy, PartialEq, Eq)]
enum EditMode {
    /// Züge auf dem Spielfeld aufnehmen.
    Field,
    /// Alle Bereiche als Liste bearbeiten.
    List,
}

/// Wohin eine Karte aus der Suche hinzugefügt wird.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    Section(Section),
    Step(usize),
    Deck,
    /// Karte für das Spielfeld aus Deck oder Extra Deck (Aufnahme).
    RecordFrom(crate::board::Area),
    /// Karte für das Spielfeld aus Deck oder Extra Deck (Üben).
    PracticeFrom(crate::board::Area),
    /// Karte für das freie Spielfeld im Handtest.
    FreeFrom(crate::board::Area),
}

/// Welche Karte im Listen-Editor ausgewählt ist.
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

/// Aufnahme von Zügen auf dem Spielfeld.
struct Recorder {
    combo_id: uuid::Uuid,
    board: Board,
    field: FieldUi,
    /// Starthand, mit der das Feld aufgebaut wurde.
    hand: Vec<crate::model::Slot>,
}

/// Übung auf dem Spielfeld.
struct Practice {
    session: Session,
    field: FieldUi,
    /// Rückmeldung zum letzten Zug: (Text, richtig?).
    feedback: Option<(String, bool)>,
}

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
    /// Suchtext für die Combo-Liste.
    combo_filter: String,
    view: View,
    edit_mode: EditMode,
    db: CardDb,
    db_status: String,
    db_rx: Option<Receiver<DbMsg>>,
    images: ImageCache,
    picker: Option<Picker>,
    selection: Option<Sel>,
    recorder: Option<Recorder>,
    practice: Option<Practice>,
    dirty: bool,
    last_change: Instant,
    import_text: Option<String>,
    confirm_delete: Option<usize>,
    decks: Vec<Deck>,
    selected_deck: Option<usize>,
    /// Name und Text für den Deck-Import.
    deck_import: Option<(String, String)>,
    archetype_query: String,
    confirm_delete_deck: Option<usize>,
    hand_test: hand_test::HandTestState,
    message: Option<String>,
    update: UpdateState,
    update_rx: Option<Receiver<UpdateMsg>>,
    /// Automatische Prüfung beim Start: nur melden, wenn es ein Update gibt.
    update_silent: bool,
}

impl ComboApp {
    pub fn new(cc: &eframe::CreationContext<'_>, paths: Paths) -> Self {
        let mut settings = storage::load_settings(&paths);
        let lang = *settings.lang.get_or_insert_with(Lang::from_system);
        i18n::set_lang(lang);
        let decks = storage::load_decks(&paths).unwrap_or_default();
        let (combos, message) = match storage::load_combos(&paths) {
            Ok(c) => (c, None),
            Err(e) => (
                Vec::new(),
                Some(trf("msg.combos_load_failed", &[&format!("{e:#}")])),
            ),
        };
        let images = ImageCache::new(cc.egui_ctx.clone(), paths.image_dir());
        let mut app = Self {
            paths,
            saved_settings: Settings::default(),
            settings,
            selected: if combos.is_empty() { None } else { Some(0) },
            combos,
            combo_filter: String::new(),
            view: View::Edit,
            edit_mode: EditMode::Field,
            db: CardDb::default(),
            db_status: String::new(),
            db_rx: None,
            images,
            picker: None,
            selection: None,
            recorder: None,
            practice: None,
            dirty: false,
            last_change: Instant::now(),
            import_text: None,
            confirm_delete: None,
            selected_deck: if decks.is_empty() { None } else { Some(0) },
            decks,
            deck_import: None,
            archetype_query: String::new(),
            confirm_delete_deck: None,
            hand_test: hand_test::HandTestState::default(),
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

    /// Sprachcode für Kartennamen (`None` = Englisch).
    fn name_lang(&self) -> Option<&'static str> {
        if self.settings.localized_card_names {
            i18n::lang().card_api_code()
        } else {
            None
        }
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
                send(DbMsg::Progress(tr("db.loading_disk").into()));
                if let Ok(file) = cards::load_from_disk(&path) {
                    // Ältere Datenbank ohne französische/italienische/portugiesische
                    // Namen: einmal neu laden.
                    let has_new_names = file.cards.iter().any(|c| c.names.contains_key("fr"));
                    if has_new_names {
                        send(DbMsg::Loaded(file));
                        return;
                    }
                }
            } else if have_cards {
                send(DbMsg::Progress(tr("db.checking_new").into()));
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
                    progress(tr("db.saving"));
                    if let Err(e) = cards::save_to_disk(&path, &file) {
                        send(DbMsg::Failed(trf("msg.save_failed", &[&format!("{e:#}")])));
                    }
                    send(DbMsg::Loaded(file));
                }
                Err(e) => {
                    // Ohne Internet: alte Datei benutzen, falls vorhanden.
                    if let Ok(file) = cards::load_from_disk(&path) {
                        send(DbMsg::Loaded(file));
                    } else {
                        send(DbMsg::Failed(format!("{e:#}")));
                    }
                }
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
                    self.db_status = trf("db.loaded", &[&self.db.cards.len()]);
                    finished = true;
                }
                DbMsg::UpToDate => {
                    self.db_status = trf("db.up_to_date", &[&self.db.cards.len()]);
                    finished = true;
                }
                DbMsg::Failed(e) => {
                    self.db_status = tr("db.error").into();
                    self.message = Some(trf("msg.db_failed", &[&e]));
                    finished = true;
                }
            }
        }
        if finished {
            self.db_rx = None;
        }
    }

    fn db_busy(&self) -> bool {
        self.db_rx.is_some()
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
        egui::Window::new(tr("update.title"))
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_TOP, [0.0, 60.0])
            .show(ctx, |ui| {
                ui.label(trf(
                    "update.installed_version",
                    &[&updater::CURRENT_VERSION],
                ));
                ui.add_space(4.0);
                match &self.update {
                    UpdateState::Idle => {}
                    UpdateState::Checking => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(tr("update.checking"));
                        });
                    }
                    UpdateState::UpToDate => {
                        ui.label(tr("update.up_to_date"));
                        close = ui.button(tr("common.ok")).clicked();
                    }
                    UpdateState::Available(info) => {
                        ui.label(RichText::new(trf("update.available", &[&info.version])).strong());
                        if !info.notes.trim().is_empty() {
                            egui::ScrollArea::vertical()
                                .max_height(200.0)
                                .show(ui, |ui| {
                                    ui.label(&info.notes);
                                });
                        }
                        ui.horizontal(|ui| {
                            if ui.button(tr("update.install_now")).clicked() {
                                install = Some(info.clone());
                            }
                            close = ui.button(tr("update.later")).clicked();
                        });
                    }
                    UpdateState::Installing(v) => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(trf("update.installing", &[v]));
                        });
                    }
                    UpdateState::Installed(exe, v) => {
                        ui.label(trf("update.installed", &[v]));
                        ui.label(tr("update.combos_saved"));
                        if ui.button(tr("update.restart")).clicked() {
                            restart = Some(exe.clone());
                        }
                    }
                    UpdateState::Failed(e) => {
                        ui.label(RichText::new(tr("update.failed")).color(BAD_COLOR));
                        ui.label(e);
                        close = ui.button(tr("common.ok")).clicked();
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
        let result = storage::save_combos(&self.paths, &self.combos)
            .and_then(|_| storage::save_decks(&self.paths, &self.decks));
        match result {
            Ok(()) => self.dirty = false,
            Err(e) => self.message = Some(trf("msg.save_failed", &[&format!("{e:#}")])),
        }
    }

    fn select_combo(&mut self, idx: Option<usize>) {
        self.selected = idx;
        self.selection = None;
        self.picker = None;
        self.recorder = None;
        self.practice = None;
    }
}

impl Drop for ComboApp {
    fn drop(&mut self) {
        if self.dirty {
            let _ = storage::save_combos(&self.paths, &self.combos);
            let _ = storage::save_decks(&self.paths, &self.decks);
        }
    }
}

// ---------------------------------------------------------------------------
// Hauptschleife
// ---------------------------------------------------------------------------

impl eframe::App for ComboApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_db();
        self.images.poll(&ctx);

        egui::Panel::top("top_bar")
            .frame(
                egui::Frame::new()
                    .fill(crate::theme::CARD_BG)
                    .inner_margin(egui::Margin::symmetric(10, 6)),
            )
            .show(ui, |ui| self.top_bar(ui));
        if !matches!(self.view, View::Decks | View::HandTest) {
            egui::Panel::left("combo_list")
                .resizable(true)
                .default_size(230.0)
                .show(ui, |ui| self.combo_list(ui));
        }
        egui::CentralPanel::default().show(ui, |ui| match self.view {
            View::Edit => self.editor(ui),
            View::Practice => self.practice_view(ui),
            View::Decks => self.decks_view(ui),
            View::HandTest => self.hand_test_view(ui),
        });

        self.picker_window(&ctx);
        self.import_window(&ctx);
        self.delete_window(&ctx);
        self.deck_import_window(&ctx);
        self.delete_deck_window(&ctx);
        self.message_window(&ctx);
        self.poll_update();
        self.update_window(&ctx);
        self.autosave(&ctx);
    }
}

impl ComboApp {
    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading(RichText::new("YGO Combomaker").strong());
            ui.separator();
            ui.selectable_value(&mut self.view, View::Edit, tr("view.edit"));
            ui.selectable_value(&mut self.view, View::Practice, tr("view.practice"));
            ui.selectable_value(&mut self.view, View::Decks, tr("view.decks"));
            ui.selectable_value(&mut self.view, View::HandTest, tr("view.hand_test"));
            ui.separator();
            // Das Menü bleibt offen, bis man außerhalb klickt.
            egui::containers::menu::MenuButton::new(tr("settings.title"))
                .config(
                    egui::containers::menu::MenuConfig::new()
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside),
                )
                .ui(ui, |ui| self.settings_menu(ui));
            let busy_update = matches!(
                self.update,
                UpdateState::Checking | UpdateState::Installing(_)
            );
            if ui
                .add_enabled(!busy_update, egui::Button::new(tr("update.button")))
                .on_hover_text(trf("update.button_hint", &[&updater::CURRENT_VERSION]))
                .clicked()
            {
                let ctx = ui.ctx().clone();
                self.start_update_check(ctx, false);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let busy = self.db_busy();
                if ui
                    .add_enabled(!busy, egui::Button::new(tr("db.update_button")))
                    .on_hover_text(tr("db.update_hint"))
                    .clicked()
                {
                    let ctx = ui.ctx().clone();
                    self.start_db_job(ctx, true);
                }
                if busy {
                    ui.spinner();
                }
                ui.label(RichText::new(&self.db_status).weak());
            });
        });
    }

    fn settings_menu(&mut self, ui: &mut egui::Ui) {
        ui.set_min_width(280.0);
        ui.label(RichText::new(tr("settings.language")).strong());
        let mut lang = self.settings.lang.unwrap_or_default();
        // Knöpfe statt Dropdown: Ein Dropdown im Menü würde das Menü schließen.
        ui.horizontal_wrapped(|ui| {
            for l in Lang::ALL {
                ui.selectable_value(&mut lang, l, l.native_name());
            }
        });
        if Some(lang) != self.settings.lang {
            self.settings.lang = Some(lang);
            i18n::set_lang(lang);
        }
        let can_localize = lang.card_api_code().is_some();
        ui.add_enabled(
            can_localize,
            egui::Checkbox::new(
                &mut self.settings.localized_card_names,
                tr("settings.localized_names"),
            ),
        )
        .on_disabled_hover_text(tr("settings.localized_names_unavailable"));
        ui.separator();
        ui.checkbox(&mut self.settings.md_only, tr("settings.md_only"));
        ui.add(
            egui::Slider::new(&mut self.settings.card_width, 48.0..=280.0)
                .text(tr("settings.card_size")),
        );
        ui.add(
            egui::Slider::new(&mut self.settings.picker_card_width, 40.0..=220.0)
                .text(tr("settings.picker_card_size")),
        );
        ui.checkbox(&mut self.settings.auto_zone, tr("settings.auto_zone"))
            .on_hover_text(tr("settings.auto_zone_hint"));
        ui.checkbox(
            &mut self.settings.auto_update_check,
            tr("settings.auto_update"),
        );
        ui.separator();
        if ui.button(tr("settings.retry_images")).clicked() {
            self.images.retry_failed();
        }
    }

    fn combo_list(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button(tr("combo.new")).clicked() {
                self.combos.push(Combo::new(tr("combo.new_title")));
                self.select_combo(Some(self.combos.len() - 1));
                self.view = View::Edit;
                self.mark_dirty();
            }
            if ui
                .button(tr("common.import"))
                .on_hover_text(tr("combo.import_hint"))
                .clicked()
            {
                self.import_text = Some(String::new());
            }
        });
        ui.add(
            TextEdit::singleline(&mut self.combo_filter)
                .hint_text(tr("combo.filter_hint"))
                .desired_width(f32::INFINITY),
        );
        ui.separator();
        // Filter: Titel oder Deck enthält den Suchtext (ohne Groß/Klein).
        let filter = self.combo_filter.trim().to_lowercase();
        let mut shown = 0;
        let mut new_sel = None;
        let mut duplicate = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (i, c) in self.combos.iter().enumerate() {
                if !filter.is_empty()
                    && !c.title.to_lowercase().contains(&filter)
                    && !c.deck.to_lowercase().contains(&filter)
                {
                    continue;
                }
                shown += 1;
                let title = if c.title.is_empty() {
                    tr("combo.untitled")
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
                    if ui.button(tr("combo.duplicate")).clicked() {
                        duplicate = Some(i);
                        ui.close();
                    }
                    if ui.button(tr("common.delete_dots")).clicked() {
                        self.confirm_delete = Some(i);
                        ui.close();
                    }
                });
            }
            if self.combos.is_empty() {
                ui.label(RichText::new(tr("combo.list_empty")).weak());
            } else if shown == 0 {
                ui.label(RichText::new(tr("combo.filter_no_match")).weak());
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
            c.title.push_str(tr("combo.copy_suffix"));
            c.practice = Default::default();
            self.combos.insert(i + 1, c);
            self.select_combo(Some(i + 1));
            self.mark_dirty();
        }
    }

    // -----------------------------------------------------------------------
    // Fenster
    // -----------------------------------------------------------------------

    fn import_window(&mut self, ctx: &egui::Context) {
        let Some(text) = &mut self.import_text else {
            return;
        };
        let mut open = true;
        let mut do_import = false;
        egui::Window::new(tr("combo.import_title"))
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(tr("combo.import_paste"));
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
                if ui.button(tr("common.import")).clicked() {
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
                Err(e) => self.message = Some(trf("msg.import_failed", &[&format!("{e:#}")])),
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
        egui::Window::new(tr("combo.delete_title"))
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(trf("combo.delete_text", &[&title]));
                ui.horizontal(|ui| {
                    if ui.button(tr("common.delete")).clicked() {
                        answer = Some(true);
                    }
                    if ui.button(tr("common.cancel")).clicked() {
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
        egui::Window::new(tr("msg.title"))
            .collapsible(false)
            .anchor(Align2::CENTER_TOP, [0.0, 60.0])
            .show(ctx, |ui| {
                ui.label(msg);
                if ui.button(tr("common.ok")).clicked() {
                    close = true;
                }
            });
        if close {
            self.message = None;
        }
    }
}
