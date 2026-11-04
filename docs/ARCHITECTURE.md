# Architektur – geplantes Rust-Linux-CLI

## Abhängigkeitsrichtung

`CLI → Application/Engine → ICMP transport`; `Engine → Events/Stats → Text/Graph/JSONL`. Das optionale `TUI`-Crate hängt von Events ab, die Messengine jedoch nie von der TUI. Range und Fast verwenden **denselben** Scheduler/Transport/Timeout-Mechanismus statt unabhängiger ICMP-Pfade.

```text
CLI parser / config (clap oder gleichwertig)
      │
      ▼
Runner / cancellation / signal
      │
      ├── single target ping
      ├── fast target ─┐
      ├── range sweep ─┴── bounded scheduler / deadlines / token bucket
      │                            │
      │                       ICMP transport
      │                   Linux dgram ping sockets
      │                  ICMPv4 / ICMPv6 adapters
      │                            │
      ▼                            ▼
Probe events  ◄──────── validated replies / monotonic RTT
      │
      ├── aggregator: loss / min / avg / max / mdev / p50 / p95 / jitter
      ├── classic text stdout (core)
      ├── smoke rolling chart (text, core)
      ├── jsonl/csv (P1)
      └── ratatui/crossterm fullscreen (optional Cargo feature)
```

## Verzeichnisstruktur (Ziel, noch nicht vorhanden)

- `Cargo.toml` / `Cargo.lock` – Linux-only Binärcrate, explizite optionale Features.
- `src/main.rs` – Einstieg, Signale, CLI-Resultate und Exitcodes.
- `src/cli.rs` – Argument-/Konfigurationsvalidierung.
- `src/icmp/{mod,v4,v6}.rs` – IP-Familien, Enc/Dec, Validierung, Socket.
- `src/engine/{mod,scheduler,probe}.rs` – send/receive, Deadline, token bucket, cancellation.
- `src/stats.rs` – reine Messstatistik.
- `src/range.rs` – lazy, begrenzte Netziteratoren.
- `src/output/{classic,smoke,machine}.rs` – Renderers ohne Netzwerk.
- `src/tui.rs` – optional, nur via Feature.
- `tests/` – Synthetic/Loopback/CLI-Golden.
- `docs/` – Architektur, Vertrag, Sicherheit, QA, Entwicklung, Release.

## Messinvarianten

- Ein Probe-Key umfasst mindestens IP-Familie, Ziel, Socket/Identifier, Sequenznummer und Sende-Epoch, soweit API verfügbar; verworfene Replies dürfen nicht fremde Messungen erfüllen.
- Receive-Parser prüft ICMP-Typ, Code, Nutzdatenlänge und Echo-Zuordnung. ICMP-Error-Replies differenziert behandeln; nie Erfolg ohne Echo Reply.
- Intervalle, Timeouts und RTT ausschließlich mit `Instant`/monotoner Zeit; Wandzeit nur Ausgabe.
- Keine unbegrenzte HashMap mit ausstehenden Antworten; alle Probes laufen per Deadline aus.
- Fast und Range teilen globales PPS-Limit und maximale simultane Probes. Fairness über Hosts; keine stillen Nebenaktionen.
- Aggregator zählt sent/received/duplicate/late konsistent; Loss wird aus sent und gültigen replies berechnet.
- Ctrl-C/Cancellation stoppt Senden, räumt Queues/Sockets auf und druckt Abschlusssummary.

## Linux Socketstrategie

`SOCK_DGRAM + IPPROTO_ICMP` als Standard; `net.ipv4.ping_group_range`/Systemrestriktionen diagnostizieren. Nicht aus „standard ping geht“ ableiten, dass alle Container/Namespaces dieselben Rechte haben. ICMPv6-Socket-/Checksum-Unterschiede gesondert testen. Raw-Socket-Fallback ist **nicht** automatisch oder verpflichtend; Privilege-Mode braucht eigenes Security Review.

## Rust/CI

Bevorzugt gut gewartete, kleine Rust-Crates statt eigener Unsafe-FFI; `socket2` ist eine Kandidatin, Versionsentscheidung in #1/#3 treffen und dokumentieren. Tokio/async erst nach Benchmark/Komplexitätsbegründung, keine unnötige Runtime im MVP. GitHub Actions Linux stable: fmt, clippy, test, release build, Features/IPv4/IPv6; echte Loopback-Integration nur wenn erlaubte Sockets verfügbar. Kein CI-Grün ohne tatsächlichen Rust-Code und laufende Prüfungen.

## Skalierungsstrategie

Single target first; danach bounded multi-target send/receive. Große Bereiche als Lazy-Iterator statt Materialisierung, Hard-Caps auf hosts, workers, PPS und payload; Ausgaben streamen. TUI empfängt bounded Broadcast-/Ringbuffer, darf die Messengine nie blockieren.

## M6 Fastpath und Discovery

Die ICMP-Messengine verwendet einen vorinitialisierten Payload-Buffer, ersetzt nur 24 Bytes pro Probe, leert Receive-Queues direkt in einen Callback ohne per-Wakeup-Vec, und schläft per Linux poll ohne fixen 200-ms-Wecker. Für Range läuft eine begrenzte `VecDeque`-Bereitliste: ein Host wird erst nach Abschluss eines Echo/Timeout wieder freigeschaltet, O(1) pro Auswahl. Range-Stats werden ohne RTT-Percentile-Historie pro Host geführt.

Layer-2 ist vom ICMP-Engine-Modul getrennt: `src/discovery.rs` validiert CLI und Ausgabe, `src/neighbor.rs` liest read-only RTM_GETNEIGH / NLM_F_DUMP mit defensivem, endian-korrektem Parser, `src/arp.rs` sendet nur bei `--active` minimal gültige ARP-Ethernet-Frames. Keine zusätzlichen Cargo-Abhängigkeiten; der Standard-Ping öffnet keine Raw-Ethernet-Sockets. Ein gescheitertes/ausstehendes CI-Run ist kein Nachweis von Funktion oder Performance.
