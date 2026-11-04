# Roadmap und GitHub-Issue-Organisation

Stand: **04.11.2026**. Piping hat derzeit **nur Planung**, keine Rust-Implementierung. Diese Roadmap übernimmt den P0/P1/P2-Ansatz, kritischen Pfad, Go-Live-/DoD-Gates und zentrale Tracking-Organisation von `kostuemwettbewerb`.

## Prioritäten

- **P0:** Ohne dies ist das versprochene Linux-Ping-/Fast-/Range-/Textgraph-MVP nicht abgenommen.
- **P1:** sehr nützlich, aber für erstes sicheres Release nicht zwingend.
- **P2:** optionale Shell-TUI und langfristiger Ausbau.

## Meilensteine (logische Milestones, keine GitHub-Milestone-Objekte)

| Phase | Pflicht-Issues | Lieferschwelle |
| --- | --- | --- |
| **M0 – Foundation** | [#1](../issues/1), [#2](../issues/2) | Rust-Projekt/CI/CLI/Exitcodes nachweislich baubar |
| **M1 – Reliable Ping** | [#3](../issues/3), [#4](../issues/4), [#5](../issues/5), [#6](../issues/6) | Rootless-freundliches ICMPv4/v6, korrekte Korrelation und Statistik |
| **M2 – Advanced Tools** | [#7](../issues/7), [#8](../issues/8) | Fast Ping bounded; Range Pong mit CIDR, Caps, Summary |
| **M3 – Visualization** | [#9](../issues/9) Pflicht; [#11](../issues/11) P1; [#10](../issues/10) P2 | Smoke-ASCII-Fenster auf stdout; TUI getrennt und optional |
| **M4 – Release-ready** | [#12](../issues/12), [#13](../issues/13), [#14](../issues/14) Pflicht; [#15](../issues/15) P1 | Linux-Build, QA-Matrix, Sicherheit, Installationshandbuch |
| **M5 – Later** | [#16](../issues/16) P2 | Langzeit-Logging/Alerts nur nach stabilem MVP |

## Kritischer Pfad und Parallelisierung

`#1 → #2 → #3/#4 → #5/#6 → #7 → #8 → #9 → #12/#14 → #13`.

- Protocol-Agent kann #3 und #4 teilweise parallel nach festem Paket-/Event-Kontrakt arbeiten.
- Engine-Agent startet #5 parallel zu Protokoll-Fakes, verknüpft danach echten Transport.
- Range-Agent beginnt erst, wenn globales PPS-Limit und Multi-Target-IDs von #7 stehen.
- Graph-Agent arbeitet gegen stabile synthetische Mess-Events; keine Abhängigkeit zu optionaler #10.
- QA/Security überprüft durchgängig; #12 läuft inkrementell, nicht erst am Schluss.
- Docs-Agent hält Beispiele nach Implementierung aktuell; TUI und Langzeit-Alerts dürfen den kritischen Pfad nicht blockieren.

## Release-0.1.0-Abnahme

- [ ] Standard-CLI `piping HOST` mit Classic-Ausgabe und korrekten 0/1/2-Exitcodes
- [ ] Linux ICMPv4 & ICMPv6 Echo/Timeout/invalid Reply korrekt
- [ ] Keine Rechte-Eskalation; verständliche Rootless-/EPERM-Hinweise
- [ ] Fast Ping mit getesteten Limits, Fairness und sauberem Abbruch
- [ ] Range Pong auf CIDR/IP-Range mit Hostlimit und ohne Portscan
- [ ] Smoke-ähnliche ASCII/Unicode-Loss-/RTT-Grafik ohne Fullscreen-Abhängigkeit
- [ ] fmt + clippy + test + release build tatsächlich auf Linux bestanden
- [ ] Alle P0-Fälle aus `docs/ACCEPTANCE_TESTS.md` ausgeführt oder begründet offen
- [ ] README, --help, Rechte- und Sicherheitsdokumentation reproduzierbar
- [ ] In separater VM/Container aus frischem Checkout gebaut und Beispielkommando nachweislich ausgeführt

## Nicht verhandelbare Fallbacks

Fehlen Terminalfähigkeiten: Classic-/ASCII-Ausgabe bleibt. Fehlen Socketrechte: saubere Fehlermeldung, keine privilegierte Selbstinstallation. Fehlende IPv6-Unterstützung: explizite Fehldiagnose, kein unbemerkter Protokollwechsel. Fast/Range darf bei Ressourcen-/Rate-Grenzen abbrechen statt unkontrolliert zu senden.

## Arbeitsmodus

Jede Änderung bekommt ein Issue, einen kurzen Branch/PR, reproduzierbare Tests und einen ehrlichen Prüfstatus. Die Source of Truth ist `AGENTS.md` + `docs/` + das zentrale Tracking-Issue; GitHub-Issues bleiben offen bis DoD nachgewiesen ist. Keine erfundenen CI-Erfolge und keine vorweggenommenen Releases.
