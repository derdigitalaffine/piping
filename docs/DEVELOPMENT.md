# Entwicklung und Reproduzierbarkeit

> **Planungsstand:** In `main` existieren zunächst Dokumentation und Issues; Cargo-Dateien sowie Rust-Code gehören zu #1 und Folge-Issues. Unten sind die **später verbindlichen** Befehle, keine bereits ausgeführten Prüfungen.

## Voraussetzungen

Linux x86_64, stable Rust per rustup, Cargo, Git. Rust-Edition 2024, MSRV im Foundation-Issue definieren. Für ICMP-Loopback-Integration passende Socket-Rechte. Keine Docker-Pflicht für Kernentwicklung.

## Sobald Cargo-Projekt vorhanden

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked
cargo build --release --locked
cargo build --release --locked --no-default-features
./target/release/piping --help
./target/release/piping -c 3 127.0.0.1
./target/release/piping -6 -c 3 ::1
```

Die letzten zwei Kommandos benötigen passende Linux-Ping-Sockets. Kein Test darf ohne tatsächlichen Lauf als bestanden erfasst werden; wenn ICMP gesperrt ist, kennzeichnen `SKIP: EPERM/ping_group_range` statt Fake-Grün.

## Tests

- Pure Unit-Tests: Parser, Proto-Fixtures, Sequenzen, Kennzahlen, Grenzwerte.
- Deterministische Simulation: Fake Clock, Mock-Transport, Timeout, Duplicates, Rate Limiting, Ctrl-C.
- CLI-Golden: `--help`, Flags, Classic, Plain/ASCII, JSONL, Exitcodes, stderr.
- Integrations-Suite: Loopback v4/v6 auf Linux, missing socket permission, IPv6 disabled, DNS error, redirection, small/no TTY.
- Performance: ausschließlich Loopback/isoliertes Netzwerk; dokumentierte Maschine/Kernel, CPU/RAM/PPS/latency; `--pps` ist Obergrenze, keine Durchsatzgarantie.

## Branch-/PR-Standard

```text
feat/issue-3-icmpv4
fix/issue-5-timeouts
docs/issue-13-linux-install
```

PR-Text: Bezug zu #Issue, Verhalten, Socket-/Sicherheitsänderung, Tests mit Kommando und echtem Resultat, neue Flags, bekannte Risiken, explizit nicht getestete Plattformen.

## Linux-Fehleranalyse

```sh
uname -a
cat /proc/sys/net/ipv4/ping_group_range
id -g
```

Diese Befehle sind rein lesend. **Nicht automatisch:** `sudo`, `setcap`, `sysctl -w`. Diagnosen mit echten Fehlern abgleichen, niemals Testvoraussetzungen verschweigen.

## CI-Freigabe

Der Foundation-Agent baut GitHub Actions für Linux stable mit `fmt`, `clippy`, `test` und releasefähigem `--locked`-Build. Optionales TUI-Feature bekommt einen getrennten Build/Smoke-Job. Ein nicht gestarteter Workflow oder ein YAML ohne Rust-Projekt ist **kein** Build-Nachweis.
