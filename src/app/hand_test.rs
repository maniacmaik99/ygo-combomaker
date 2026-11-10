//! Handtest: zufällige Starthand aus einem Deck ziehen und prüfen, welche
//! gespeicherten Combos damit spielbar sind. Dazu eine einfache Statistik
//! über viele zufällige Hände.

use egui::RichText;
use uuid::Uuid;

use super::widgets::{Marks, View as CardView, card_widget, is_extra};
use super::{ComboApp, View};
use crate::i18n::{tr, trf};
use crate::model::{Combo, Deck, Entry, Slot};

/// Anzahl der Hände für die Statistik.
const SIM_RUNS: u32 = 1000;

// ---------------------------------------------------------------------------
// Reine Logik (ohne Oberfläche)
// ---------------------------------------------------------------------------

/// Kleiner Zufallsgenerator (xorshift64*). Reicht zum Mischen von Karten.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        // Der Zustand darf nie 0 sein, sonst liefert xorshift nur Nullen.
        Self(if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        })
    }

    /// Startwert aus einer zufälligen UUID.
    pub fn from_entropy() -> Self {
        let v = Uuid::new_v4().as_u128();
        Self::new((v as u64) ^ ((v >> 64) as u64))
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Zufallszahl in `0..n`. Für `n == 0` kommt 0 zurück.
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        // Die oberen Bits sind bei xorshift64* die besseren.
        ((self.next_u64() >> 11) % n as u64) as usize
    }
}

/// Zieht `n` verschiedene Positionen aus `0..len` (ohne Zurücklegen).
/// Ist `n` größer als `len`, kommen alle Positionen zurück.
pub fn draw_indices(len: usize, n: usize, rng: &mut Rng) -> Vec<usize> {
    let n = n.min(len);
    let mut all: Vec<usize> = (0..len).collect();
    // Teilweises Mischen (Fisher-Yates): nur die ersten `n` Plätze.
    for i in 0..n {
        let j = i + rng.below(len - i);
        all.swap(i, j);
    }
    all.truncate(n);
    all
}

/// Starthand einer Combo für die Prüfung: `Some(id)` = echte Karte,
/// `None` = Platzhalter (passt auf jede übrige Karte).
pub fn needs_of(starting_hand: &[Entry]) -> Vec<Option<u32>> {
    starting_hand.iter().map(|e| e.slot.card_id()).collect()
}

/// Ist die Combo mit dieser Hand spielbar? Jede echte Karte muss in der
/// Hand sein (mehrfache Karten entsprechend oft). Jeder Platzhalter braucht
/// eine weitere, noch nicht verwendete Karte. Eine leere Starthand ist nie
/// spielbar.
pub fn is_playable(needs: &[Option<u32>], hand: &[u32]) -> bool {
    if needs.is_empty() || needs.len() > hand.len() {
        return false;
    }
    let mut rest = hand.to_vec();
    for id in needs.iter().flatten() {
        match rest.iter().position(|h| h == id) {
            Some(i) => {
                rest.swap_remove(i);
            }
            None => return false,
        }
    }
    // Die Platzhalter passen, weil `needs.len() <= hand.len()` gilt.
    true
}

/// Ergebnis einer Simulation.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SimCounts {
    pub runs: u32,
    /// Für jede Combo (gleiche Reihenfolge wie die Eingabe): Anzahl der
    /// Hände, mit denen sie spielbar war.
    pub per_combo: Vec<u32>,
    /// Anzahl der Hände mit mindestens einer spielbaren Combo.
    pub any: u32,
}

/// Zieht `runs` zufällige Hände aus `pool` (Karten-IDs des Main Decks, jede
/// Kopie einzeln) und zählt, wie oft jede Combo spielbar war.
pub fn simulate(
    pool: &[u32],
    needs: &[Vec<Option<u32>>],
    hand_size: usize,
    runs: u32,
    rng: &mut Rng,
) -> SimCounts {
    let mut out = SimCounts {
        runs,
        per_combo: vec![0; needs.len()],
        any: 0,
    };
    for _ in 0..runs {
        let hand: Vec<u32> = draw_indices(pool.len(), hand_size, rng)
            .into_iter()
            .map(|i| pool[i])
            .collect();
        let mut any = false;
        for (count, need) in out.per_combo.iter_mut().zip(needs) {
            if is_playable(need, &hand) {
                *count += 1;
                any = true;
            }
        }
        if any {
            out.any += 1;
        }
    }
    out
}

/// Positionen der spielbaren Combos. Zuerst die Combos, die mit dem Deck
/// verknüpft sind, danach die anderen; sonst in der Reihenfolge der Liste.
pub fn playable_combos(combos: &[Combo], deck_id: Uuid, hand: &[u32]) -> Vec<usize> {
    let mut out: Vec<usize> = combos
        .iter()
        .enumerate()
        .filter(|(_, c)| is_playable(&needs_of(&c.starting_hand), hand))
        .map(|(i, _)| i)
        .collect();
    // `sort_by_key` ist stabil: die Reihenfolge der Liste bleibt erhalten.
    out.sort_by_key(|&i| combos[i].deck_id != Some(deck_id));
    out
}

/// Anteil in Prozent mit einer Nachkommastelle. `comma`: Komma statt Punkt
/// (alle Sprachen außer Englisch).
fn percent(count: u32, runs: u32, comma: bool) -> String {
    let p = if runs == 0 {
        0.0
    } else {
        count as f64 * 100.0 / runs as f64
    };
    let s = format!("{p:.1}");
    if comma { s.replace('.', ",") } else { s }
}

// ---------------------------------------------------------------------------
// Zustand der Ansicht
// ---------------------------------------------------------------------------

/// Ergebnis der letzten Simulation für die Anzeige.
struct SimResult {
    deck_id: Uuid,
    hand_size: usize,
    runs: u32,
    /// (ID der Combo, Anzahl der Hände, mit denen sie spielbar war)
    per_combo: Vec<(Uuid, u32)>,
    any: u32,
}

pub struct HandTestState {
    /// Gewähltes Deck. `None` = noch keins gewählt (dann das erste).
    deck_id: Option<Uuid>,
    hand_size: usize,
    /// Gezogene Hand und das Deck, aus dem sie stammt.
    hand: Vec<Slot>,
    hand_deck: Option<Uuid>,
    sim: Option<SimResult>,
    rng: Rng,
}

impl Default for HandTestState {
    fn default() -> Self {
        Self {
            deck_id: None,
            hand_size: 5,
            hand: Vec::new(),
            hand_deck: None,
            sim: None,
            rng: Rng::from_entropy(),
        }
    }
}

// ---------------------------------------------------------------------------
// Oberfläche
// ---------------------------------------------------------------------------

/// Was nach dem Zeichnen mit einer Combo passieren soll.
enum Jump {
    Practice(usize),
    Open(usize),
}

impl ComboApp {
    /// Karten des Main Decks, jede Kopie einzeln. Extra-Deck-Monster fehlen.
    fn main_deck_pool(&self, deck: &Deck) -> Vec<Slot> {
        let mut pool = Vec::new();
        for c in &deck.cards {
            let slot = c.to_slot();
            if is_extra(&self.db, &slot) {
                continue;
            }
            for _ in 0..c.count {
                pool.push(slot.clone());
            }
        }
        pool
    }

    pub(super) fn hand_test_view(&mut self, ui: &mut egui::Ui) {
        ui.heading(tr("view.hand_test"));
        if self.decks.is_empty() {
            ui.label(tr("handtest.no_decks"));
            return;
        }
        ui.label(RichText::new(tr("handtest.intro")).weak());
        ui.add_space(6.0);

        // Gewähltes Deck bestimmen. Wurde es gelöscht, gilt wieder das
        // Deck aus der Ansicht „Decks“ oder das erste.
        let di = self
            .hand_test
            .deck_id
            .and_then(|id| self.decks.iter().position(|d| d.id == id))
            .or(self.selected_deck.filter(|&i| i < self.decks.len()))
            .unwrap_or(0);
        let mut new_di = di;
        let pool = self.main_deck_pool(&self.decks[di]);
        let mut draw = false;
        let mut run_sim = false;

        ui.horizontal(|ui| {
            ui.label(tr("handtest.deck"));
            egui::ComboBox::from_id_salt("hand_test_deck")
                .selected_text(&self.decks[di].name)
                .width(220.0)
                .show_ui(ui, |ui| {
                    for (i, d) in self.decks.iter().enumerate() {
                        ui.selectable_value(&mut new_di, i, &d.name);
                    }
                });
            ui.label(RichText::new(trf("handtest.main_deck_count", &[&pool.len()])).weak());
        });
        if new_di != di {
            // Anderes Deck: die alte Hand und die alte Statistik passen
            // nicht mehr. Im nächsten Bild wird das neue Deck gezeigt.
            self.hand_test.deck_id = Some(self.decks[new_di].id);
            self.hand_test.hand.clear();
            self.hand_test.hand_deck = None;
            self.hand_test.sim = None;
            ui.ctx().request_repaint();
            return;
        }
        let deck_id = self.decks[di].id;
        self.hand_test.deck_id = Some(deck_id);
        if self.hand_test.hand_deck.is_some_and(|id| id != deck_id) {
            self.hand_test.hand.clear();
            self.hand_test.hand_deck = None;
        }

        if pool.is_empty() {
            ui.label(tr("handtest.deck_empty"));
            return;
        }

        ui.horizontal(|ui| {
            ui.label(tr("handtest.hand_size"));
            ui.add(egui::DragValue::new(&mut self.hand_test.hand_size).range(1..=10));
            draw = ui.button(tr("handtest.draw")).clicked();
            run_sim = ui
                .button(trf("handtest.simulate", &[&SIM_RUNS]))
                .on_hover_text(tr("handtest.simulate_hint"))
                .clicked();
        });
        let hand_size = self.hand_test.hand_size.clamp(1, 10);

        if draw {
            self.hand_test.hand = draw_indices(pool.len(), hand_size, &mut self.hand_test.rng)
                .into_iter()
                .map(|i| pool[i].clone())
                .collect();
            self.hand_test.hand_deck = Some(deck_id);
        }
        if run_sim {
            let ids: Vec<u32> = pool.iter().filter_map(Slot::card_id).collect();
            let with_hand: Vec<&Combo> = self
                .combos
                .iter()
                .filter(|c| !c.starting_hand.is_empty())
                .collect();
            let needs: Vec<Vec<Option<u32>>> = with_hand
                .iter()
                .map(|c| needs_of(&c.starting_hand))
                .collect();
            let counts = simulate(&ids, &needs, hand_size, SIM_RUNS, &mut self.hand_test.rng);
            self.hand_test.sim = Some(SimResult {
                deck_id,
                hand_size,
                runs: counts.runs,
                per_combo: with_hand
                    .iter()
                    .map(|c| c.id)
                    .zip(counts.per_combo)
                    .collect(),
                any: counts.any,
            });
        }

        let width = self.settings.card_width;
        let v = CardView {
            db: &self.db,
            name_lang: self.name_lang(),
        };
        let mut jump = None;
        let comma = crate::i18n::lang() != crate::i18n::Lang::En;

        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            // --- Hand ---
            if self.hand_test.hand.is_empty() {
                ui.label(RichText::new(tr("handtest.no_hand")).weak());
            } else {
                ui.label(RichText::new(tr("handtest.hand")).strong());
                ui.horizontal_wrapped(|ui| {
                    for slot in &self.hand_test.hand {
                        card_widget(ui, &mut self.images, v, slot, Marks::default(), width, None);
                    }
                });
                ui.add_space(8.0);

                // --- Spielbare Combos ---
                ui.label(RichText::new(tr("handtest.playable")).strong());
                let ids: Vec<u32> = self
                    .hand_test
                    .hand
                    .iter()
                    .filter_map(Slot::card_id)
                    .collect();
                let playable = playable_combos(&self.combos, deck_id, &ids);
                if playable.is_empty() {
                    ui.label(RichText::new(tr("handtest.none_playable")).weak());
                }
                for i in playable {
                    let c = &self.combos[i];
                    ui.horizontal(|ui| {
                        if ui.button(tr("view.practice")).clicked() {
                            jump = Some(Jump::Practice(i));
                        }
                        if ui.button(tr("handtest.open")).clicked() {
                            jump = Some(Jump::Open(i));
                        }
                        ui.label(combo_title(c));
                        if c.deck_id == Some(deck_id) {
                            ui.label(RichText::new(tr("handtest.linked")).weak());
                        }
                    });
                }
            }

            // --- Statistik ---
            if let Some(sim) = self.hand_test.sim.as_ref().filter(|s| s.deck_id == deck_id) {
                ui.add_space(8.0);
                ui.separator();
                ui.label(
                    RichText::new(trf("handtest.stats_title", &[&sim.runs, &sim.hand_size]))
                        .strong(),
                );
                if sim.per_combo.is_empty() {
                    ui.label(RichText::new(tr("handtest.stats_no_combos")).weak());
                } else {
                    egui::Grid::new("hand_test_stats")
                        .num_columns(2)
                        .striped(true)
                        .show(ui, |ui| {
                            for (id, count) in &sim.per_combo {
                                // Inzwischen gelöschte Combos überspringen.
                                let Some(c) = self.combos.iter().find(|c| c.id == *id) else {
                                    continue;
                                };
                                ui.label(combo_title(c));
                                ui.label(trf(
                                    "handtest.stats_percent",
                                    &[&percent(*count, sim.runs, comma)],
                                ));
                                ui.end_row();
                            }
                        });
                    ui.add_space(4.0);
                    ui.label(trf(
                        "handtest.stats_any",
                        &[&percent(sim.any, sim.runs, comma)],
                    ));
                }
            }
        });

        match jump {
            Some(Jump::Practice(i)) => {
                self.select_combo(Some(i));
                self.start_practice(i);
                self.view = View::Practice;
            }
            Some(Jump::Open(i)) => {
                self.select_combo(Some(i));
                self.view = View::Edit;
            }
            None => {}
        }
    }
}

fn combo_title(c: &Combo) -> &str {
    if c.title.is_empty() {
        tr("combo.untitled")
    } else {
        &c.title
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CardKind;

    fn card(id: u32) -> Entry {
        Entry::new(Slot::Card {
            id,
            image_id: id,
            name: format!("Card {id}"),
        })
    }

    fn any_card() -> Entry {
        Entry::new(Slot::Placeholder {
            kind: CardKind::Monster,
            label: "Any Card".into(),
        })
    }

    #[test]
    fn rng_is_deterministic_and_in_range() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
        let mut r = Rng::new(0);
        assert_ne!(r.next_u64(), 0);
        for n in 1..50 {
            assert!(r.below(n) < n);
        }
        assert_eq!(r.below(0), 0);
    }

    #[test]
    fn draw_has_no_duplicates() {
        let mut rng = Rng::new(7);
        for _ in 0..200 {
            let mut d = draw_indices(40, 5, &mut rng);
            assert_eq!(d.len(), 5);
            assert!(d.iter().all(|&i| i < 40));
            d.sort_unstable();
            d.dedup();
            assert_eq!(d.len(), 5);
        }
    }

    #[test]
    fn draw_from_small_deck_returns_everything() {
        let mut rng = Rng::new(7);
        let mut d = draw_indices(3, 5, &mut rng);
        d.sort_unstable();
        assert_eq!(d, vec![0, 1, 2]);
        assert!(draw_indices(0, 5, &mut rng).is_empty());
    }

    #[test]
    fn draw_reaches_every_position() {
        let mut rng = Rng::new(123);
        let mut seen = [false; 10];
        for _ in 0..500 {
            for i in draw_indices(10, 2, &mut rng) {
                seen[i] = true;
            }
        }
        assert!(seen.iter().all(|s| *s));
    }

    #[test]
    fn playable_needs_every_card() {
        let needs = needs_of(&[card(1), card(2)]);
        assert!(is_playable(&needs, &[5, 2, 9, 1, 7]));
        assert!(!is_playable(&needs, &[5, 2, 9, 3, 7]));
    }

    #[test]
    fn playable_respects_multiplicity() {
        let needs = needs_of(&[card(1), card(1)]);
        assert!(!is_playable(&needs, &[1, 2, 3, 4, 5]));
        assert!(is_playable(&needs, &[1, 2, 1, 4, 5]));
    }

    #[test]
    fn placeholder_matches_any_remaining_card() {
        let needs = needs_of(&[card(1), any_card()]);
        assert!(is_playable(&needs, &[1, 2]));
        // Nur eine Karte in der Hand: für den Platzhalter bleibt keine übrig.
        assert!(!is_playable(&needs, &[1]));
        // Die echte Karte fehlt.
        assert!(!is_playable(&needs, &[2, 3]));
    }

    #[test]
    fn empty_starting_hand_is_not_playable() {
        assert!(!is_playable(&[], &[1, 2, 3]));
    }

    #[test]
    fn linked_combos_come_first() {
        let deck = Uuid::new_v4();
        let mut a = Combo::new("not linked");
        a.starting_hand.push(card(1));
        let mut b = Combo::new("linked");
        b.starting_hand.push(card(1));
        b.deck_id = Some(deck);
        let mut c = Combo::new("not playable");
        c.starting_hand.push(card(99));
        let d = Combo::new("empty hand");
        let combos = vec![a, b, c, d];
        assert_eq!(playable_combos(&combos, deck, &[1, 2, 3]), vec![1, 0]);
    }

    #[test]
    fn simulate_counts_certain_and_impossible_combos() {
        let mut rng = Rng::new(99);
        // Deck aus fünf verschiedenen Karten, Hand mit fünf Karten:
        // jede Hand enthält alle Karten.
        let pool = [1, 2, 3, 4, 5];
        let needs = vec![
            needs_of(&[card(1), card(2)]),
            needs_of(&[card(6)]),
            needs_of(&[card(1), card(1)]),
        ];
        let r = simulate(&pool, &needs, 5, 100, &mut rng);
        assert_eq!(r.runs, 100);
        assert_eq!(r.per_combo, vec![100, 0, 0]);
        assert_eq!(r.any, 100);
    }

    #[test]
    fn simulate_is_close_to_the_exact_probability() {
        let mut rng = Rng::new(2026);
        // 40 Karten, davon 3 Kopien von Karte 1. Mindestens eine Kopie in
        // 5 Karten: 1 - C(37,5)/C(40,5) = 33,76 %.
        let mut pool = vec![1, 1, 1];
        pool.extend(100..137);
        let needs = vec![needs_of(&[card(1)])];
        let r = simulate(&pool, &needs, 5, 20_000, &mut rng);
        let p = r.per_combo[0] as f64 / r.runs as f64;
        assert!((p - 0.3376).abs() < 0.02, "p = {p}");
        assert_eq!(r.any, r.per_combo[0]);
    }

    #[test]
    fn percent_has_one_decimal() {
        assert_eq!(percent(337, 1000, false), "33.7");
        assert_eq!(percent(337, 1000, true), "33,7");
        assert_eq!(percent(0, 0, false), "0.0");
    }
}
