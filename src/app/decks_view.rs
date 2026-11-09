//! Decklisten: anlegen, aus Archetyp füllen, importieren, löschen.

use egui::{Align2, RichText, TextEdit};

use super::widgets::{Marks, View as CardView, card_widget};
use super::{ComboApp, Target};
use crate::decks;
use crate::i18n::{tr, trf};
use crate::model::Deck;

impl ComboApp {
    pub(super) fn decks_view(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("deck_list")
            .resizable(true)
            .default_size(200.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.button(tr("deck.new")).clicked() {
                        self.decks.push(Deck::new(tr("deck.new_name")));
                        self.selected_deck = Some(self.decks.len() - 1);
                        self.mark_dirty();
                    }
                    if ui
                        .button(tr("common.import"))
                        .on_hover_text(tr("deck.import_hint"))
                        .clicked()
                    {
                        self.deck_import = Some((String::new(), String::new()));
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (i, d) in self.decks.iter().enumerate() {
                        let text = format!("{}\n{}", d.name, trf("deck.card_count", &[&d.total()]));
                        if ui
                            .selectable_label(self.selected_deck == Some(i), text)
                            .clicked()
                        {
                            self.selected_deck = Some(i);
                        }
                    }
                });
            });

        let Some(di) = self.selected_deck.filter(|&i| i < self.decks.len()) else {
            ui.label(tr("deck.none_selected"));
            ui.add_space(8.0);
            ui.label(tr("deck.explain"));
            return;
        };

        let width = self.settings.card_width;
        let v = CardView {
            db: &self.db,
            name_lang: self.name_lang(),
        };
        let mut changed = false;
        let mut remove_one = None;
        let mut add_one = None;
        let mut fill_archetype = None;
        let mut open_picker = false;

        egui::ScrollArea::vertical().show(ui, |ui| {
            let deck = &mut self.decks[di];
            ui.horizontal(|ui| {
                ui.label(tr("common.name"));
                changed |= ui
                    .add(TextEdit::singleline(&mut deck.name).desired_width(300.0))
                    .changed();
                ui.label(trf("deck.card_count", &[&deck.total()]));
            });
            ui.horizontal(|ui| {
                if ui.button(tr("deck.add_card")).clicked() {
                    open_picker = true;
                }
                if ui
                    .button(tr("deck.copy_ydke"))
                    .on_hover_text(tr("deck.copy_ydke_hint"))
                    .clicked()
                {
                    ui.ctx().copy_text(decks::to_ydke(deck));
                    self.message = Some(tr("msg.ydke_copied").into());
                }
                if ui.button(tr("deck.delete")).clicked() {
                    self.confirm_delete_deck = Some(di);
                }
            });

            ui.add_space(6.0);
            ui.label(RichText::new(tr("deck.archetype_title")).strong());
            ui.add(
                TextEdit::singleline(&mut self.archetype_query)
                    .hint_text(tr("deck.archetype_hint"))
                    .desired_width(300.0),
            );
            let q = self.archetype_query.trim().to_lowercase();
            if !q.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    for a in self
                        .db
                        .archetypes()
                        .into_iter()
                        .filter(|a| a.to_lowercase().contains(&q))
                        .take(12)
                    {
                        let n = self.db.cards_of_archetype(&a, self.settings.md_only).len();
                        if ui.button(format!("{a} ({n})")).clicked() {
                            fill_archetype = Some(a);
                        }
                    }
                });
            }

            ui.separator();
            ui.label(RichText::new(tr("deck.click_hint")).weak());
            let deck = &self.decks[di];
            ui.horizontal_wrapped(|ui| {
                for c in &deck.cards {
                    ui.vertical(|ui| {
                        ui.set_width(width + 6.0);
                        let slot = c.to_slot();
                        let r = card_widget(
                            ui,
                            &mut self.images,
                            v,
                            &slot,
                            Marks::default(),
                            width,
                            None,
                        );
                        if r.clicked() {
                            remove_one = Some(c.id);
                        }
                        if r.secondary_clicked() {
                            add_one = Some(c.clone());
                        }
                        ui.label(RichText::new(format!("{}×", c.count)).strong());
                    });
                }
            });
        });

        let deck = &mut self.decks[di];
        if let Some(id) = remove_one {
            deck.remove_one(id);
            changed = true;
        }
        if let Some(c) = add_one {
            deck.add(c.id, c.image_id, &c.name, 1);
            changed = true;
        }
        if let Some(a) = fill_archetype {
            for c in self.db.cards_of_archetype(&a, self.settings.md_only) {
                if !deck.cards.iter().any(|x| x.id == c.id) {
                    deck.add(c.id, c.image_id, &c.name, 1);
                }
            }
            if deck.name == tr("deck.new_name") {
                deck.name = a.clone();
            }
            self.archetype_query.clear();
            changed = true;
        }
        if changed {
            self.mark_dirty();
        }
        if open_picker {
            self.open_picker(Target::Deck);
        }
    }

    pub(super) fn deck_import_window(&mut self, ctx: &egui::Context) {
        let Some((name, text)) = &mut self.deck_import else {
            return;
        };
        let mut open = true;
        let mut do_import = false;
        egui::Window::new(tr("deck.import_title"))
            .open(&mut open)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(tr("common.name"));
                    ui.add(
                        TextEdit::singleline(name)
                            .hint_text(tr("editor.deck_hint"))
                            .desired_width(250.0),
                    );
                });
                ui.label(tr("deck.import_paste"));
                ui.label(tr("deck.import_fmt_ydke"));
                ui.label(tr("deck.import_fmt_ydk"));
                ui.label(tr("deck.import_fmt_list"));
                egui::ScrollArea::vertical()
                    .max_height(260.0)
                    .show(ui, |ui| {
                        ui.add(
                            TextEdit::multiline(text)
                                .desired_rows(10)
                                .desired_width(420.0)
                                .code_editor(),
                        );
                    });
                if ui.button(tr("common.import")).clicked() {
                    do_import = true;
                }
            });
        if do_import {
            if self.db.is_empty() {
                self.message = Some(tr("msg.db_not_ready").into());
                return;
            }
            let name = if name.trim().is_empty() {
                tr("deck.imported_name").to_string()
            } else {
                name.trim().to_string()
            };
            let result = decks::import(text, &name, &self.db);
            let total = result.deck.total();
            self.decks.push(result.deck);
            self.selected_deck = Some(self.decks.len() - 1);
            self.deck_import = None;
            self.mark_dirty();
            let mut msg = trf("deck.imported", &[&total]);
            if !result.unknown.is_empty() {
                msg.push_str("\n\n");
                msg.push_str(&trf("deck.not_found", &[&result.unknown.len()]));
                msg.push('\n');
                msg.push_str(&result.unknown.join("\n"));
            }
            self.message = Some(msg);
            return;
        }
        if !open {
            self.deck_import = None;
        }
    }

    pub(super) fn delete_deck_window(&mut self, ctx: &egui::Context) {
        let Some(i) = self.confirm_delete_deck else {
            return;
        };
        let Some(deck) = self.decks.get(i) else {
            self.confirm_delete_deck = None;
            return;
        };
        let name = deck.name.clone();
        let mut answer = None;
        egui::Window::new(tr("deck.delete_title"))
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(trf("deck.delete_text", &[&name]));
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
                let id = self.decks.remove(i).id;
                for c in &mut self.combos {
                    if c.deck_id == Some(id) {
                        c.deck_id = None;
                    }
                }
                self.selected_deck = if self.decks.is_empty() {
                    None
                } else {
                    Some(i.min(self.decks.len() - 1))
                };
                self.confirm_delete_deck = None;
                self.mark_dirty();
            }
            Some(false) => self.confirm_delete_deck = None,
            None => {}
        }
    }
}
