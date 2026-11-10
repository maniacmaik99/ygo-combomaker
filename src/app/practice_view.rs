//! Übungsmodus: Die Combo wird auf dem Spielfeld Zug für Zug nachgespielt.

use egui::RichText;

use super::field::{FieldEvent, FieldUi};
use super::widgets::{
    BAD_COLOR, GOOD_COLOR, SELECT_COLOR, View as CardView, entry_view, is_extra, kind_of, step_row,
};
use super::{ComboApp, Practice, Target};
use crate::i18n::{tr, trf};
use crate::practice::{Session, Verdict};

impl ComboApp {
    pub(super) fn start_practice(&mut self, idx: usize) {
        self.practice = Some(Practice {
            session: Session::new(&self.combos[idx]),
            field: FieldUi::default(),
            feedback: None,
        });
    }

    pub(super) fn practice_view(&mut self, ui: &mut egui::Ui) {
        let Some(idx) = self.selected else {
            ui.label(tr("practice.no_combo"));
            return;
        };
        let combo_id = self.combos[idx].id;
        if self
            .practice
            .as_ref()
            .is_some_and(|p| p.session.combo_id != combo_id)
        {
            self.practice = None;
        }

        if self.practice.is_none() {
            let c = &self.combos[idx];
            ui.heading(&c.title);
            if c.steps.iter().all(|s| s.parts.is_empty()) {
                ui.label(tr("practice.no_steps"));
                return;
            }
            // Die Übung startet sofort, ohne Zwischenseite.
            self.start_practice(idx);
        }

        // Ergebnis einmalig speichern, wenn die Übung fertig ist.
        if let Some(p) = &mut self.practice
            && p.session.finished()
            && !p.session.stats_saved
        {
            p.session.stats_saved = true;
            let percent = p.session.percent();
            let stats = &mut self.combos[idx].practice;
            stats.attempts += 1;
            stats.best_percent = stats.best_percent.max(percent);
            self.mark_dirty();
        }

        let width = (self.settings.card_width * 0.85).max(48.0);
        let v = CardView {
            db: &self.db,
            name_lang: self.name_lang(),
        };
        let combo = &self.combos[idx];
        let p = self.practice.as_mut().expect("practice");
        let auto_zone = self.settings.auto_zone;
        let mut hint = false;
        let mut reveal = false;
        let mut restart = false;
        let mut stop = false;
        let mut pick = None;

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading(&combo.title);
                ui.separator();
                let total = p.session.total();
                ui.label(trf(
                    "practice.progress",
                    &[&(p.session.done() + 1).min(total), &total],
                ));
                ui.separator();
                ui.label(trf("practice.mistakes", &[&p.session.mistakes]));
                ui.separator();
                ui.label(
                    RichText::new(trf(
                        "practice.stats",
                        &[&combo.practice.attempts, &combo.practice.best_percent],
                    ))
                    .weak(),
                );
                ui.separator();
                ui.checkbox(&mut p.session.show_endboard, tr("practice.show_endboard"));
                restart = ui.button(tr("practice.restart")).clicked();
                stop = ui.button(tr("practice.stop")).clicked();
            });
            ui.separator();

            if p.session.show_endboard && !combo.endboard.is_empty() {
                ui.label(RichText::new(tr("practice.target")).strong());
                ui.horizontal_wrapped(|ui| {
                    for e in &combo.endboard {
                        entry_view(ui, &mut self.images, v, e, width * 0.7, None);
                    }
                });
                ui.separator();
            }

            if p.session.finished() {
                ui.add_space(10.0);
                ui.heading(trf(
                    "practice.finished",
                    &[
                        &p.session.correct_count(),
                        &p.session.results.len(),
                        &p.session.percent(),
                    ],
                ));
                ui.horizontal(|ui| {
                    restart |= ui.button(tr("practice.again")).clicked();
                    stop |= ui.button(tr("practice.stop")).clicked();
                });
                ui.add_space(8.0);
                ui.label(RichText::new(tr("practice.solution")).strong());
                for (si, step) in combo.steps.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{}.", si + 1)).size(20.0).strong());
                        step_row(ui, &mut self.images, v, step, width * 0.7, &|_| None);
                    });
                }
                return;
            }

            // Aufgabe: aktueller Schritt und was darin schon erledigt ist.
            if let Some((si, pi)) = p.session.current() {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(trf("practice.step", &[&(si + 1), &combo.steps.len()]))
                            .size(20.0)
                            .strong()
                            .color(SELECT_COLOR),
                    );
                    ui.label(trf(
                        "practice.move_in_step",
                        &[&(pi + 1), &combo.steps[si].parts.len()],
                    ));
                    hint = ui
                        .button(tr("practice.hint"))
                        .on_hover_text(tr("practice.hint_hint"))
                        .clicked();
                    reveal = ui
                        .button(tr("practice.reveal"))
                        .on_hover_text(tr("practice.reveal_hint"))
                        .clicked();
                });
                if !combo.steps[si].note.is_empty() {
                    ui.label(RichText::new(&combo.steps[si].note).italics());
                }
                if pi > 0 {
                    let done = crate::model::Step {
                        parts: combo.steps[si].parts[..pi].to_vec(),
                        note: String::new(),
                    };
                    ui.horizontal(|ui| {
                        ui.label(tr("practice.done_in_step"));
                        step_row(ui, &mut self.images, v, &done, width * 0.6, &|_| {
                            Some(GOOD_COLOR)
                        });
                    });
                }
            }
            if let Some((text, ok)) = &p.feedback {
                let color = if *ok { GOOD_COLOR } else { BAD_COLOR };
                ui.label(RichText::new(text).color(color).size(16.0).strong());
            } else {
                // Platz frei halten, damit das Spielfeld nicht springt.
                ui.label(RichText::new(" ").size(16.0));
            }
            ui.add_space(4.0);

            let event = p
                .field
                .show(ui, &p.session.board, &mut self.images, v, width, auto_zone);
            match event {
                Some(FieldEvent::Pick(area)) => {
                    p.feedback = None;
                    pick = Some(area);
                }
                Some(FieldEvent::Move(mv)) => {
                    let kind = kind_of(v.db, &mv.slot);
                    p.feedback = Some(match p.session.submit(combo, &mv, kind) {
                        Ok(Verdict::Correct) => (tr("practice.correct").into(), true),
                        Ok(Verdict::Wrong) => (trf("practice.wrong", &[&mv.action.label()]), false),
                        Err(e) => (tr(e.key()).into(), false),
                    });
                }
                None => {}
            }
        });

        // Picker öffnen (außerhalb der Ausleihe von self.practice).
        if let Some(area) = pick {
            self.open_picker(Target::PracticeFrom(area));
        }

        let name_lang = self.name_lang();
        if hint
            && let Some(p) = &mut self.practice
            && let Some(slot) = p.session.hint(&self.combos[idx])
        {
            let name = super::widgets::shown_name(
                CardView {
                    db: &self.db,
                    name_lang,
                },
                &slot,
            );
            p.feedback = Some((trf("practice.hint_text", &[&name]), false));
            p.field.hint = Some(slot);
        }
        if reveal {
            let db = &self.db;
            let combo = &self.combos[idx];
            if let Some(p) = &mut self.practice
                && let Some(mv) = p
                    .session
                    .reveal(combo, |s| (kind_of(db, s), is_extra(db, s)))
            {
                let name = super::widgets::shown_name(
                    CardView {
                        db,
                        name_lang: None,
                    },
                    &mv.slot,
                );
                p.feedback = Some((
                    trf("practice.revealed", &[&name, &mv.action.label()]),
                    false,
                ));
                p.field.reset();
            }
        }
        if restart {
            self.start_practice(idx);
        }
        if stop {
            self.practice = None;
            self.view = super::View::Edit;
        }
    }
}
