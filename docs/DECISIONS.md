# Architekturentscheidungen und Auslegung (04.11.2026)

## D-001: Projektsprache und Scope

**Entscheidung:** Rust 2024, Linux-first, ein einzelnes ausführbares `pipi`, kein verpflichtender Service und keine Weboberfläche. Stable Rust/MSRV werden im Foundation-Issue festgelegt.

## D-002: Standard-CLI bleibt einfach

**Entscheidung:** `pipi HOST` liefert Classic-Ping, mit `-c`/`-i`/`-W` und `-4/-6`. TUI ausschließlich explizit. Keine vollständige iputils-Drop-in-Kompatibilität behaupten.

## D-003: „Range Pong“

**Arbeitsinterpretation:** gezielter ICMP-Echo-Sweep über CIDR und IPv4-Start–Ende. IP-/CIDR-Grenzen und Explosionsschutz sind Pflicht. Falls später ein anderes „Range Pong“-Protokoll gewünscht ist, braucht es ein separates Issue und angepasste Tests.

## D-004: „Fast Ping“

**Entscheidung:** bounded, rate-limited, parallele ICMP-Probes mit präziser monotonic time. Standard 20 PPS, Nutzer kann bewusst mehr konfigurieren, harte Grenze vor MVP-Release. Kein unlimitierter Flood.

## D-005: „SmokePing-artig“

**Entscheidung:** Median plus Streuung/Häufigkeit pro Zeitbucket, Verlustindikator sowie p95/Jitter in einer Textgrafik. Funktionale Idee, keine Integration/Kopie von SmokePing-Software oder Logos.

## D-006: Rechte

**Entscheidung:** Linux datagram ping sockets zuerst; kein systemverändernder Installationsschritt; fehlende Berechtigung klar erklären. Raw fallback nur nach speziellem Review.

## D-007: Agentenorganisation

**Entscheidung:** ``AGENTS.md``, Spezifikation, Architektur, Roadmap, Acceptance Tests, Tracking-Issue, PR pro Issue und QA-Vermerke nach `kostuemwettbewerb`-Muster; team-/domänenspezifische Rollen für Rust/ICMP statt Django.

## Noch vor dem Code zu präzisieren

- exakte `-W`/`-i`-Einheiten und iputils-Verträglichkeit (`#2`)
- ICMPv6-Checksum/ID/Scope-ID API und erforderliche Crate-Version (`#4`)
- harte `--pps`-/`--jobs`-/`--max-hosts`-/Payload-Grenzen (`#14`)
- Range-`/31`-`/32`- und Netz-/Broadcast-Semantik (`#8`)
- Linux-Versionen/MSRV/zweite CPU-Architektur (`#1`/`#15`)
- Lizenzentscheidung beim Maintainer (`#15`)

Offene Punkte sind keine stillschweigenden Implementierungsannahmen.
