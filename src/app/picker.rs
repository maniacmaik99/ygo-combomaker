//! Fenster „Karte suchen“: Deckkarten zum Anklicken, Suche in allen Karten
//! und Platzhalter.

use egui::{Align2, RichText, TextEdit, Vec2};

use super::widgets::{Marks, View as CardView, card_widget, is_extra};
use super::{ComboApp, Picker, Target};
use crate::board::Area;
use crate::i18n::{tr, trf};
use crate::model::{CardKind, Connector, Entry, Slot, StepPart};

impl ComboApp {
    pub(super) fn open_picker(&mut self, target: Target) {
        self.picker = Some(Picker {
            target,
            query: String::new(),
            placeholder_kind: CardKind::Monster,
            placeholder_label: "Any Card".into(),
            focus_requested: false,
        });
    }

    fn add_to_target(&mut self, target: Target, slot: Slot) {
        match target {
            Target::Deck => {
                if let (Some(i), Slot::Card { id, image_id, name }) = (self.selected_deck, &slot) {
                    self.decks[i].add(*id, *image_id, name, 1);
                    self.mark_dirty();
                }
            }
            Target::RecordFrom(area) => {
                if let Some(r) = &mut self.recorder {
                    r.field.select(slot, area);
                }
                self.picker = None;
            }
            Target::PracticeFrom(area) => {
                if let Some(p) = &mut self.practice {
                    p.field.select(slot, area);
                }
                self.picker = None;
            }
            Target::FreeFrom(area) => {
                self.hand_test.select_from_picker(slot, area);
                self.picker = None;
            }
            Target::Section(sec) => {
                if let Some(idx) = self.selected {
                    sec.entries_mut(&mut self.combos[idx])
                        .push(Entry::new(slot));
                    self.mark_dirty();
                }
            }
            Target::Step(si) => {
                if let Some(step) = self.selected.and_then(|i| self.combos[i].steps.get_mut(si)) {
                    step.parts.push(StepPart {
                        connector: Connector::Then,
                        entry: Entry::new(slot),
                    });
                    self.mark_dirty();
                }
            }
        }
    }

    pub(super) fn picker_window(&mut self, ctx: &egui::Context) {
        let v = CardView {
            db: &self.db,
            name_lang: self.name_lang(),
        };
        let Some(picker) = &mut self.picker else {
            return;
        };
        let mut open = true;
        let mut chosen: Option<Slot> = None;
        let md_only = &mut self.settings.md_only;
        let pw = self.settings.picker_card_width.clamp(40.0, 220.0);
        let images = &mut self.images;
        let db = v.db;
        let title = match picker.target {
            Target::Deck => tr("picker.title_deck"),
            Target::RecordFrom(Area::ExtraDeck)
            | Target::PracticeFrom(Area::ExtraDeck)
            | Target::FreeFrom(Area::ExtraDeck) => tr("picker.title_extra"),
            Target::RecordFrom(_) | Target::PracticeFrom(_) | Target::FreeFrom(_) => {
                tr("picker.title_deck_field")
            }
            _ => tr("picker.title_add"),
        };
        // Nur Extra-Deck-Karten (true), nur Main-Deck-Karten (false) oder alle (None).
        let extra_filter = match picker.target {
            Target::RecordFrom(Area::ExtraDeck)
            | Target::PracticeFrom(Area::ExtraDeck)
            | Target::FreeFrom(Area::ExtraDeck) => Some(true),
            Target::RecordFrom(_) | Target::PracticeFrom(_) | Target::FreeFrom(_) => Some(false),
            _ => None,
        };
        let deck = match picker.target {
            Target::Deck => None,
            Target::FreeFrom(_) => self
                .hand_test
                .deck_id()
                .and_then(|id| self.decks.iter().find(|d| d.id == id)),
            _ => self
                .selected
                .and_then(|i| self.combos[i].deck_id)
                .and_then(|id| self.decks.iter().find(|d| d.id == id)),
        };
        egui::Window::new(title)
            .open(&mut open)
            .default_size([(pw * 6.5).max(460.0), 560.0])
            .pivot(Align2::CENTER_TOP)
            .default_pos(ctx.content_rect().center_top() + Vec2::new(0.0, 60.0))
            .collapsible(false)
            .show(ctx, |ui| {
                let resp = ui.add(
                    TextEdit::singleline(&mut picker.query)
                        .hint_text(tr("picker.search_hint"))
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
                ui.checkbox(md_only, tr("settings.md_only_short"));
                let q = picker.query.trim().to_lowercase();

                if let Some(deck) = deck {
                    let cards: Vec<_> = deck
                        .cards
                        .iter()
                        .filter(|c| extra_filter.is_none_or(|e| is_extra(db, &c.to_slot()) == e))
                        .filter(|c| {
                            q.is_empty()
                                || c.name.to_lowercase().contains(&q)
                                || db.get(c.id).is_some_and(|x| {
                                    x.names.values().any(|n| n.to_lowercase().contains(&q))
                                })
                        })
                        .collect();
                    ui.label(RichText::new(trf("picker.from_deck", &[&deck.name])).strong());
                    egui::ScrollArea::vertical()
                        .id_salt("deck_cards")
                        .max_height((pw * 4.6).max(260.0))
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                for c in cards {
                                    let slot = c.to_slot();
                                    if card_widget(ui, images, v, &slot, Marks::default(), pw, None)
                                        .clicked()
                                    {
                                        chosen = Some(slot);
                                    }
                                }
                            });
                        });
                    ui.separator();
                    ui.label(RichText::new(tr("picker.all_cards")).strong());
                }

                let results = db.search(&picker.query, *md_only, 60);
                if db.is_empty() {
                    ui.label(tr("picker.db_not_loaded"));
                } else if q.is_empty() {
                    ui.label(tr("picker.type_hint"));
                } else if results.is_empty() {
                    ui.label(tr("picker.no_results"));
                }
                if resp.lost_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    && let Some(c) = results.first()
                {
                    chosen = Some(c.to_slot());
                    picker.query.clear();
                    picker.focus_requested = false;
                }

                egui::ScrollArea::vertical()
                    .id_salt("search_results")
                    .max_height(380.0)
                    .show(ui, |ui| {
                        for card in &results {
                            ui.horizontal(|ui| {
                                let slot = card.to_slot();
                                let thumb = card_widget(
                                    ui,
                                    images,
                                    v,
                                    &slot,
                                    Marks::default(),
                                    pw * 0.7,
                                    None,
                                );
                                let shown = card.localized_name(v.name_lang);
                                let name = if shown != card.name {
                                    format!("{shown}\n{}", card.name)
                                } else {
                                    card.name.clone()
                                };
                                let label = ui
                                    .selectable_label(false, format!("{name}\n{}", card.type_line));
                                if thumb.clicked() || label.clicked() {
                                    chosen = Some(slot);
                                }
                            });
                        }
                    });

                if matches!(picker.target, Target::Section(_) | Target::Step(_)) {
                    ui.separator();
                    ui.label(tr("picker.placeholder_hint"));
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("ph_new_kind")
                            .selected_text(picker.placeholder_kind.label())
                            .show_ui(ui, |ui| {
                                for k in CardKind::ALL {
                                    ui.selectable_value(&mut picker.placeholder_kind, k, k.label());
                                }
                            });
                        ui.add(
                            TextEdit::singleline(&mut picker.placeholder_label)
                                .desired_width(140.0),
                        );
                        if ui.button(tr("common.add")).clicked() {
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
}
