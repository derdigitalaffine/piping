# Produkt- und CLI-Spezifikation (v0, Planungsstand)

## Kernziel

`pipi` ist ein Linux-first Rust-ICMP-Echo-Werkzeug, das **ohne Oberfläche als normaler Ping** funktioniert. Drei Erweiterungsmodi: Fast Ping, Range Pong und SmokePing-artiger Konsolen-Monitor. Die Advanced Shell UI (TUI) ist optional.

## Bedienung (geplanter CLI-Vertrag)

| Modus | Aufruf | Verhalten |
| --- | --- | --- |
| Classic | `pipi HOST` | sequenzielle Echo-Messung, Standardzeilen und Statistik, bis Ctrl-C |
| Classic begrenzt | `pipi -c 4 -i 1 -W 2 HOST` | vier Probes, Abstand/Timeout in Sekunden |
| IPv4/IPv6 | `pipi -4 HOST` / `pipi -6 HOST` | explizite IP-Familie |
| Fast | `pipi fast HOST --pps 20 --count 100` | scheduler-gesteuerte Probes, bounded outstanding |
| Range Pong | `pipi range 192.168.1.0/24` | Ping-Sweep, Zusammenfassung alive/down/RTT |
| IP-Range | `pipi range 192.168.1.10-192.168.1.30` | inklusive IPv4-Endpunkte |
| Smoke-Text | `pipi monitor HOST --window 60` | rollende ASCII/Unicode-Zeitgrafik, Median/Loss/Spread |
| TUI optional | `pipi tui HOST` | optionales Cargo-Feature, nicht für Standardbetrieb nötig |

Geplante Standardoptionen: `-c/--count`, `-i/--interval`, `-W/--timeout`, `-4`, `-6`, `-n/--numeric`, `-q/--quiet`, `--ttl`, `--size`, `--ascii`, `--no-color`, `--output text|jsonl|csv` (Ausgabeformate P1). Flag-Bedeutungen mit iputils ausdrücklich prüfen; keine implizite vollständige Kompatibilitätsgarantie.

## Ausgabevertrag

- Classic: `PING target (addr)`; pro Echo beispielsweise `64 bytes from ...: icmp_seq=... ttl=... time=... ms`; Schluss: gesendet/empfangen/Verlust und min/avg/max/mdev, **ohne** erfundene Felder, wenn Kernel sie nicht bereitstellt.
- Kein Echo → Statistik mit 100 % Verlust und Exitcode 1; ungültige Argumente/Socketfehler → Exitcode 2; mind. ein gültiges Echo → 0. Range-Exitcode: 0 wenn mind. ein Host antwortet, 1 wenn keiner antwortet, 2 bei Parse-/Systemfehler; Teilverluste in Statistik, später optional `--require-all`.
- Stdout: Messereignisse/Statistik; Stderr: Diagnostik. `--quiet` unterdrückt Einzelprobes, nicht Schlussstatistik.
- JSONL (P1): getrennte `probe`, `timeout`, `summary`-Events mit Schema-Version; Zeitstempel UTC, monotone RTT in ms; keine ANSI-Bytes. CSV nur klar definierte Felder.
- `NO_COLOR`, `--no-color` und Pipes/Redirect werden respektiert. TUI nie automatisch.

## Fast Ping

Default-`--pps` 20, per user konfigurierbar bis zur später festzulegenden Sicherheitsgrenze; bounded Queue/Concurrency/Timeouts. `--count` zählt gesendete Probes, nicht abgeschlossene Wiederholungen. Rate-Limits gelten auch bei DNS-/ICMP-Fehlern. `--pps` ist ein **Limit**, keine Leistungsgarantie.

## Range Pong

Interpretiert als Echo-Sweep über IPv4-CIDR, begrenzte IPv6-CIDR und inklusive IPv4-Endpunktbereiche. Default maximal 256 Ziele, `--max-hosts` explizit bis zum später definierten harten Cap; `--jobs` und `--pps` global begrenzt. Default IPv4-CIDR-Verhalten bezüglich Netz-/Broadcast-Adresse wird in #8 als testbarer Kontrakt festgelegt; für `/31` und `/32` müssen Randfälle definiert sein. IPv6 nie vollständig aus großem Präfix expandieren.

Range-Sweep ist nur nach explizitem Subcommand gestattet. Kein Portscan, kein TCP-Connect-Fallback, kein automatisches wiederholtes Scannen großer öffentlicher Adressbereiche.

## Smokeping-ähnliche Anzeige

Ein Zeitbucket fasst mehrere Echo-Messungen zusammen: Median als Zentrum, Ausbreitung (p10–p90 oder Histogramm) als „Smoke“, Ausfallquote separat, dazu p95/Jitter. ANSI/Unicode nur terminaltauglich; ASCII-Fallback und maschinenlesbares Ereignisformat ohne TUI. Keine grafischen Fremdassets oder verwechslungsfähige Bezeichnung „SmokePing“ als Produktname.

## Nichtziele des MVP

Traceroute, TCP-/UDP-Probe, Portscanner, Privileg-Setup-Assistent, Netzwerk-Daemon, Prometheus-Webserver, Traffic-Flood, Speichern von fremden Zugangsdaten, versprochene fping-Überlegenheit, Windows/macOS-Support. Erweiterungen separat priorisieren.
