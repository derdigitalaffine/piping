## Ziel

Closes #...

### Änderung und Benutzereffekt

### Schnittstellen/CLI/Dokumentation

### Netzwerk-/Sicherheitsauswirkungen

- Socketart / Rootless / Capabilities:
- ICMP-Validierung / Deadline / Timeout:
- PPS / outstanding-Probes / Hostlimits / Memory:
- Ausgabe und ANSI/TTY/NO_COLOR:

### Echte Tests und Umgebung

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] `cargo test --locked`
- [ ] `cargo build --release --locked`
- [ ] Linux Loopback / IPv4
- [ ] Linux Loopback / IPv6
- [ ] Docs und `--help` synchron

**Ausgeführt (Kommando, Ergebnis, Kernel/Rust):**

**Nicht ausgeführt / SKIP mit Grund:**

### Risiken, Fallback, offene Abnahme

Bitte keine Häkchen setzen ohne ausgeführte Prüfung. Vorgaben aus `AGENTS.md` und `docs/ACCEPTANCE_TESTS.md` beachten.
