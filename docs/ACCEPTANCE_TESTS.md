# Abnahme- und Qualitätstestmatrix – v0 Plan

**Status:** Alle Tests zunächst **OFFEN**. Anforderungen und Tests gehören zu den Issues, nicht bloß in eine CI-YAML. Vor 0.1.0 Ergebnisse als PASS / FAIL / SKIP + Grund/Umgebung protokollieren.

| ID | Test | Erwartung |
| --- | --- | --- |
| A01 | `cargo build --release --locked` auf Linux stable | Binary buildet; Version/Arch dokumentiert |
| A02 | `cargo fmt`, `clippy -D warnings`, `cargo test` | Alle relevanten Checks grün |
| A03 | `piping --help` ohne Socketrechte | Return 0 und vollständige Hilfe |
| A04 | `piping -4 -c 3 127.0.0.1` | 3 gezählte Requests, RTT/Statistik, Exit 0 bei Reply |
| A05 | `piping -6 -c 3 ::1` | richtige IPv6-Antworten; Skip nur mit Begründung |
| A06 | nicht antwortender Mock-Host | Timeout, 100 % Loss, Exit 1, keine Division durch Null |
| A07 | ungültige CLI-Flags / Adresse | Exit 2, klare stderr-Meldung, keine Panic |
| A08 | Fake-Replies: falscher Typ/ID/Seq/Quelle/Payload | werden nicht als Erfolg gezählt |
| A09 | Duplicate, late, out-of-order | korrekte Statistik, kein Doppelzählen |
| A10 | `-c`, `-i`, `-W` und Ctrl-C | korrekte Probezahl, Deadline, Schlussstatistik |
| A11 | monotone Uhr mit Fake Time | RTT, min/avg/max/mdev, p50/p95/Jitter korrekt |
| A12 | `fast` mit MockClock und überlasteter Queue | Rate- und Outstanding-Caps eingehalten, sauberer Abbruch |
| A13 | `range` CIDR, Start–Ende, `/31` und `/32` | deterministisches Zielset ohne unendliche Expansion |
| A14 | großes Netz, falsche Range, IPv6 /64 | Limitabbruch, keine Speicherexplosion, keine ungefragten Scans |
| A15 | Smoke-Buckets mit Verlust/Ausreißer | Median, Spread und Verlust korrekt, stabile Golden-ASCII-Ausgabe |
| A16 | TTY 40/80/120 Zeichen, no-color, Pipe | kein unlesbarer/roher ANSI-Output, kein Zwangsfullscreen |
| A17 | unprivilegierter Nutzer ohne Ping-Socket-Freigabe | verständliche Fehlermeldung, kein Auto-Sudo |
| A18 | Socket-Datagramm + ICMPv6 Kernel-Prüfsummen | richtige Semantik, auf Linux separat verifiziert |
| A19 | `--output jsonl`, `--quiet` (P1) | schema-konform, stdout/stderr-Trennung |
| A20 | `--features tui` und `--no-default-features` (P2) | Core ohne TUI baubar, TUI hält Terminalzustand sauber |
| A21 | Payload/Target/PPS/Jobs-Grenzen | Fehlerpfade schließen Sockets, Queue bleibt bounded |
| A22 | frischer Linux Checkout/Install | README-Kommandos reproduzierbar, keine privilegierte Installation nötig |

## Definition of Done für P0

- Kein Test wird ohne tatsächlichen Lauf grün geschrieben.
- A01–A18, A21–A22 müssen für Zielumgebung bestanden sein; bei untestbarer IPv6-/Socket-Umgebung ist Release-Entscheidung samt Einschränkung ausdrücklich zu dokumentieren.
- A19 und A20 gelten nur, sobald P1/P2 implementiert sind.
- Korrektheit + Sicherheits-/Ratenlimits stehen über Microbenchmarks.
- Keine Performance-Tests gegen fremde öffentliche Hosts; keine CI mit produktiven Netz-Zielen.
