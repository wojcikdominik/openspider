# Unabhängige Abnahmetests

Nutzerentscheidung 06.11.2026, gilt ab den beta.4-Paketen (B4-*).

## Warum

Bisher schreibt der bauende Agent Code und Tests eines Pakets selbst; getrennt waren nur Vorprüfung und Merge-Freigabe (Fable 5.1, frischer Kontext). Damit Tests nicht nur bestätigen, was der Code tut, schreibt ein zweiter Agent zusätzliche Abnahmetests: anderes Modell, ohne den Arbeitsverlauf des Bauers, nur aus der Spezifikation.

## Rollen

| Rolle | Modell | Kontext |
|---|---|---|
| Bau (Code und eigene Tests) | Claude Opus 5.5 | Paketauftrag, Spec, Code |
| Abnahmetests | Claude Sonnet 5.5 | nur Spec-IDs des Pakets, Abnahmekriterien, öffentliche Schnittstellen (CLI, MCP, IPC, UI); **nicht** Bericht, Commit-Texte oder Verlauf des Bauers |
| Merge-Freigabe | Fable 5.1 | frisch: Diff, beide Testsätze, Berichte, Spec |

## Ablauf je Paket

1. Der Orchestrator legt die Spec-IDs des Pakets fest und startet den Testagenten, sobald der Bau-Branch eine prüfbare Schnittstelle hat (spätestens bei „PAKET FERTIG“).
2. Der Testagent arbeitet auf einem eigenen Branch `<bau-branch>-tests` ab dem Stand des Bau-Branches und ändert **nur Testdateien**. Erwartetes Verhalten kommt ausschließlich aus `docs/spec/*`. Code liest er nur, um zu wissen, wie etwas aufgerufen wird. Widerspricht der Code der Spec, bleibt der Test spec-treu und wird als Befund gemeldet, nicht an den Code angepasst.
3. Tests bevorzugt von außen: CLI-Aufrufe, MCP-Protokoll, IPC-Befehle, Playwright gegen die Oberfläche, Fixtures aus `os-testsite`. Jeder Test trägt die Spec-ID (`// spec: AGENT-020`) und die Markierung `// acceptance: independent`.
4. Ergebnis: Bericht `docs/plan/M3-reports/<paket>-tests.md` mit Tabelle Spec-Kriterium → Test → grün/rot, Befunden und Spec-Unklarheiten.
5. Rote Tests sortiert der Orchestrator: Produktfehler → zurück an den Bauer; Spec-Unklarheit → Entscheidung (Fable oder Nutzer); Testfehler → zurück an den Testagenten. Kein Test wird gelockert, um grün zu werden (K2-63).
6. Fable prüft Bau-Diff und Abnahmetests zusammen; gemergt wird beides gemeinsam.

## Ausnahmen

Pakete ohne Verhaltensänderung (nur Nachweis-Markierungen, Doku, CI-Konfiguration) brauchen keinen Testagenten; der Orchestrator nennt das im Merge-Titel.
