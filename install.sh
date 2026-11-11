#!/bin/sh
# YGO Combomaker – Installation für Linux (alle Distributionen, x86_64).
# Installiert nur für den aktuellen Benutzer, ohne root:
#   Programm:    ~/.local/bin/ygo-combomaker
#   Menüeintrag: ~/.local/share/applications/ygo-combomaker.desktop
#   Symbol:      ~/.local/share/icons/hicolor/512x512/apps/ygo-combomaker.png
#
# Installieren:    sh install.sh
# Deinstallieren:  sh install.sh --uninstall
set -eu

REPO="maniacmaik99/ygo-combomaker"
BIN_DIR="${XDG_BIN_HOME:-$HOME/.local/bin}"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"
BIN="$BIN_DIR/ygo-combomaker"
DESKTOP="$DATA_DIR/applications/ygo-combomaker.desktop"
ICON="$DATA_DIR/icons/hicolor/512x512/apps/ygo-combomaker.png"

if [ "${1:-}" = "--uninstall" ]; then
    rm -f "$BIN" "$DESKTOP" "$ICON"
    echo "YGO Combomaker wurde entfernt. Combos und Einstellungen liegen weiter in $DATA_DIR/ygo-combomaker."
    exit 0
fi

if [ "$(uname -s)" != "Linux" ] || [ "$(uname -m)" != "x86_64" ]; then
    echo "Fehler: Dieses Skript ist nur für Linux auf x86_64." >&2
    exit 1
fi

download() { # download <URL> <Zieldatei>
    if command -v curl >/dev/null 2>&1; then
        curl -fL --progress-bar -o "$2" "$1"
    elif command -v wget >/dev/null 2>&1; then
        wget -q --show-progress -O "$2" "$1"
    else
        echo "Fehler: Weder curl noch wget ist installiert." >&2
        exit 1
    fi
}

mkdir -p "$BIN_DIR" "$(dirname "$DESKTOP")" "$(dirname "$ICON")"

echo "Lade das Programm herunter …"
TMP="$BIN.download"
download "https://github.com/$REPO/releases/latest/download/ygo-combomaker-linux-x86_64" "$TMP"
chmod +x "$TMP"
mv -f "$TMP" "$BIN"

echo "Lade das Symbol herunter …"
download "https://raw.githubusercontent.com/$REPO/main/assets/icon-512.png" "$ICON" || true

cat > "$DESKTOP" <<DESKTOP_EOF
[Desktop Entry]
Type=Application
Name=YGO Combomaker
Comment=Enter and practice Yu-Gi-Oh! combos
Comment[de]=Yu-Gi-Oh!-Combos eintragen und üben
Exec=$BIN
Icon=ygo-combomaker
Terminal=false
Categories=Game;CardGame;
StartupWMClass=ygo-combomaker
DESKTOP_EOF

command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$(dirname "$DESKTOP")" >/dev/null 2>&1 || true

echo
echo "Fertig. YGO Combomaker steht jetzt im Startmenü."
echo "Start im Terminal: $BIN"
case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) echo "Hinweis: $BIN_DIR ist nicht im PATH. Der Menüeintrag funktioniert trotzdem." ;;
esac
