# pipi

**pipi** ist ein Rust-CLI-Tool für zuverlässige ICMP-Echo-Messungen unter Linux: klassisches Ping, schnelle Messungen, Range Pong (CIDR-/Adressbereich-Sweep) und SmokePing-artige Terminalgrafiken. Eine interaktive Fullscreen-TUI ist **optional**.

> **Entwicklungsstand (04.11.2026):** Aus dem bisherigen Binary \`piping\` wird **\`pipi\` 0.2.0**. GitHub-Repository und Clone-URL bleiben \`derdigitalaffine/piping\`. Neue Features: MTR-artiges Trace mit Smoke pro Hop, kompakte Ping-/Trace-TUI, automatisch gruppierte ARP-/NDP-Nachbarschaftsgrafik. **Die aktuelle GitHub-Actions-Infrastruktur startet keine Jobs; Rust-/Linux-Tests und Produktivreife der neuen Version sind noch nicht nachgewiesen.** Der ältere grüne [CI-Lauf #41807144867](https://github.com/derdigitalaffine/piping/actions/runs/41807144867) gilt nur für den vorherigen \`piping\`-Stand.

## Neu: Trace / Neighborhood / TUI

\`\`\`sh
pipi trace -c 3 --smoke --max-hops 16 127.0.0.1
pipi trace --tui --smoke 1.1.1.1
pipi neighborhood                       # passive lokale Netzwerkumgebung
pipi neighborhood --tui                 # automatisch aktualisierte grafische Ansicht
pipi neighborhood --scan-local -i eth0  # explizit aktive private ARP-Erkennung
pipi discover --family 6 --output jsonl
\`\`\`

**Installation mit TUI:** \`cargo install --path . --locked --features tui\`. Details zu Grenzen und Privilegien: [Trace + Neighborhood](docs/TRACE_NEIGHBORHOOD.md) und [Discovery](docs/DISCOVERY.md).

## Basisbefehle

```sh
pipi example.com
pipi -c 4 -i 1 -W 2 1.1.1.1
pipi -4 -n -q 192.0.2.1
pipi -6 -c 3 ::1
pipi fast 1.1.1.1 --pps 20 --count 100
pipi range 192.168.1.0/24 --only-alive
pipi range 192.168.1.10-192.168.1.30 --jobs 16
pipi monitor example.com --window 60 --ascii
pipi tui example.com  # optionales Cargo-Feature 'tui'
```

`pipi HOST` ist der einfache Standardmodus; auf Nicht-TTYs wird niemals automatisch eine Fullscreen-Oberfläche gestartet. Für bestehende Scripts ist nur die dokumentierte Untermenge der iputils-Flags beabsichtigt – keine vollständige binäre Kompatibilität.

## Linux-Build und Installation

Benötigt: Linux (zuerst x86_64, dann aarch64), stable Rust/Cargo und ein ICMP-Echo-fähiges Netzwerk. Aus dem Hauptbranch bauen:

```sh
cargo build --release --locked
cargo test --locked
cargo install --path . --locked
./target/release/pipi --help
# Optionale Fullscreen-UI:
cargo build --release --locked --features tui
./target/release/pipi tui ::1
```

DNS-Auflösung wird auf fünf Sekunden begrenzt; JSONL/CSV enthalten UTC-Epoch-Millisekunden. Die CI prüft zusätzlich `cargo install` aus einem unabhängigen Build-Verzeichnis.

Linux-ICMP-Echo-Sockets können ohne Root funktionieren, wenn die Systemkonfiguration dies erlaubt. Das Programm verändert nie automatisch `sysctl`-Werte, fordert keine pauschalen Root-Rechte und führt keine ungefragten Scans aus. Näheres in [Sicherheit](docs/SECURITY.md) und der [CLI-Referenz](docs/CLI.md).

## Organisation und Umsetzung

Dieses Repository übernimmt die **Arbeitsorganisation** des Schwesterprojekts `kostuemwettbewerb` (nicht dessen Python-/Web-Stack): [AGENTS.md](AGENTS.md), verbindliche Spezifikationen, [Roadmap](docs/ROADMAP.md), [Abnahmetests](docs/ACCEPTANCE_TESTS.md), kleine Issue-/PR-Pakete und ehrliche QA-Berichte.

- [Ausführbare CLI-Referenz](docs/CLI.md)
- [Produkt-/CLI-Spezifikation](docs/PRODUCT_SPEC.md)
- [Architektur](docs/ARCHITECTURE.md)
- [Roadmap und Issue-Links](docs/ROADMAP.md)
- [Agentenübergaben](docs/AGENT_HANDOFF.md)
- [Sicherheitsmodell](docs/SECURITY.md)
- [Entwicklung und Prüfbefehle](docs/DEVELOPMENT.md)
- [QA-Status und tatsächliche Prüfergebnisse](docs/QA_REPORT.md)
- [Release-Anleitung](docs/RELEASE.md)
- [Entscheidungen und offene Designfragen](docs/DECISIONS.md)

[GitHub-Issues](https://github.com/derdigitalaffine/piping/issues) enthalten umsetzbare Aufgaben. Der [zentrale v0.1-Tracker #17](https://github.com/derdigitalaffine/piping/issues/17) bündelt den kritischen Pfad und die Release-Abnahme. Offene Implementierungs- und Testkästchen werden erst nach nachgewiesener Abnahme geschlossen.

## Leitplanken

Zuerst **korrektes, lesbares, rootless-freundliches Ping**; darauf Fast/Range; dann Textgrafiken; zuletzt optionale TUI. Begrenzte PPS/Parallelität und kein ungefragter Netzscan sind Pflicht. Für SmokePing wird das **Messkonzept** (Median, Verteilung, Verlust) aufgenommen, keine Fremdsoftware oder geschützte Assets kopiert.
