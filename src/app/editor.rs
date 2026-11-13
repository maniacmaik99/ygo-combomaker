//! Combo-Editor: Kopfdaten, Aufnahme auf dem Spielfeld und Listen-Ansicht.

use egui::{RichText, TextEdit, Vec2};

use super::field::{FieldEvent, FieldUi};
use super::widgets::{
    CARD_RATIO, Marks, SELECT_COLOR, View as CardView, action_combo, add_button, card_widget,
    entry_view, is_extra, kind_of, shown_name, step_row,
};
use super::{ComboApp, EditMode, Recorder, Sel, Target, View};
use crate::board::{self, Board};
use crate::i18n::{tr, trf};
use crate::model::{CardKind, Connector, Facing, Section, Slot, Step, Zone};
use crate::storage;

impl ComboApp {
    pub(super) fn editor(&mut self, ui: &mut egui::Ui) {
        let Some(idx) = self.selected else {
            ui.centered_and_justified(|ui| {
                ui.label(tr("editor.no_combo"));
            });
            return;
        };

        if self.edit_mode == EditMode::List && self.selection.is_some() {
            egui::Panel::right("entry_editor")
                .resizable(false)
                .exact_size(260.0)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| self.entry_editor(ui, idx));
                });
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            // Kopfdaten einklappbar, damit das Spielfeld ohne Scrollen sichtbar ist.
            // Bei einer neuen Combo (noch ohne Schritte) sind sie aufgeklappt.
            let fresh = self.combos[idx].steps.is_empty();
            egui::CollapsingHeader::new(RichText::new(tr("editor.meta")).strong())
                .id_salt(("meta", self.combos[idx].id))
                .default_open(fresh || self.edit_mode == EditMode::List)
                .show(ui, |ui| self.meta_ui(ui, idx));
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr("editor.mode")).strong());
                ui.selectable_value(
                    &mut self.edit_mode,
                    EditMode::Field,
                    tr("editor.mode_field"),
                )
                .on_hover_text(tr("editor.mode_field_hint"));
                ui.selectable_value(&mut self.edit_mode, EditMode::List, tr("editor.mode_list"))
                    .on_hover_text(tr("editor.mode_list_hint"));
            });
            ui.separator();
            match self.edit_mode {
                EditMode::Field => {
                    self.section_ui(ui, idx, Section::StartingHand);
                    ui.separator();
                    self.recorder_ui(ui, idx);
                    ui.separator();
                    self.steps_overview(ui, idx);
                }
                EditMode::List => {
                    for section in Section::ALL {
                        self.section_ui(ui, idx, section);
                    }
                    self.steps_ui(ui, idx);
                }
            }
        });
    }

    fn meta_ui(&mut self, ui: &mut egui::Ui, idx: usize) {
        let mut changed = false;
        let decks = &self.decks;
        let c = &mut self.combos[idx];
        egui::Grid::new("meta")
            .num_columns(2)
            .spacing([8.0, 6.0])
            .show(ui, |ui| {
                ui.label(tr("editor.title"));
                changed |= ui
                    .add(TextEdit::singleline(&mut c.title).desired_width(420.0))
                    .changed();
                ui.end_row();
                ui.label(tr("editor.deck"));
                changed |= ui
                    .add(
                        TextEdit::singleline(&mut c.deck)
                            .hint_text(tr("editor.deck_hint"))
                            .desired_width(420.0),
                    )
                    .changed();
                ui.end_row();
                ui.label(tr("editor.deck_list"));
                let current = c
                    .deck_id
                    .and_then(|id| decks.iter().find(|d| d.id == id))
                    .map(|d| d.name.clone())
                    .unwrap_or_else(|| tr("common.none").into());
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("combo_deck")
                        .selected_text(current)
                        .width(250.0)
                        .show_ui(ui, |ui| {
                            changed |= ui
                                .selectable_value(&mut c.deck_id, None, tr("common.none"))
                                .changed();
                            for d in decks {
                                if ui
                                    .selectable_value(&mut c.deck_id, Some(d.id), &d.name)
                                    .changed()
                                {
                                    changed = true;
                                    if c.deck.is_empty() {
                                        c.deck = d.name.clone();
                                    }
                                }
                            }
                        });
                    ui.label(RichText::new(tr("editor.deck_list_hint")).weak());
                });
                ui.end_row();
                ui.label(tr("editor.author"));
                changed |= ui
                    .add(TextEdit::singleline(&mut c.author).desired_width(420.0))
                    .changed();
                ui.end_row();
                ui.label(tr("editor.notes"));
                changed |= ui
                    .add(
                        TextEdit::multiline(&mut c.notes)
                            .desired_rows(2)
                            .desired_width(420.0),
                    )
                    .changed();
                ui.end_row();
            });
        if changed {
            self.mark_dirty();
        }
        ui.horizontal(|ui| {
            if ui.button(tr("editor.start_practice")).clicked() {
                self.start_practice(idx);
                self.view = View::Practice;
            }
            if ui
                .button(tr("editor.share"))
                .on_hover_text(tr("editor.share_hint"))
                .clicked()
                && let Ok(text) = storage::combo_to_share_text(&self.combos[idx])
            {
                ui.ctx().copy_text(text);
                self.message = Some(tr("msg.combo_copied").into());
            }
            if ui
                .button(tr("textimport.copy"))
                .on_hover_text(tr("textimport.copy_hint"))
                .clicked()
            {
                ui.ctx()
                    .copy_text(crate::combotext::to_text(&self.combos[idx]));
                self.message = Some(tr("msg.combo_copied").into());
            }
        });
    }

    // -----------------------------------------------------------------------
    // Aufnahme auf dem Spielfeld
    // -----------------------------------------------------------------------

    /// Baut das Feld aus Starthand und allen gespeicherten Schritten auf.
    fn rebuild_board(&self, idx: usize) -> Board {
        // Züge, die nur zu einer Alternative gehören, liegen nicht auf dem Feld.
        let combo = &self.combos[idx].with_hand_choice(&[]);
        let mut b = Board::with_hand(&combo.starting_hand);
        for step in &combo.steps {
            for part in &step.parts {
                let kind = kind_of(&self.db, &part.entry.slot);
                let extra = is_extra(&self.db, &part.entry.slot);
                let mv = board::part_to_move(&b, &part.entry, kind, extra);
                let _ = b.apply(&mv, kind);
            }
        }
        b
    }

    fn start_recording(&mut self, idx: usize) {
        self.recorder = Some(Recorder {
            combo_id: self.combos[idx].id,
            board: self.rebuild_board(idx),
            field: FieldUi::default(),
            hand: self.combos[idx]
                .starting_hand
                .iter()
                .map(|e| e.slot.clone())
                .collect(),
        });
    }

    fn recorder_ui(&mut self, ui: &mut egui::Ui, idx: usize) {
        let combo_id = self.combos[idx].id;
        if self
            .recorder
            .as_ref()
            .is_some_and(|r| r.combo_id != combo_id)
        {
            self.recorder = None;
        }
        // Die Aufnahme läuft immer. Ändert sich die Starthand, wird das
        // Feld neu aufgebaut.
        let hand: Vec<Slot> = self.combos[idx]
            .starting_hand
            .iter()
            .map(|e| e.slot.clone())
            .collect();
        if self.recorder.as_ref().is_none_or(|r| r.hand != hand) {
            self.start_recording(idx);
        }

        // Werkzeugleiste
        let mut new_step = false;
        let mut undo = false;
        let mut take_endboard = false;
        let mut rebuild = false;
        // Strg+Z macht den letzten Zug rückgängig (nicht während der Texteingabe).
        if !ui.ctx().egui_wants_keyboard_input()
            && ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Z))
        {
            undo = self.combos[idx].steps.iter().any(|s| !s.parts.is_empty());
        }
        ui.horizontal_wrapped(|ui| {
            let steps = &self.combos[idx].steps;
            let current = steps.len().max(1);
            ui.label(
                RichText::new(trf("rec.current_step", &[&current]))
                    .strong()
                    .color(SELECT_COLOR),
            );
            new_step = ui
                .button(tr("rec.new_step"))
                .on_hover_text(tr("rec.new_step_hint"))
                .clicked();
            undo |= ui
                .add_enabled(
                    steps.iter().any(|s| !s.parts.is_empty()),
                    egui::Button::new(tr("rec.undo")),
                )
                .on_hover_text(tr("rec.undo_hint"))
                .clicked();
            take_endboard = ui
                .button(tr("rec.take_endboard"))
                .on_hover_text(tr("rec.take_endboard_hint"))
                .clicked();
            rebuild = ui
                .button(tr("rec.rebuild"))
                .on_hover_text(tr("rec.rebuild_hint"))
                .clicked();
        });

        // Notiz zum letzten Zug
        if let Some(part) = self.combos[idx]
            .steps
            .last_mut()
            .and_then(|s| s.parts.last_mut())
        {
            ui.horizontal(|ui| {
                ui.label(tr("rec.last_note"));
                if ui
                    .add(
                        TextEdit::singleline(&mut part.entry.note)
                            .hint_text(tr("rec.last_note_hint"))
                            .desired_width(300.0),
                    )
                    .changed()
                {
                    self.dirty = true;
                    self.last_change = std::time::Instant::now();
                }
            });
        }
        ui.add_space(4.0);

        // Spielfeld
        let width = 56.0;
        let v = CardView {
            db: &self.db,
            name_lang: self.name_lang(),
            ban: &self.ban_index,
        };
        let rec = self.recorder.as_mut().expect("recorder");
        let event = rec.field.show(
            ui,
            &rec.board,
            &mut self.images,
            v,
            width,
            self.settings.auto_zone,
        );

        match event {
            Some(FieldEvent::Pick(area)) => self.open_picker(Target::RecordFrom(area)),
            Some(FieldEvent::Move(mv)) => {
                let kind = kind_of(&self.db, &mv.slot);
                let rec = self.recorder.as_mut().expect("recorder");
                match rec.board.apply(&mv, kind) {
                    Ok(()) => {
                        let part = board::move_to_part(&mv, kind);
                        let steps = &mut self.combos[idx].steps;
                        if steps.is_empty() {
                            steps.push(Step::default());
                        }
                        steps.last_mut().expect("step").parts.push(part);
                        self.mark_dirty();
                    }
                    Err(e) => self.message = Some(tr(e.key()).into()),
                }
            }
            None => {}
        }

        if new_step {
            let steps = &mut self.combos[idx].steps;
            if steps.last().is_none_or(|s| !s.parts.is_empty()) {
                steps.push(Step::default());
                self.mark_dirty();
            }
        }
        if undo {
            let steps = &mut self.combos[idx].steps;
            while steps.last().is_some_and(|s| s.parts.is_empty()) {
                steps.pop();
            }
            if let Some(s) = steps.last_mut() {
                s.parts.pop();
                if s.parts.is_empty() {
                    steps.pop();
                }
            }
            self.mark_dirty();
            rebuild = true;
        }
        if take_endboard && let Some(rec) = &self.recorder {
            self.combos[idx].endboard = rec.board.field_entries();
            self.mark_dirty();
            self.message = Some(trf(
                "rec.endboard_taken",
                &[&self.combos[idx].endboard.len()],
            ));
        }
        if rebuild {
            let b = self.rebuild_board(idx);
            if let Some(rec) = &mut self.recorder {
                rec.board = b;
                rec.field.reset();
            }
        }
    }

    /// Kompakte Übersicht der Schritte (nur Anzeige).
    fn steps_overview(&mut self, ui: &mut egui::Ui, idx: usize) {
        ui.heading(tr("editor.steps"));
        let v = CardView {
            db: &self.db,
            name_lang: self.name_lang(),
            ban: &self.ban_index,
        };
        let combo = &self.combos[idx];
        if combo.steps.iter().all(|s| s.parts.is_empty()) {
            ui.label(RichText::new(tr("editor.no_steps")).weak());
        }
        let width = (self.settings.card_width * 0.85).max(44.0);
        for (si, step) in combo.steps.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.add_sized(
                    Vec2::new(34.0, width * CARD_RATIO),
                    egui::Label::new(RichText::new(format!("{}.", si + 1)).size(22.0).strong()),
                );
                step_row(ui, &mut self.images, v, step, width, &|_| None);
            });
            if !step.note.is_empty() {
                ui.label(RichText::new(&step.note).italics());
            }
        }
        if !combo.endboard.is_empty() {
            ui.separator();
            ui.label(RichText::new(Section::Endboard.label()).strong());
            ui.horizontal_wrapped(|ui| {
                for e in &combo.endboard {
                    entry_view(ui, &mut self.images, v, e, width, None);
                }
            });
        }
        ui.label(RichText::new(tr("editor.list_hint")).weak());
    }

    // -----------------------------------------------------------------------
    // Listen-Ansicht
    // -----------------------------------------------------------------------

    fn section_ui(&mut self, ui: &mut egui::Ui, idx: usize, section: Section) {
        ui.horizontal(|ui| {
            ui.heading(section.label());
            if section == Section::Required
                && ui
                    .small_button(tr("editor.required_from_steps"))
                    .on_hover_text(tr("editor.required_from_steps_hint"))
                    .clicked()
            {
                let combo = &mut self.combos[idx];
                let have: Vec<u32> = combo
                    .required
                    .iter()
                    .filter_map(|e| e.slot.card_id())
                    .collect();
                for e in combo.cards_used_in_steps_not_in_hand() {
                    if !e.slot.card_id().is_some_and(|id| have.contains(&id)) {
                        combo.required.push(e);
                    }
                }
                self.mark_dirty();
            }
            if section == Section::StartingHand && self.recorder.is_some() {
                ui.label(RichText::new(tr("editor.hand_click_hint")).weak());
            }
        });
        let width = self.settings.card_width;
        let v = CardView {
            db: &self.db,
            name_lang: self.name_lang(),
            ban: &self.ban_index,
        };
        let mut clicked = None;
        let mut open_picker = false;
        // Starthand: unter jeder Karte ihre Alternativen ("oder").
        let with_alts = section == Section::StartingHand;
        let mut add_alt = None;
        let mut remove_alt = None;
        ui.horizontal_wrapped(|ui| {
            for (i, e) in section.entries(&self.combos[idx]).iter().enumerate() {
                let hl = (self.selection == Some(Sel::Section(section, i))).then_some(SELECT_COLOR);
                if !with_alts {
                    if entry_view(ui, &mut self.images, v, e, width, hl).clicked() {
                        clicked = Some(i);
                    }
                    continue;
                }
                let size = egui::Vec2::new(width + 6.0, width * 1.46);
                ui.allocate_ui_with_layout(size, egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(width + 6.0);
                    if entry_view(ui, &mut self.images, v, e, width, hl).clicked() {
                        clicked = Some(i);
                    }
                    if !e.alternatives.is_empty() {
                        ui.label(RichText::new(tr("editor.alt_or")).weak());
                        let small = ((width - 4.0) / 2.0).max(28.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = egui::Vec2::splat(3.0);
                            for (ai, alt) in e.alternatives.iter().enumerate() {
                                let r = super::widgets::card_widget(
                                    ui,
                                    &mut self.images,
                                    v,
                                    alt,
                                    Default::default(),
                                    small,
                                    None,
                                );
                                if r.on_hover_text(tr("editor.alt_remove_hint")).clicked() {
                                    remove_alt = Some((i, ai));
                                }
                            }
                        });
                    }
                    if ui
                        .small_button(tr("editor.alt_add"))
                        .on_hover_text(tr("editor.alt_hint"))
                        .clicked()
                    {
                        add_alt = Some(i);
                    }
                });
            }
            if add_button(ui, width, tr("editor.add_card")) {
                open_picker = true;
            }
        });
        if let Some(i) = clicked
            && self.edit_mode == EditMode::Field
        {
            // Im Spielfeld-Modus entfernt ein Klick die Karte aus der Starthand.
            section.entries_mut(&mut self.combos[idx]).remove(i);
            self.mark_dirty();
        } else if let Some(i) = clicked {
            self.selection = Some(Sel::Section(section, i));
        }
        if let Some((i, ai)) = remove_alt {
            self.combos[idx].starting_hand[i].alternatives.remove(ai);
            self.mark_dirty();
        }
        if let Some(i) = add_alt {
            self.open_picker(Target::Alt(i));
        }
        if open_picker {
            self.open_picker(Target::Section(section));
        }
        ui.separator();
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
        ui.heading(tr("editor.steps"));
        let width = self.settings.card_width;
        let v = CardView {
            db: &self.db,
            name_lang: self.name_lang(),
            ban: &self.ban_index,
        };
        let mut op = None;
        let mut note_changed = false;
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
                                .on_hover_text(tr("editor.connector_hint"))
                                .clicked()
                            {
                                op = Some(StepOp::CycleConnector(si, pi));
                            }
                        }
                        let hl =
                            (self.selection == Some(Sel::StepPart(si, pi))).then_some(SELECT_COLOR);
                        if entry_view(ui, &mut self.images, v, &part.entry, width, hl).clicked() {
                            op = Some(StepOp::Select(si, pi));
                        }
                    }
                    if add_button(ui, width, tr("editor.add_card_to_step")) {
                        op = Some(StepOp::Pick(si));
                    }
                    ui.vertical(|ui| {
                        if ui
                            .small_button("↑")
                            .on_hover_text(tr("editor.step_up"))
                            .clicked()
                        {
                            op = Some(StepOp::Up(si));
                        }
                        if ui
                            .small_button("↓")
                            .on_hover_text(tr("editor.step_down"))
                            .clicked()
                        {
                            op = Some(StepOp::Down(si));
                        }
                        if ui.small_button(tr("common.delete")).clicked() {
                            op = Some(StepOp::Delete(si));
                        }
                    });
                });
            });
            let note = &mut self.combos[idx].steps[si].note;
            note_changed |= ui
                .add(
                    TextEdit::singleline(note)
                        .hint_text(tr("editor.step_note_hint"))
                        .desired_width(500.0),
                )
                .changed();
            ui.add_space(6.0);
        }
        if note_changed {
            self.mark_dirty();
        }
        if ui.button(tr("editor.add_step")).clicked() {
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
        self.recorder = None;
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
        let v = CardView {
            db: &self.db,
            name_lang: self.name_lang(),
            ban: &self.ban_index,
        };
        // Karten der Starthand, zwischen denen gewählt wird (für Züge, die
        // nur mit einer bestimmten Karte gespielt werden).
        let hand_options = match sel {
            Sel::StepPart(..) => self.combos[idx].hand_options(),
            Sel::Section(..) => Vec::new(),
        };
        let combo = &mut self.combos[idx];
        let (entry, connector, len, pos) = match sel {
            Sel::Section(sec, i) => {
                let list = sec.entries_mut(combo);
                let len = list.len();
                match list.get_mut(i) {
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
        ui.heading(tr("entry.title"));
        ui.add_space(4.0);
        card_widget(
            ui,
            &mut self.images,
            v,
            &entry.slot,
            Marks::of(entry),
            150.0,
            None,
        );
        ui.label(RichText::new(shown_name(v, &entry.slot)).strong());
        ui.add_space(6.0);

        if let Slot::Placeholder { kind, label } = &mut entry.slot {
            ui.label(tr("entry.placeholder_text"));
            changed |= ui.text_edit_singleline(label).changed();
            egui::ComboBox::from_id_salt("ph_kind")
                .selected_text(kind.label())
                .show_ui(ui, |ui| {
                    for k in CardKind::ALL {
                        changed |= ui.selectable_value(kind, k, k.label()).changed();
                    }
                });
        }

        ui.label(tr("field.action"));
        changed |= action_combo(ui, "entry_action", &mut entry.action);

        ui.label(tr("field.zone"));
        egui::ComboBox::from_id_salt("zone")
            .selected_text(entry.zone.label())
            .width(220.0)
            .show_ui(ui, |ui| {
                for z in Zone::ALL {
                    changed |= ui.selectable_value(&mut entry.zone, z, z.label()).changed();
                }
            });

        ui.label(tr("field.facing"));
        ui.horizontal_wrapped(|ui| {
            for f in Facing::ALL {
                changed |= ui
                    .selectable_value(&mut entry.facing, f, f.label())
                    .changed();
            }
        });

        if !hand_options.is_empty() || entry.only_with.is_some() {
            ui.label(tr("entry.only_with"))
                .on_hover_text(tr("entry.only_with_hint"));
            ui.horizontal_wrapped(|ui| {
                changed |= ui
                    .selectable_value(&mut entry.only_with, None, tr("entry.only_with_always"))
                    .changed();
                for slot in &hand_options {
                    changed |= ui
                        .selectable_value(
                            &mut entry.only_with,
                            Some(slot.clone()),
                            shown_name(v, slot),
                        )
                        .changed();
                }
            });
        }

        if let Some(conn) = connector {
            ui.label(tr("entry.connector"));
            ui.horizontal(|ui| {
                for c in Connector::ALL {
                    changed |= ui.selectable_value(conn, c, c.symbol()).changed();
                }
            });
        }

        ui.label(tr("entry.note"));
        changed |= ui
            .add(
                TextEdit::multiline(&mut entry.note)
                    .desired_rows(2)
                    .hint_text(tr("entry.note_hint")),
            )
            .changed();

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(pos > 0, egui::Button::new(tr("entry.left")))
                .clicked()
            {
                op = Some(EditOp::Left);
            }
            if ui
                .add_enabled(pos + 1 < len, egui::Button::new(tr("entry.right")))
                .clicked()
            {
                op = Some(EditOp::Right);
            }
        });
        ui.horizontal(|ui| {
            if ui.button(tr("entry.remove")).clicked() {
                op = Some(EditOp::Remove);
            }
            if ui.button(tr("common.close")).clicked() {
                op = Some(EditOp::Close);
            }
        });

        if changed {
            self.recorder = None;
            self.mark_dirty();
        }
        let Some(op) = op else { return };
        if let EditOp::Close = op {
            self.selection = None;
            return;
        }
        let combo = &mut self.combos[idx];
        // Gleiche Bewegung für beide Listenarten.
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
        self.recorder = None;
        self.mark_dirty();
    }
}
