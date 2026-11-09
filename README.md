# YGO Combomaker

Desktop-App in Rust zum Eintragen und Üben von Yu-Gi-Oh!-Combos (Master Duel, TCG).
Keine Anmeldung, keine Web-Technik: Oberfläche mit [egui/eframe](https://github.com/emilk/egui), läuft nativ unter Linux und Windows.

## Datenquelle

Kartennamen, Kartentexte und Bilder kommen von der freien **YGOPRODeck-API** (https://ygoprodeck.com/api-guide/).

- Beim ersten Start lädt die App die komplette Kartenliste (englisch + deutsche Namen) und speichert sie lokal.
- Bilder werden erst geladen, wenn eine Karte angezeigt wird, und danach lokal gespeichert. Jedes Bild wird nur einmal heruntergeladen.
- Höchstens 20 Bild-Downloads pro Sekunde (Limit der API).
- „Karten aktualisieren“ prüft online die Datenbank-Version und lädt nur bei Änderungen neu.

Master Duel Meta hat keine offizielle, dokumentierte API. Deshalb nutzt die App deren Daten nicht.

Kartentexte und -bilder: © Konami / 4K Media. Diese App ist nicht mit Konami oder YGOPRODeck verbunden.

## Herunterladen

Fertige Programme für Linux und Windows liegen unter „Releases“ im GitHub-Repo:

- Linux: `ygo-combomaker-linux-x86_64` herunterladen, ausführbar machen (`chmod +x ygo-combomaker-linux-x86_64`), starten.
- Windows: `ygo-combomaker-windows-x86_64.exe` herunterladen und starten.

## Updates

Der Knopf „Update“ oben in der App sucht auf GitHub nach einem neueren Release. Wenn es eins gibt, lädt die App die passende Datei herunter, ersetzt sich selbst und startet neu. Optional prüft die App das auch beim Start (Einstellungen).

Solange das Repo **privat** ist, braucht die App dafür ein GitHub-Token (4 Schritte):

1. https://github.com/settings/personal-access-tokens/new öffnen.
2. „Repository access“: „Only select repositories“ → `ygo-combomaker`.
3. „Permissions“ → „Repository permissions“ → „Contents“: „Read-only“. Token erstellen.
4. Token in der App unter „Einstellungen“ → „GitHub-Token“ einfügen.

Das Token wird unverschlüsselt in `settings.json` gespeichert. Es hat nur Leserecht für dieses eine Repo.

## Neue Version veröffentlichen (2 Schritte)

1. In `Cargo.toml` die Version erhöhen, z. B. `0.2.0`.
2. Committen und auf `main` pushen.

GitHub Actions merkt, dass es für diese Version noch kein Release gibt, baut Linux- und Windows-Version und legt das Release `v0.2.0` an (Datei `.github/workflows/release.yml`). Das dauert etwa 10 Minuten.

## Selbst bauen

Voraussetzung: Rust (https://rustup.rs).

```bash
cargo run --release
```

Das fertige Programm liegt danach unter `target/release/ygo-combomaker` (Windows: `ygo-combomaker.exe`).

## Speicherorte

| Was | Linux | Windows |
|---|---|---|
| Combos, Einstellungen | `~/.local/share/ygo-combomaker/` | `%APPDATA%\ygo-combomaker\data\` |
| Kartendatenbank, Bilder | `~/.cache/ygo-combomaker/` | `%LOCALAPPDATA%\ygo-combomaker\cache\` |

## Funktionen (Stand 0.2)

- Combo anlegen: Titel, Deck, Autor, Notizen
- Bereiche: Starthand, Endboard, „Spielt um“ (Handtraps), Benötigte Karten, Schritte
- Karten über die Suche (englischer oder deutscher Name) oder als Platzhalter („Any Card“, „discard“)
- Pro Karte: Aktion (Effekt, Normal-/Spezialbeschwörung, Suchen, Abwerfen, Setzen, Friedhof, Verbannen, Kettenglied 1–6) und Notiz
- Decklisten: aus einem Archetyp füllen oder importieren (ydke-Link, YDK-Datei, Kartenliste als Text). Eine Combo kann mit einer Deckliste verknüpft werden; deren Karten stehen beim Eintragen ganz oben und sind mit einem Klick auswählbar
- Schritte mit Zeichen zwischen den Karten: → ← + = (anklicken zum Ändern)
- Übungsmodus: Karten pro Schritt raten, prüfen, Ergebnis in Prozent, bestes Ergebnis wird gespeichert
- Teilen: Combo als Text in die Zwischenablage kopieren, mit „Importieren“ wieder einfügen

## Aufbau des Codes

| Datei | Inhalt |
|---|---|
| `src/model.rs` | Datenmodell (Combo, Schritt, Karte, Aktion) |
| `src/cards.rs` | Kartendatenbank, YGOPRODeck-API, Suche |
| `src/images.rs` | Bilder laden und zwischenspeichern (Hintergrund-Threads) |
| `src/decks.rs` | Decklisten importieren (ydke, YDK, Kartenliste) |
| `src/storage.rs` | Speicherorte, Laden/Speichern, Teilen |
| `src/practice.rs` | Logik des Übungsmodus |
| `src/updater.rs` | Update über GitHub Releases |
| `src/app.rs` | Oberfläche |

Tests: `cargo test`
