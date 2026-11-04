# Agenten-Übergabe und PR-Checkliste

## Überblick

Das Arbeitsprinzip entspricht dem Schwesterprojekt: jedes Ticket ist abnahmefähig, Agenten arbeiten in festgelegter Reihenfolge, dokumentieren ihre Entscheidungen, halten den Kritischen Pfad frei und hinterlassen einen reproduzierbaren QA-Nachweis.

| Agent | Issues | Übergabe an |
| --- | --- | --- |
| Foundation | #1, #2 | API von CLI, Error/Exitcodes, Feature-/CI-Basis an alle |
| Protocol | #3, #4, #6 | Socket-Trait, Probe-/Reply-Metadaten, Rechtefälle an Engine |
| Engine/Stats | #5, #7 | Event-Schema, Scheduler, Cancellation und Messstatistik an Range/Output |
| Range | #8 | Zieliterator/Limit-/Summary-Schema an Output/QA |
| Graph/Text | #9, #11 | UI-freie Renderer-APIs an optionalen TUI-Agent |
| Optional TUI | #10 | opt-in, keine API-Änderung in Engine ohne Tests |
| Security/QA | #12, #14 | Review, Limit-Protokoll, reale Prüfnachweise an Release |
| Docs/Release | #13, #15, #16 | getestete Linux-Kommandos und Release-Entscheidung |

## Vor jeder Implementierung

- [ ] `AGENTS.md` und docs gelesen
- [ ] P0/P1/P2, Scope und Abhängigkeiten aus Issue bestätigt
- [ ] Exakte Schnittstellen und invarianten definiert
- [ ] Happy-Path und Fehlerfalltests geplant
- [ ] Möglichst eigener PR-Branch statt Änderungen parallel in `main`

## PR-Vorlage (inhaltlich)

1. `Closes #...` und Ziel/Änderungen.
2. CLI- und Schnittstellenänderungen, neue Abhängigkeiten.
3. Security/Performance: Socket-Typ, Rechte, Rate/Queue-Limits, Parser.
4. Konkrete Testkommandos **mit Resultat** und OS/Rust-Version.
5. Nicht getestete Fälle, bekannte Limits und dokumentierter Fallback.
6. Geänderte Spezifikation/Architektur/README bei jeder Nutzerschnittstellenänderung.

## Übergaberegeln

Kein Agent darf stillschweigend mit einem eigenen Paketformat, Exitcode oder globalem Scheduler beginnen. Öffentliche Interfaces zuerst im Foundation-/Protocol-/Engine-PR festlegen und dann testen. Keine „fertig“-Behauptung nur aus Code-Lieferung: Gültigkeit ergibt sich aus Tests/CI/DoD.
