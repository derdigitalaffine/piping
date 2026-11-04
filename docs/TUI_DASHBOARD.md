# pipi v0.3 – grafisches Netzwerk-Kontrollzentrum

## Installation und Start

    git clone https://github.com/derdigitalaffine/piping.git
    cd piping
    cargo install --locked --path . --features tui
    pipi

Ohne HOST öffnet **pipi** ein grafisches, farbiges Terminal-Menü. **pipi tui** ohne HOST öffnet das gleiche Menü. **pipi tui 127.0.0.1** startet weiterhin die direkte Ping-TUI. Für den Menüstart werden TUI-Feature, interaktives stdin/stdout und ein fullscreenfähiges Terminal vorausgesetzt. Kein automatischer Netzwerkscan beim Öffnen!

## Navigation und Formulare

Das Menü bietet neun Module: klassisches PING, FAST (Rate/Jobs), RANGE (CIDR/Hostlimit), SMOKE (RTT-/Verlustkurve), TRACE (MTR-artig je Hop mit Smoke), NEIGHBORHOOD (passiver ARP/NDP-Baum), DISCOVER (passive Nachbarn), ARP SCAN (explizite CIDR) und LOCAL SCAN (explizites privates lokales Netz).

Mit **Pfeilen/Tab** durch Menü und Formulare navigieren, **1–9** als Hauptmenü-Kurzwahl nutzen. Textfelder durch Tippen/Backspace bearbeiten, Auswahlwerte per Links/Rechts/Enter umschalten. **F5 oder Strg+R** startet eine Messung. **Esc** navigiert zurück, **q** stoppt eine Messung oder beendet das Startmenü. Nach einem beendeten Lauf führt Enter zur Formularansicht zurück.

Die Messung erscheint mit Status, Parametern und einem auf 120 Einträge begrenzten Live-Ergebnisstrom. Die Panels passen sich auch kleinen Terminals an und zeigen die ausgewählte Formularzeile bzw. neueste Messergebnisse.

## Farbe und Gerätekompatibilität

ANSI-Farben nur bei geeigneten interaktiven Terminals. **NO_COLOR=1 pipi** oder **pipi tui --no-color** erzwingt Monochrom. JSONL/CSV, Pipes und TERM=dumb erhalten keine Farbsequenzen; auf TERM=dumb wird das fullscreenfähige Menü mit einer Diagnose abgelehnt. Die Terminal-Attribute und der Alternate Screen werden bei normalem Verlassen oder Ctrl-C wiederhergestellt.

    NO_COLOR=1 pipi
    pipi tui --no-color
    pipi tui 127.0.0.1 --no-color

## Aktive Discovery ist gesondert bestätigt

Das Menü öffnet keine Scan-Sockets. Beim ARP SCAN und LOCAL SCAN muss die Netzwerk-Schnittstelle gewählt, danach START gedrückt und **nochmals auf der Bestätigungsseite** Enter/Y gewählt werden, bevor Pakete gesendet werden. Esc/N bricht ohne Scan ab. Es gelten alle bestehenden on-link-, Rate-/Hostlimits und CAP_NET_RAW-Anforderungen. Es gibt kein Auto-Sudo und keine Shell.

## Reproduzierbare Abnahme

    cargo fmt --all -- --check
    cargo clippy --locked --all-targets --all-features -- -D warnings
    cargo test --locked --all-features
    cargo build --release --locked --all-features
    python3 tests/tui_console_pty.py ./target/release/pipi

Der PTY-Test prüft ohne Host gestartete Menüs, Tasten, Farbe/NO_COLOR, aktive Scan-Bestätigung, Ping-Loopback, Terminal-Restore und den Nicht-TTY-Fallback.
