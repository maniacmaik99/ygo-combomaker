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

- Linux: `ygo-combomaker-linux-x86_64` herunterladen, ausführbar machen (`chmod +x ygo-combomaker-linux-x86_64`), starten. Für einen Eintrag im Startmenü: Datei als `ygo-combomaker` in einen Ordner im `PATH` legen, `assets/ygo-combomaker.desktop` nach `~/.local/share/applications/` und `assets/icon-512.png` als `~/.local/share/icons/hicolor/512x512/apps/ygo-combomaker.png` kopieren.
- Windows: `ygo-combomaker-windows-x86_64.exe` herunterladen und starten. Die Datei hat das App-Logo als Symbol.

## Updates

Der Knopf „Update“ oben in der App sucht auf GitHub nach einem neueren Release. Wenn es eins gibt, lädt die App die passende Datei herunter, ersetzt sich selbst und startet neu. Optional prüft die App das auch beim Start (Einstellungen).

## Neue Version veröffentlichen (2 Schritte)

1. In `Cargo.toml` die Version erhöhen, z. B. `0.2.0`.
2. Committen und auf `main` pushen.

GitHub Actions merkt, dass es für diese Version noch kein Release gibt, baut Linux- und Windows-Version und legt das Release `v0.2.0` an (Datei `.github/workflows/release.yml`). Das dauert etwa 13 Minuten.

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

## Funktionen (Stand 0.5)

- **Sprachen:** Englisch, Deutsch, Französisch, Italienisch, Spanisch, Portugiesisch (Einstellungen → Sprache). Kartennamen optional in der gewählten Sprache (außer Spanisch, das bietet die Kartendatenbank nicht an).
- **Bedienung wie im Spiel:** Karte anklicken → direkt an der Karte erscheint ein Menü mit den Aktionen, die an diesem Ort üblich sind (z. B. Hand: Normalbeschwörung, Spezialbeschwörung, Setzen, Effekt aktivieren). Oder die Karte mit der Maus auf eine Zone, den Friedhof, den Verbannt-Stapel oder die Hand ziehen. Deck und Extra Deck sind Stapel auf dem Feld. Esc oder Rechtsklick bricht ab, Strg+Z macht den letzten aufgenommenen Zug rückgängig. Optional wählt die App freie Zonen automatisch (Einstellungen).
- **Tipp beim Üben:** markiert die Karte des nächsten Zugs.
- **Handtest:** zieht eine zufällige Starthand aus einer Deckliste, zeigt, welche gespeicherten Combos damit spielbar sind, und simuliert 1000 Hände (Anteil der Hände, mit denen eine Combo spielbar ist).
- **Suche in der Combo-Liste.**
- **Combo eintragen auf dem Spielfeld:** Starthand festlegen, dann die Combo spielen: Karte anklicken → Aktion wählen → falls nötig Zielzone anklicken. Jeder Zug wird gespeichert. „Neuer Schritt“ trennt Schritte, „Letzten Zug rückgängig“, „Feld als Endboard übernehmen“.
- **Combo eintragen als Liste:** alle Bereiche (Starthand, Endboard, Handtraps, benötigte Karten, Schritte) Karte für Karte bearbeiten.
- **Spielfeld (Aufbau wie Master Rule 5):** 2 Extra-Monsterzonen über Monsterzone 2 und 4; Feldzauberzone, 5 Monsterzonen, Friedhof; Extra Deck, 5 Zauber-/Fallenzonen (die beiden äußeren sind zugleich die Pendelzonen), Deck; Verbannt über dem Friedhof; Hand darunter. Das Feld passt seine Größe an das Fenster an und ist zentriert. Xyz-Material erscheint als Stapel, sobald es welches gibt.
- **Aktionen (38):** Effekt aktivieren, Kettenglied (frei wählbare Nummer), in die Pendelzone legen, Zauber/Falle setzen, offen auf das Feld legen, Ausrüsten; Normal-, Tribut-, Flipp-, Spezial-, Fusions-, Ritual-, Synchro-, Xyz-, Pendel- und Linkbeschwörung, Monster verdeckt setzen; Tribut, Material, Xyz-Material an-/abhängen, Abwerfen; Suchen (auf Hand oder Friedhof), Ziehen, Friedhof, Verbannen, Zerstören, zurück auf die Hand, Friedhof → Hand/Deck, verbannt → Hand/Friedhof, zurück ins Deck/Extra Deck, Aufdecken; Annullieren, Ziel wählen, Kampfposition ändern.
- **Üben auf dem Spielfeld:** Die Combo Zug für Zug nachspielen. Nach jedem Zug: richtig oder falsch. „Lösung zeigen“ spielt den nächsten Zug vor. Ergebnis in Prozent, bestes Ergebnis wird gespeichert.
- **Decklisten:** aus einem Archetyp füllen oder importieren (ydke-Link, YDK-Datei, Kartenliste). Eine Combo kann mit einer Deckliste verknüpft werden; deren Karten stehen beim Hinzufügen ganz oben.
- **Teilen:** Combo als Text in die Zwischenablage kopieren, mit „Importieren“ wieder einfügen.

## Aufbau des Codes

| Datei | Inhalt |
|---|---|
| `src/model.rs` | Datenmodell (Combo, Schritt, Karte, Aktion, Zone) |
| `src/cards.rs` | Kartendatenbank, YGOPRODeck-API, Suche |
| `src/images.rs` | Bilder laden und zwischenspeichern (Hintergrund-Threads) |
| `src/decks.rs` | Decklisten importieren (ydke, YDK, Kartenliste) |
| `src/storage.rs` | Speicherorte, Laden/Speichern, Teilen |
| `src/board.rs` | Spielfeld-Logik: Zonen, Züge, Ziel jeder Aktion |
| `src/practice.rs` | Logik des Übungsmodus (Züge prüfen) |
| `src/updater.rs` | Update über GitHub Releases |
| `src/i18n/` | Übersetzungen (eine Datei pro Sprache) |
| `src/theme.rs` | Farben und Stil der Oberfläche |
| `src/app/` | Oberfläche (Editor, Spielfeld, Üben, Decks, Handtest, Kartensuche) |
| `assets/` | Logo (`make_icon.py` erzeugt es neu), Desktop-Datei für Linux |

Tests: `cargo test`
