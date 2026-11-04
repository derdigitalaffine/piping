# AGENTS.md – Piping Agentenorganisation

## Mission

Entwickle ein auf Linux kompilierbares, robustes Rust-Programm für ICMP Ping; es muss ohne TUI wie ein einfacher Ping-Befehl bedienbar sein, plus Fast Ping, Range Pong und SmokePing-artige Shellgrafik bieten. **Korrektheit, Rechte-Minimierung und reproduzierbare Qualität gehen vor Effekten und hohem PPS.**

Dieses Dokument übernimmt das Arbeitsmodell aus `kostuemwettbewerb` (Rollen, Dokumentationshierarchie, Issue/PR-Disziplin, Test- und Abnahmepflichten), aber **keinen fachfremden Django-/Docker-/Eventcode**.

## Pflichtlektüre vor Änderungen

1. `README.md`
2. `docs/PRODUCT_SPEC.md`
3. `docs/ARCHITECTURE.md`
4. `docs/SECURITY.md`
5. `docs/ROADMAP.md`
6. `docs/ACCEPTANCE_TESTS.md`
7. `docs/DECISIONS.md`
8. `docs/DEVELOPMENT.md`

Widersprüche in PR und `docs/DECISIONS.md` dokumentieren, Sicherheitsinvarianten nicht stillschweigend aufweichen.

## Agenten – Zuständigkeiten und Reihenfolge

1. **Foundation-Agent (#1, #2)**: Cargo, CI, CLI-Vertrag, Fehler-/Exitcode-API, Feature-Grenzen.
2. **Protocol-Agent (#3, #4, #6)**: ICMPv4/v6, Linux Ping-Sockets, Rechte/Protokollvalidierung.
3. **Engine/Stats-Agent (#5, #7)**: Monotone Messuhr, Scheduler, Korrelation, Fast/Rate-Limits, Statistik.
4. **Range-Agent (#8)**: CIDR/Adressbereich, Hostlimits, fairer Multi-Target-Scheduler.
5. **Output/Graph-Agent (#9, #11)**: Classic-Ausgabe, ASCII-Smokegrafik, maschinenlesbare Formate.
6. **TUI-Agent (#10, optional)**: Fullscreen nur hinter opt-in-Cargo-Feature; Core unabhängig.
7. **QA/Security-Agent (#12, #14)**: Fixtures, reproduzierbare Tests, Fehlermodelle, Benchmarks.
8. **Docs/Release-Agent (#13, #15, #16)**: Linux-Anleitung, Referenz, Pakete/Changelog, später Monitoring.

**Freigabegates:** Foundation → ICMP und Engine → Fast/Range → Smoke-Text → QA/Release. Security/QA läuft ab Foundation mit, TUI erst nach stabiler Text-UI. Parallelität nur bei getrennten Modulen und festgelegten Schnittstellen.

## Technische Invarianten

- Linux-first, stable Rust, `Cargo.lock` im Git; releasefähiger Build mit `cargo build --release --locked`.
- Einfaches `pipi HOST` ohne TUI, ohne Admin-UI, ohne Konfigurationsdatei lauffähig, wenn Netzwerk/Sockets verfügbar.
- Keine verpflichtende Fullscreen-/Terminal-Abhängigkeit im Core.
- Keine ungefragten Systemänderungen, kein `sudo`-/`setuid`-Autofix, keine Hardcoded-Secrets.
- Ping-Echo-Datagram-Socket als rootless Standard; Privileg-/Raw-Pfad nur bewusst, dokumentiert und getestet.
- Falsche, verspätete, duplizierte oder fremde Echo-Antworten nicht als Erfolg verbuchen.
- Alle Fast-/Range-Fanouts, Queues, Größen und Raten begrenzen; niemals ungefragt flooden.
- Text-/JSON-/Pipes haben keine ungefragten ANSI-Cursorsteuerungen; `NO_COLOR` beachten.
- Kein öffentliches Netzziel für Performance-Tests; nur Loopback/isoliertes Testnetz und Mock-Transport.
- IPv4 und IPv6 getrennt prüfen; ICMPv6-Prüfsummen-/Kernelverhalten nicht aus IPv4 raten.
- Keine Behauptung über Test-/Build-Erfolg ohne tatsächlichen Run.

## Implementierungsablauf

1. **Ein Issue oder eng gekoppeltes Paket** wählen; Abhängigkeiten im Tracking-Issue prüfen.
2. PR-Branch `feat/issue-<nr>-<slug>` erstellen; kleine, reviewbare Commits.
3. Öffentliche Schnittstellen und Messinvarianten vor Codeänderung dokumentieren.
4. Tests für Erfolg, Fehlfall, Privilegien, Timeout und Ressourcenlimits ergänzen.
5. Lokal `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --locked` und `cargo build --release --locked`, sofern Rust-Code vorhanden.
6. PR mit Issue-Referenz, Änderungen, Sicherheitskonsequenzen, Kommandos mit Ergebnissen, nicht ausgeführten Prüfungen und Dokuabweichungen.
7. Erst nach Review/CI/Abnahme mergen; kein Agent markiert fremde Arbeit allein deshalb als erledigt, weil ein Commit existiert.

## Definition of Done

Ein Issue gilt erst als fertig, wenn Kriterien im Ticket + zugehörige `docs/ACCEPTANCE_TESTS.md`-Fälle bestanden sind, Beispiel-CLI/Doku stimmt, Ressourcen/Rechte geprüft wurden und echte Linux-Build-/CI-Nachweise vorliegen. Bei äußerlichen Tests: offen/übersprungen explizit markieren. Vollständige Projekt-DoD siehe Roadmap.

## M6-Agenten (Issues #20–#22)

**Hotpath/Performance-Agent (#20):** Keine Heap-Allokation pro Echo/Socketevent, faire O(1)-Queue, CPU/RSS/PPS auf gleichem Linux-Runner messen. Grenzwerte bleiben für autorisierte Netze und Loopback. **Layer-2-Agent (#21):** Rootless, passive read-only Netlink ARP/NDP, starke Parser-Validierung, fehlende Cacheeinträge als normal behandeln. **Active-ARP/Security-Agent (#22):** Explizit freigegebenes Interface/CIDR, on-link-Route, kein Auto-Sudo, max 100 PPS/256 Hosts, synthetische Forgery-Tests und veth-only Live-Integration. Bestehende Main-DoD darf nicht als Bestätigung der neuen Module missverstanden werden.
