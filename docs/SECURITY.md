# Sicherheit, Privilegien und Responsible Network Use

## Threat Model

Piping sendet ICMP-Echo-Pakete und verarbeitet fremde Netzwerkantworten. Risiken: falsche/gespoofte Echo-Antworten, übergroße oder defekte Pakete, unkontrollierter Versand (Fast/Range), Speicherwachstum, falsche RTT-/Loss-Statistik, unerwünschte Privilegien und Terminal-Escape-Sequenzen.

## Rechte

- Auf Linux bevorzugt `SOCK_DGRAM`/`IPPROTO_ICMP`: wenn die ping-GID-Range des Kernels es erlaubt, sind ICMP-Echo-Sockets auch ohne CAP_NET_RAW möglich. Verhalten variiert nach Host/Container/Netns.
- Keine pauschalen `sudo`-Anweisungen, keine eigenmächtige `sysctl`-Anpassung, keine Setuid-Installation, kein automatisches `setcap`.
- Sollte ein Raw-Socket-Pfad nötig werden, muss er separat designreviewt und kleinräumig dokumentiert werden. CAP_NET_RAW ist kein Default-Recht.
- Diagnose: `cat /proc/sys/net/ipv4/ping_group_range` zur Einsicht; nötige Systemänderungen liegen allein beim Administrator.

## Netzwerkrücksicht

- Standard ist ein einzelner Host. Range nur durch expliziten Subcommand, Fast nur durch expliziten Subcommand.
- Konservative PPS (Fast default 20), maximale Jobs, Hostzahl, Queue, Payload und Gesamtlaufzeit; harte Grenzen in #14.
- Kein Ping-Flood ohne ausdrückliche Einstellung, keine Paket-Flut-Benchmarks gegen öffentliche IP-Adressen.
- Rate-Limit zählt **gesendete** Probes aller Ziele und Wiederholungen, nicht nur Antworten.
- Kein IP-/Subnetz-Discovery im Hintergrund und keine externen Dienste/Telmetrie.

## Paketvalidierung

- ICMP-Typ, Code, tatsächliche Paketlänge, ID, Seq, Payload-Marker/Nonce sowie (wenn möglich) erwartete Quelle prüfen.
- Identifier kann vom Kernel verwaltet werden – Semantik per Socket-Modus testen.
- ICMPv4 vs. ICMPv6 Parsing, Checksums und Linux-Datagram-Verhalten getrennt handhaben.
- Duplicate, Late, Wrong-Host, Errors und Out-of-Order explizit behandeln.
- Overflow, untergroße Pakete, leere Statistiken und Zeitquellen-Umrechnung gegen Panics absichern.

## CLI/Terminal/Logs

- Terminalkontrollzeichen aus DNS-/Hostnamen und Netzwerkdaten neutralisieren; kein roh interpolierter ANSI-Text.
- `NO_COLOR`, redirect, CSV/JSON ohne Terminal-Sequenzen; Fullscreen nur opt-in.
- Keine Root-Rechte anfordern um Hilfe, Version oder Parser-Tests zu zeigen.
- Kein persistentes Recording oder externe Uploads im MVP; mögliche PII in Ziel-Hostnamen und Logfiles bei späterer Persistenz beachten.
- Fuzz-/Property-Tests und Grenzwert-Tests aus `docs/ACCEPTANCE_TESTS.md` ausführen.

## Scope & Reporting

Das Werkzeug ist für die Diagnose eigener bzw. autorisierter Netze vorgesehen. Test-Harness benutzt Loopback und isolierte Netze. Offene Sicherheits-/Performance-Grenzen bleiben vor Release dokumentiert statt ignoriert.

## Layer-2 Discovery – neue, getrennte Vertrauensgrenze

- `pipi discover` ist passiv (RTM_GETNEIGH/NETLINK_ROUTE) und sendet keine Pakete; Kernel-Cache-Zustände können veraltet, leer oder manipuliert sein.
- Nur `pipi discover --active CIDR --interface IFACE` öffnet einen Ethernet-`AF_PACKET`-Raw-Socket. Dies **erfordert** CAP_NET_RAW. Das Programm erhöht Rechte nicht selbst, startet keine Shell, schreibt keine Sysctls und ruft `sudo` nie auf.
- Aktive Scans prüfen ein direkt am gewählten Interface angeschlossenes IPv4-Netz in `/proc/net/route`, erlauben maximal 256 Ziel-IPs, höchstens 100 Pakete/s, maximal 5s Empfangsfenster, und schließen eigene Quelladresse aus.
- Ethernet-/ARP-Reply-Felder, Quell-/Ziel-MAC und solicited-IP werden validiert; ARP bietet **keine** kryptografische Authentifizierung (Spoofing möglich). IPv6-NDP-Discovery bleibt passiv.
- Netlink-Attribute werden über geprüfte Byte-Slices analysiert, mit maximal 8192 Cache-Einträgen und 65-KiB wiederverwendetem Empfangspuffer. Keine Interpretation unsicherer Netlink-Daten als Terminal-Steuersequenzen.
- Aktiver Linux-CI-Test nutzt ausschließlich ein temporäres veth-Paar und 198.18.0.0/30. Wenn Host-CAP_NET_ADMIN fehlt, wird **SKIP**, nicht PASS gemeldet.
