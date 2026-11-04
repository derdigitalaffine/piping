# piping – QA-Protokoll und Freigabegates

Stand: **04.11.2026**, Kandidat `v0.1.0` auf `main` nach [PR #18](https://github.com/derdigitalaffine/piping/pull/18) und [PR #19](https://github.com/derdigitalaffine/piping/pull/19).

## Nachweislich erfolgreiche Linux-CI-Prüfungen

[GitHub-Actions-Lauf #41807144867](https://github.com/derdigitalaffine/piping/actions/runs/41807144867), Commit `997606d2cd2ced6548474dc8d1cf95cbdc757099`, Resultat **success** (beide Jobs).

- Rust Stable auf Ubuntu x86_64: `cargo test --locked --all-features` → **26 Unit-Tests und 4 CLI-Tests bestanden**.
- `cargo fmt --all -- --check` und `cargo clippy --locked --all-targets --all-features -- -D warnings` → bestanden.
- `cargo build --release --locked --no-default-features` und `--all-features` → bestanden.
- Rust **1.85.0**: separater MSRV-Job mit Tests und Release-Build `--no-default-features` → bestanden.
- `tests/linux_smoke.sh`: **11/11 PASS**, **0 SKIP, 0 FAIL**. Linux CI host erlaubte Datagrams nach einer explizit auf den ephemeren Runner beschränkten `ping_group_range`-Einstellung. Die Anwendung selbst ändert kein `sysctl` und verwendet keine erhöhten Rechte.
- Laufzeit-Smoketests: ICMPv4 `127.0.0.1` (3/3), ICMPv6 `::1` (3/3), Fast Ping (5/5), Range IPv4 `127.0.0.1/32` (1/1), Monitor-Smoke-Grafik (9/9), JSONL (2/2 + validiertes JSON), CSV (2/2 + CSV-Schema), SIGINT-Abschluss, Broken-Pipe, Fullscreen-TUI im Pseudo-Terminal mit Alternate-Screen-Restore, Fehlerpfad ohne TTY.
- `tests/benchmark_loopback.sh`: **201/201** IPv4-Loopback-Replies mit `--pps 100`, **2013 ms** externe Laufzeit, `/usr/bin/time` **2.00 s**, **9 % CPU**, **2220 KB max RSS** auf dem Runner (Linux **7.0.0-1017-azure**, x86_64). Das ist *eine* reproduzierbare Loopback-Messung, kein Vergleich zu `fping` und keine Performance-Garantie.

## Testmatrix gegenüber `docs/ACCEPTANCE_TESTS.md`

| IDs | Status | Beleg / Einschränkung |
| --- | --- | --- |
| A01, A02, A03 | PASS | CI Release-Builds, fmt/clippy/tests, `--help` ohne Socket |
| A04, A05 | PASS | echte Linux-Loopback-Echos mit IPv4 und IPv6 |
| A06 | TEILWEISE | Null-Statistik tests vorhanden, durchgehend nichtantwortender Host/Exitcode nicht live getestet |
| A07, A08 | PASS | CLI negative tests, ICMP-Paketvalidator, Reply source/seq/nonce |
| A09 | TEILWEISE | duplicate und late reply fixtures; Out-of-order End-to-End noch offen |
| A10 | TEILWEISE | `-c`, Intervalle und SIGINT live; Deadline/Timeout unter künstlicher Last noch offen |
| A11 | TEILWEISE | Stats/Welford/Percentile tests; Fake-Clock für Scheduler offen |
| A12 | TEILWEISE | Fast-Loopback und Rate-Benchmark; künstliche Queue-Überlastsimulation offen |
| A13, A14 | PASS | CIDR-/31-/32-/IPv6-/Hostlimit/ungültige Range Unit-Tests |
| A15 | PASS | graph bucket Golden-/Loss-/Spread Tests und realer Monitor |
| A16 | TEILWEISE | ASCII und Pipes / non-TTY getestet; Terminalbreiten 40/80/120 nicht separat geprüft |
| A17 | TEILWEISE | EPERM-Fehler real in früheren Runnern beobachtet; isolierter Restriktions-Netns-Test offen |
| A18 | PASS | Echo über reale Linux ICMPv6 Ping-Sockets, Kernel-Checksum-Pfad |
| A19, A20 | PASS | JSONL/CSV und TUI-Minimal-/Feature-/Pseudo-TTY-Restore getestet |
| A21 | TEILWEISE | CLI-Bounds, begrenzte Stats-Reservoirgröße und PPS gemessen; Queue-Stresstest offen |
| A22 | TEILWEISE | frischer CI-Checkout mit isoliertem `cargo install --path . --locked`, `--help`, `--version` und echtem IPv4-Echo als installiertes Binary geprüft; unabhängige VM-Installation / aarch64 offen |

## Nachgehärtete v0.1-Tests

- DNS-Auflösung aus Hostnamen hat eine feste **5-Sekunden-Deadline**; numerische IP-Adressen umgehen DNS. Der Timeout-Test simuliert einen blockierenden Resolver ohne externe Netzwerkverbindungen.
- Streaming JSONL und CSV haben `timestamp_unix_ms` als UTC-Epoch-Zeitstempel; die echte Loopback-Suite validiert die Felder.
- Die Quelle wurde auf demselben CI-Runner zusätzlich mit isoliertem `CARGO_TARGET_DIR` installiert, `--help`/`--version` und `-4 -c 2 127.0.0.1` wurden unter dem installierten Pfad erfolgreich ausgeführt.
- Zwei neue Rust-Tests (DNS Numeric/Familie, Resolver-Deadline) erhöhen die Anzahl auf **26 Unit- + 4 CLI-Tests**. Die Paket-/Duplikat-Regressionstests sind weiterhin aktiv.
- Compiler-/Installationsjobs sind voneinander isoliert, damit eine No-Feature-Installation das mit `tui` gebaute Binary nicht überschreibt.

## Nach-Merge-CI-Transparenz

Die nach dem grünen Code-Commit erfolgte Änderung war ausschließlich Dokumentation. [Compare 997606...21c45](https://github.com/derdigitalaffine/piping/compare/997606d2cd2ced6548474dc8d1cf95cbdc757099...21c45e1111b8bb4ea9b73a8aa13797ea4cdafb24) zeigt sechs Dokumentdateien, keine Rust- oder Workflowänderung. Danach scheiterten die GitHub-Actions-Läufe auf dem Dokumentations- bzw. Main-Head **vor jedem Jobschritt**; [Main-Run #41807582908](https://github.com/derdigitalaffine/piping/actions/runs/41807582908) hat zwei fehlgeschlagene Jobs mit **0 Steps und ohne verfügbare Logs**. Ursache nicht bestätigt; daraus darf weder ein Compilerfehler noch ein Testerfolg abgeleitet werden. Der letzte vollständige Quellcode-Testnachweis ist [#41807144867](https://github.com/derdigitalaffine/piping/actions/runs/41807144867). Für formelle Release-Freigabe sind erneut ausführbare Runner erforderlich.

## Release-Artefakte

Beide x86_64-Varianten und SHA256-Datei werden in CI lokal erzeugt. GitHub `upload-artifact` schlug bei einem früheren Lauf wegen **ausgeschöpftem Artifact-Storage-Quota** fehl, nicht wegen Buildfehlern. Daher ist der Upload auf Versionstags beschränkt und optional; **es gibt noch keinen frei herunterladbaren bestätigten Release**. `cargo install --path . --locked` ist dokumentierter Source-Installationspfad, aber nicht als unabhängige VM-Installation abgenommen.

## Freigabe und Restrisiken

**Status: Linux x86_64 funktions-, install- und CI-geprüfter v0.1-Kandidat, aber kein öffentlich signierter Binärrelease oder unabhängiger Security-Audit.** Noch erforderlich für eine belastbare öffentliche Release-Freigabe: negative/überlastete Deadline- und Rate-Tests mit Mock-Clock, Container/Namespace-Rechte-Regressionen, mindestens eine unabhängige Linux-VM/Installation, Freigabe des `unsafe`-Socket-FFI und Lizenz-/Distributionentscheidung. Eine aarch64-Zielarchitektur ist bisher nicht getestet. TUI ist optional und bewusst minimal, nicht vollständig interaktiv.

Keine automatischen Issue-Schließungen allein wegen vorhandenem Code. Verbindliche Testbefehle:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-features
cargo build --release --locked --no-default-features
cargo build --release --locked --all-features
bash tests/linux_smoke.sh ./target/release/piping
bash tests/benchmark_loopback.sh ./target/release/piping
```
