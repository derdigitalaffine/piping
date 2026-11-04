# piping

**piping** ist ein geplantes Rust-CLI-Tool für zuverlässige ICMP-Echo-Messungen unter Linux: klassisches Ping, schnelle Messungen, Range Pong (CIDR-/Adressbereich-Sweep) und SmokePing-artige Terminalgrafiken. Eine interaktive Fullscreen-TUI ist **optional**.

> **Status (04.11.2026): Planung/Backlog eingerichtet, noch keine Rust-Implementierung und kein geprüfter Linux-Build.** Die Beispiele unten beschreiben die festgelegte CLI-Zielschnittstelle, sind noch keine ausführbaren Beispiele.

## Ziel-CLI

```sh
piping example.com
piping -c 4 -i 1 -W 2 1.1.1.1
piping -4 -n -q 192.0.2.1
piping -6 -c 3 ::1
piping fast 1.1.1.1 --pps 20 --count 100
piping range 192.168.1.0/24 --only-alive
piping range 192.168.1.10-192.168.1.30 --jobs 16
piping monitor example.com --window 60 --ascii
piping tui example.com  # optionales Cargo-Feature 'tui'
```

`piping HOST` ist der einfache Standardmodus; auf Nicht-TTYs wird niemals automatisch eine Fullscreen-Oberfläche gestartet. Für bestehende Scripts ist nur die dokumentierte Untermenge der iputils-Flags beabsichtigt – keine vollständige binäre Kompatibilität.

## Entwicklungs-/Installationsziel (noch nicht ausführbar)

Benötigt: Linux (zuerst x86_64, dann aarch64), stable Rust/Cargo und ein ICMP-Echo-fähiges Netzwerk. Sobald #1 und die Protokoll-Issues umgesetzt sind:

```sh
cargo build --release --locked
cargo test --locked
cargo install --path . --locked
./target/release/piping --help
```

Linux-ICMP-Echo-Sockets können ohne Root funktionieren, wenn die Systemkonfiguration dies erlaubt. Das Programm verändert nie automatisch `sysctl`-Werte, fordert keine pauschalen Root-Rechte und führt keine ungefragten Scans aus. Näheres in [Sicherheit](docs/SECURITY.md).

## Organisation und Umsetzung

Dieses Repository übernimmt die **Arbeitsorganisation** des Schwesterprojekts `kostuemwettbewerb` (nicht dessen Python-/Web-Stack): [AGENTS.md](AGENTS.md), verbindliche Spezifikationen, [Roadmap](docs/ROADMAP.md), [Abnahmetests](docs/ACCEPTANCE_TESTS.md), kleine Issue-/PR-Pakete und ehrliche QA-Berichte.

- [Produkt-/CLI-Spezifikation](docs/PRODUCT_SPEC.md)
- [Architektur](docs/ARCHITECTURE.md)
- [Roadmap und Issue-Links](docs/ROADMAP.md)
- [Agentenübergaben](docs/AGENT_HANDOFF.md)
- [Sicherheitsmodell](docs/SECURITY.md)
- [Entwicklung und Prüfbefehle](docs/DEVELOPMENT.md)
- [Entscheidungen und offene Designfragen](docs/DECISIONS.md)

[GitHub-Issues](https://github.com/derdigitalaffine/piping/issues) enthalten umsetzbare Aufgaben. Der [zentrale v0.1-Tracker #17](https://github.com/derdigitalaffine/piping/issues/17) bündelt den kritischen Pfad und die Release-Abnahme. Alle Implementierungs- und Testkästchen bleiben offen, bis Code und Prüfnachweise vorliegen.

## Leitplanken

Zuerst **korrektes, lesbares, rootless-freundliches Ping**; darauf Fast/Range; dann Textgrafiken; zuletzt optionale TUI. Begrenzte PPS/Parallelität und kein ungefragter Netzscan sind Pflicht. Für SmokePing wird das **Messkonzept** (Median, Verteilung, Verlust) aufgenommen, keine Fremdsoftware oder geschützte Assets kopiert.
