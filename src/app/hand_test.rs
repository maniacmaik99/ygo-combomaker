//! Handtest: zufällige Starthand aus einem Deck ziehen und prüfen, welche
//! gespeicherten Combos damit spielbar sind. Dazu eine einfache Statistik
//! über viele zufällige Hände.

use egui::RichText;
use uuid::Uuid;

use super::field::{FieldEvent, FieldUi};
use super::widgets::{Marks, View as CardView, card_widget, is_extra, kind_of};
use super::{ComboApp, Target, View};
use crate::board::{Area, Board};
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

/// Was eine Combo an einer Stelle der Starthand braucht: eine der genannten
/// Karten (Hauptkarte und ihre Alternativen). Leer = Platzhalter, passt auf
/// jede übrige Karte.
pub type Need = Vec<u32>;

/// Starthand einer Combo für die Prüfung.
pub fn needs_of(starting_hand: &[Entry]) -> Vec<Need> {
    starting_hand.iter().map(need_of).collect()
}

fn need_of(e: &Entry) -> Need {
    let mut ids = Vec::with_capacity(1 + e.alternatives.len());
    for slot in std::iter::once(&e.slot).chain(&e.alternatives) {
        match slot.card_id() {
            Some(id) => ids.push(id),
            // Ein Platzhalter als Karte oder Alternative: jede Karte passt.
            None => return Vec::new(),
        }
    }
    ids
}

/// Die Karten, die eine Combo wirklich auf der Starthand braucht:
/// nur die Karten der Starthand, die in den Schritten auch benutzt werden.
/// Karten, die nur „dabei“ sind (z. B. eine eingetragene Hand mit 5 Karten,
/// von denen die Combo 2 benutzt), zählen nicht. Ein Platzhalter zählt,
/// wenn in den Schritten ein Platzhalter vorkommt.
/// Benutzt die Combo keine Karte der Starthand (z. B. noch keine Schritte),
/// gilt die ganze Starthand.
pub fn needs_of_combo(combo: &Combo) -> Vec<Need> {
    let used: Vec<&Slot> = combo
        .steps
        .iter()
        .flat_map(|s| &s.parts)
        .map(|p| &p.entry.slot)
        .collect();
    let any_placeholder = used.iter().any(|s| s.card_id().is_none());
    let needed: Vec<Need> = combo
        .starting_hand
        .iter()
        .filter(|e| match e.slot.card_id() {
            Some(id) => used.iter().any(|s| s.card_id() == Some(id)),
            None => any_placeholder,
        })
        .map(need_of)
        .collect();
    if needed.is_empty() {
        needs_of(&combo.starting_hand)
    } else {
        needed
    }
}

/// Ist die Combo mit dieser Hand spielbar? Für jede Stelle der Starthand
/// muss eine eigene Karte der Hand passen (mehrfache Karten entsprechend
/// oft). Jeder Platzhalter braucht eine weitere, noch nicht verwendete
/// Karte. Eine leere Starthand ist nie spielbar.
pub fn is_playable(needs: &[Need], hand: &[u32]) -> bool {
    if needs.is_empty() || needs.len() > hand.len() {
        return false;
    }
    // Stellen mit Alternativen können dieselbe Handkarte beanspruchen, darum
    // werden alle Zuordnungen probiert. Die Platzhalter passen am Ende, weil
    // `needs.len() <= hand.len()` gilt.
    fn assign(needs: &[&Need], hand: &[u32], used: &mut [bool]) -> bool {
        let Some((first, rest)) = needs.split_first() else {
            return true;
        };
        for i in 0..hand.len() {
            if !used[i] && first.contains(&hand[i]) {
                used[i] = true;
                if assign(rest, hand, used) {
                    return true;
                }
                used[i] = false;
            }
        }
        false
    }
    let fixed: Vec<&Need> = needs.iter().filter(|n| !n.is_empty()).collect();
    assign(&fixed, hand, &mut vec![false; hand.len()])
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
    needs: &[Vec<Need>],
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
        .filter(|(_, c)| is_playable(&needs_of_combo(c), hand))
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
    /// Freies Spielfeld, auf dem die gezogene Hand ausgespielt wird.
    play: Option<FreePlay>,
}

/// Freies Spiel: Züge werden ausgeführt, aber nicht geprüft.
struct FreePlay {
    board: Board,
    field: FieldUi,
    /// Frühere Zustände für „Rückgängig“.
    history: Vec<Board>,
}

impl FreePlay {
    fn new(hand: &[Slot]) -> Self {
        let entries: Vec<Entry> = hand.iter().cloned().map(Entry::new).collect();
        Self {
            board: Board::with_hand(&entries),
            field: FieldUi::default(),
            history: Vec::new(),
        }
    }
}

impl HandTestState {
    /// Deck, aus dem die Kartenauswahl des freien Spielfelds Karten anbietet.
    pub(super) fn deck_id(&self) -> Option<Uuid> {
        self.deck_id
    }

    /// Karte aus der Kartenauswahl (Deck/Extra Deck) für das freie Spielfeld.
    pub(super) fn select_from_picker(&mut self, slot: Slot, area: Area) {
        if let Some(p) = &mut self.play {
            p.field.select(slot, area);
        }
    }
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
            play: None,
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
    /// Freies Spielfeld für die gezogene Hand.
    fn free_play_ui(&mut self, ui: &mut egui::Ui) {
        let v = CardView {
            db: &self.db,
            name_lang: self.name_lang(),
            ban: &self.ban_index,
        };
        let auto_zone = self.settings.auto_zone;
        let Some(play) = self.hand_test.play.as_mut() else {
            return;
        };
        let mut undo = !ui.ctx().egui_wants_keyboard_input()
            && ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Z));
        let mut reset = false;
        let mut close = false;
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(tr("handtest.play_title")).strong());
            undo |= ui
                .add_enabled(!play.history.is_empty(), egui::Button::new(tr("rec.undo")))
                .on_hover_text(tr("rec.undo_hint"))
                .clicked();
            reset = ui.button(tr("handtest.play_reset")).clicked();
            close = ui.button(tr("handtest.play_close")).clicked();
        });
        let mut pick = None;
        let mut error = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            match play
                .field
                .show(ui, &play.board, &mut self.images, v, 56.0, auto_zone)
            {
                Some(FieldEvent::Pick(area)) => pick = Some(area),
                Some(FieldEvent::Move(mv)) => {
                    let before = play.board.clone();
                    match play.board.apply(&mv, kind_of(v.db, &mv.slot)) {
                        Ok(()) => play.history.push(before),
                        Err(e) => error = Some(tr(e.key()).to_string()),
                    }
                }
                None => {}
            }
        });
        if undo && let Some(b) = play.history.pop() {
            play.board = b;
            play.field.reset();
        }
        if reset {
            *play = FreePlay::new(&self.hand_test.hand);
        }
        if close {
            self.hand_test.play = None;
        }
        if let Some(e) = error {
            self.message = Some(e);
        }
        if let Some(area) = pick {
            self.open_picker(Target::FreeFrom(area));
        }
    }

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
            self.hand_test.play = None;
            self.hand_test.hand_deck = None;
            self.hand_test.sim = None;
            ui.ctx().request_repaint();
            return;
        }
        let deck_id = self.decks[di].id;
        self.hand_test.deck_id = Some(deck_id);
        if self.hand_test.hand_deck.is_some_and(|id| id != deck_id) {
            self.hand_test.hand.clear();
            self.hand_test.play = None;
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
            self.hand_test.play = None;
        }
        if run_sim {
            let ids: Vec<u32> = pool.iter().filter_map(Slot::card_id).collect();
            let with_hand: Vec<&Combo> = self
                .combos
                .iter()
                .filter(|c| !c.starting_hand.is_empty())
                .collect();
            let needs: Vec<Vec<Need>> = with_hand.iter().map(|c| needs_of_combo(c)).collect();
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

        if self.hand_test.play.is_some() {
            ui.separator();
            self.free_play_ui(ui);
            return;
        }

        let width = self.settings.card_width;
        let v = CardView {
            db: &self.db,
            name_lang: self.name_lang(),
            ban: &self.ban_index,
        };
        let mut jump = None;
        let mut start_play = false;
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
                if ui
                    .button(RichText::new(tr("handtest.play")).strong())
                    .on_hover_text(tr("handtest.play_hint"))
                    .clicked()
                {
                    start_play = true;
                }
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
                        .num_columns(3)
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
                                // Welche Karten für diese Combo gezählt wurden.
                                let names: Vec<String> = needs_of_combo(c)
                                    .iter()
                                    .map(|need| {
                                        if need.is_empty() {
                                            return tr("handtest.any_card").to_string();
                                        }
                                        need.iter()
                                            .filter_map(|id| v.db.get(*id))
                                            .map(|card| card.localized_name(v.name_lang))
                                            .collect::<Vec<_>>()
                                            .join(" / ")
                                    })
                                    .collect();
                                ui.label(
                                    RichText::new(trf("handtest.needs", &[&names.join(" + ")]))
                                        .weak(),
                                );
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

        if start_play {
            self.hand_test.play = Some(FreePlay::new(&self.hand_test.hand));
        }
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
    fn only_cards_used_in_steps_are_needed() {
        use crate::model::{Connector, Step, StepPart};
        let mut c = Combo::new("x");
        // Eingetragene Hand mit 3 Karten, die Schritte benutzen nur Karte 1.
        c.starting_hand = vec![card(1), card(2), card(3)];
        assert_eq!(needs_of_combo(&c).len(), 3); // ohne Schritte: ganze Hand
        c.steps = vec![Step {
            parts: vec![
                StepPart {
                    connector: Connector::Then,
                    entry: card(1),
                },
                StepPart {
                    connector: Connector::Then,
                    entry: card(9),
                },
            ],
            note: String::new(),
        }];
        assert_eq!(needs_of_combo(&c), vec![vec![1]]);
        assert!(is_playable(&needs_of_combo(&c), &[1, 7, 8]));
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
