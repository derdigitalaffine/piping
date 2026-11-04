# pipi 0.2 – Trace, Smoke per hop, TUI und Neighborhood

> Status: Entwicklungsstand im Branch \`feat/mtr-trace-tui-neighborhood\` (nicht als Production-Release freigegeben). Der ältere Rust-v0.1-Linux-Testnachweis gilt nicht für diese Erweiterung; siehe [QA-Bericht](QA_REPORT.md).

## Installation – ausführbarer Name ist jetzt pipi

Das GitHub-Repository heißt weiterhin \`derdigitalaffine/piping\`. Der Crate- und Binary-Name lautet **pipi**; das bisherige Kommando \`piping\` wird **nicht** neu gebaut.

\`\`\`sh
git clone https://github.com/derdigitalaffine/piping.git
cd piping
cargo build --release --locked --all-features
cargo install --path . --locked --features tui
pipi --version
\`\`\`

## MTR-artiger ICMP Trace

\`\`\`sh
pipi trace --max-hops 20 --smoke 1.1.1.1
pipi trace -4 -c 3 --max-hops 16 -W 0.5 --smoke --ascii 127.0.0.1
pipi trace --tui --smoke 1.1.1.1
pipi trace -6 -c 2 --max-hops 12 ::1
\`\`\`

Die Funktion benutzt Linux-ICMP-Ping-Sockets und \`IP_RECVERR\`/\`IPV6_RECVERR\` mit \`MSG_ERRQUEUE\`, um Time-Exceeded- und Unreachable-Meldungen je Hop zu empfangen. **Keine Raw-IP-Sockets als Standard.** Der Hop-Limit/TTL wird vor jedem Echo auf dem dedizierten Tracesocket gesetzt; eine zeitlich und sequenziell zugeordnete Antwort des Ziels beendet die Hop-Sequenz des aktuellen Zyklus. Ohne \`-c\` läuft die Diagnose wie MTR kontinuierlich bis \`Strg+C\`.

- \`--max-hops N\`: 1–32 (Standard 16)
- \`-c N\`: 1–1000 vollständige Messzyklen
- \`-W SEC\`: 0,1–5 Sekunden Deadline je Hop (Standard 0,5)
- \`-i SEC\`: 0,05–5 Sekunden zwischen den Sendungen, entsprechend maximal 20 pps als Defaulteinstellung
- \`--smoke\`: die letzten 60 Probes je Hop als RTT-Sparkline; \`X\` bedeutet Timeout und nicht „Hop nicht vorhanden“
- \`--ascii\`: ASCII-Sparkline statt Unicode
- \`--tui\`: ein auf Terminalgröße 40x12/80x24 angepasstes Fullscreen-Hop-Dashboard; nur mit Cargo-Feature \`tui\`, keine Terminalsteuerzeichen in der Textvariante

Pro Hop werden Ziel/Router-IP, Paketanzahl, Verlust, letzte/beste/mittlere/schlechteste RTT und zeitliche Smoke-Reihe gezeigt. Das ist eine bewusst kleine MTR-ähnliche Version, **keine vollständige Implementierung aller MTR-Funktionen** (kein TCP/UDP-Trace, MPLS, AS-Lookup, historischer persistenter Log oder Reverse-DNS pro Hop). Router können ICMP Time Exceeded unterdrücken, daher ist ein \`*\` unbestimmt. Loss eines TTL-Hops kann von Router-Rate-Limits stammen und ist **nicht** zwingend Weiterleitungspaketverlust. Keine externen Ziele in Tests.

## Neighborhood: automatische Anzeige der lokalen Umgebung

\`\`\`sh
pipi neighborhood                        # autom. alle Interfaces und ARP/NDP-Cachenachbarn, einmalig
pipi neighborhood --ascii --family 4    # SSH, Pipes oder rein ASCII
pipi neighborhood --tui                 # automatische passive Aktualisierung alle 5s
pipi neighborhood --tui --watch 10 -i eth0
pipi neighborhood --watch 5 -c 3 --max-results 100
\`\`\`

Die Baumgrafik zeigt **LOCAL HOST → lokales Interface → beobachteter ARP-/NDP-Eintrag** (IP, MAC, Cache-Zustand). Das ist ein *sichtbarer Nachbarschaftsausschnitt*, nicht eine physische Ethernet-/Switch-Verbindungskarte; fehlende oder veraltete Cache-Daten sind normal. Die automatische Standarderkennung verwendet ausschließlich den Linux-Kernel-Nachbarcache via read-only RTM_GETNEIGH; kein Root, keine aktiven Pakete.

### Optionales aktives Erkennen (nur ausdrücklich)

\`\`\`sh
pipi neighborhood --scan-local --interface eth0
pipi neighborhood --active 192.168.1.0/24 --interface eth0
\`\`\`

\`--scan-local\` liest die lokale Routingtabelle, wählt **genau ein** direkt angebundenes **privates IPv4-Netz von /24 bis /31**, oder verweigert den Scan. Ein unbekanntes, öffentliches, zu breites oder mehrdeutiges Netz wird **nicht** automatisch gescannt. Beide aktiven Varianten benötigen \`CAP_NET_RAW\`, bleiben auf das gewählte Interface und on-link-CIDR beschränkt, senden maximal 256 ARP-Anfragen mit höchstens 20 pps und benutzen den bestehenden ARP-Reply-Validator. Aktive Scans werden bewusst **nicht** automatisch oder bei \`--watch\` / \`--tui\` wiederholt; das schützt vor unbemerkten periodischen Netzanfragen.

## Bildschirmlayout

Für \`pipi tui HOST\` (Ping), \`pipi trace --tui HOST\` (MTR/Smoke) und \`pipi neighborhood --tui\` gilt:

- Keine Scroll-Flut: Zeilen und Breiten werden nach \`TIOCGWINSZ\` begrenzt, mindestens ein Zeichen für Wrap-Sicherheit frei gelassen.
- Wichtigste Ping-KPIs: sent/received/loss, min/avg/max/p95/jitter, jüngste Graph-Buckets.
- Trace-KPIs je Hop: Adresse, Verlust, RTT und Smoke; auf schmalen Displays kompakte Spalten.
- Neighborhood: Interface-/ARP-/NDP-Baum, Cache-Verfügbarkeit; passives Auto-Refresh.
- Der Alternate-Screen wird bei normalem Ende und \`Strg+C\` zurückgestellt. Es gibt keine obligatorische Fullscreen-Abhängigkeit im Rust-Core.

Die TUI zeigt bei kleineren Terminals bewusst nur die sichtbaren Zeilen. Weitere Hops/Geräte bleiben in der Textansicht sichtbar; für ausgedehnte Routentabellen kann \`--max-hops\` reduziert oder die Terminalgröße erhöht werden.

## Reproduzierbare Prüfung

\`\`\`sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-features
cargo build --release --locked --all-features
bash -n tests/trace_neighborhood_smoke.sh
bash tests/trace_neighborhood_smoke.sh ./target/release/pipi
\`\`\`

Für echte Linux-ICMP-Tests sind passende Ping-Socket-Berechtigungen erforderlich. Bei fehlender Berechtigung wird **SKIP** statt „bestanden“ angegeben. Neue Linux-FFI und grafische Pfade erfordern vor einer offiziellen Freigabe echte CI-Ausführung und Security-Review.
