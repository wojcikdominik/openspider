ai-settings-title = KI-Einstellungen
settings-section-ai = KI
config-page-ai = KI

## Abschnitt KI der App-Einstellungen
ai-settings-tab-costs = Kosten und Datenschutz
ai-settings-tab-dataflow = KI-Datenfluss
ai-settings-tab-advanced = Erweitert
ai-settings-tab-accounts = Konten
ai-settings-apply = Übernehmen
ai-settings-saved = Gespeichert
ai-settings-usd-invalid = Bitte einen Betrag in USD eingeben, z. B. 1,00.
ai-settings-threshold = Bestätigung verlangen ab (USD erwarteter Kosten)
ai-settings-threshold-hint = Liegt der Kostenvoranschlag darüber oder über der Hälfte des Restbudgets, startet kein KI-Request ohne „Starten“.
ai-settings-eu-title = EU-Verarbeitung
ai-settings-eu-hint = Unter der Richtlinie „EU-Verarbeitung erzwingen“ sind nur Anbieter zulässig, deren EU-Endpunkt eingeschaltet und deren Konto bestätigt ist.
ai-settings-eu-use = EU-Endpunkt verwenden ({ $url })
ai-settings-eu-confirmed = Konto bzw. Projekt ist für EU-Verarbeitung freigeschaltet
ai-settings-eu-not-allowed = Unter „EU-Verarbeitung erzwingen“ derzeit nicht zulässig.
ai-settings-eu-anthropic = Anthropic bietet keinen EU-Endpunkt; unter „EU-Verarbeitung erzwingen“ erhalten Anthropic-Aufgaben „Skipped (policy)“.
ai-settings-redaction-title = Schwärzung
ai-settings-redaction-switch = Personenbezogene Daten vor dem Senden schwärzen
ai-settings-patterns = Eigene Muster (ein regulärer Ausdruck je Zeile)
ai-settings-patterns-save = Muster speichern
ai-settings-pattern-invalid = Ungültiges Muster in Zeile { $line }, Position { $position }.
ai-settings-advanced-hint = Gleichzeitige Anfragen je KI-Anbieter. Anbieter mit eigener Vorgabe zeigen deren Standard.
ai-settings-concurrency = Gleichzeitige Anfragen an { $provider }
ai-settings-concurrency-hint = Standard: { $standard } · Bereich { $min }–{ $max }
ai-settings-concurrency-invalid = Bitte eine ganze Zahl von { $min } bis { $max } eingeben.
ai-settings-accounts-later = Schlüssel und Verbindungstest der KI-Anbieter folgen in einer späteren Version der Vorschau.

## Konfigurationsseite KI: Modellwahl je Aufgabe (AI-011)
ai-tasks-title = Modellwahl je Aufgabe
ai-tasks-hint = Je Aufgabentyp ein Modell mit Ersatzliste. Die Reasoning-Stufe gilt für den Aufgabentyp, nicht für einzelne Fragen.
ai-tasks-policy-unavailable = Die Richtlinie dieses Profils (EU-Verarbeitung bzw. Nur lokal) wird in dieser Version nicht unterstützt; bitte „Keine Einschränkung“ verwenden.
ai-tasks-error = Die Modellwahl konnte nicht geprüft werden.
ai-col-task = Aufgabe
ai-col-model = Modell
ai-col-reasoning = Reasoning-Stufe
ai-col-max-tokens = Ausgabegrenze
ai-col-fallback = Ersatzliste
ai-col-status = Zustand
ai-col-actions = Aktionen
ai-task-model = Modell für { $task }
ai-task-model-default = Standard ({ $model })
ai-task-no-default = kein Standardmodell
ai-task-unassignable = Nicht zuweisbar: { $models }
ai-task-incapable = Dem Modell fehlt eine benötigte Fähigkeit: { $missing }
ai-task-reasoning = Reasoning-Stufe für { $task }
ai-task-reasoning-default = Standard der Aufgabe ({ $level })
ai-task-effective = Wirksam: { $level }
ai-task-levels-unknown = Stufen des Modells noch nicht bekannt (Modellliste nicht geladen).
ai-task-no-level = ohne Stufe
ai-task-warn-off = Abschalten ist für dieses Modell nicht verfügbar – es gilt die niedrigste Stufe: { $effective }.
ai-task-warn-stored = Die gespeicherte Stufe { $stored } ist für dieses Modell nicht verfügbar; es gilt { $effective }. Der gespeicherte Wert bleibt erhalten.
ai-task-max-tokens = Ausgabegrenze für { $task } (leer = Standard)
ai-task-max-standard = Standard: { $standard } Tokens
ai-task-max-formula = Standard: 2 × (64 + 16 × W) + Reserve der Stufe
ai-task-max-tight = Ausgabegrenze knapp – Antworten können abbrechen (Error: Output limit reached)
ai-task-fallback-list = Ersatzliste für { $task }
ai-task-fallback-add = Ersatzmodell hinzufügen
ai-task-fallback-up = { $model } nach oben
ai-task-fallback-down = { $model } nach unten
ai-task-fallback-remove = { $model } entfernen
ai-task-reset = Auf Standard zurücksetzen

ai-level-model-default = Modellstandard
ai-level-off = aus
ai-level-minimal = minimal (Stufe)
ai-level-low = niedrig
ai-level-medium = mittel
ai-level-high = hoch
ai-level-xhigh = sehr hoch
ai-level-max = höchste

## Richtlinie (AI-019) und Crawl-Budget (AI-013)
ai-policy-title = Richtlinie für KI-Anbieter
ai-policy-label = Wohin dürfen Daten gehen?
ai-policy-unrestricted = Keine Einschränkung
ai-policy-eu-only = EU-Verarbeitung erzwingen
ai-policy-local-only = Nur lokal
ai-policy-unrestricted-hint = Alle verbundenen Anbieter sind zulässig.
ai-policy-eu-only-hint = Nur Endpunkte mit Verarbeitung in der EU; sonst „Skipped (policy)“. Kein stiller Rückfall auf den globalen Endpunkt.
ai-policy-local-only-hint = Nur lokale Modelle (Ollama); kein KI-Request verlässt den Rechner.
ai-budget-title = Crawl-Budget bezahlter APIs
ai-budget-crawl = Budget je Crawl (USD)
ai-budget-crawl-hint = Gilt für alle bezahlten Aufrufe dieses Crawls; 0 heißt keine kostenpflichtigen Aufrufe.
ai-budget-unlimited = Unbegrenzt
ai-budget-unlimited-warning = Ohne Budget gibt es keine Obergrenze für die Kosten dieses Crawls.
ai-budget-ai-total = Teilbudget „KI gesamt“ (USD, leer = keines)
ai-budget-ai-total-hint = Begrenzt nur die KI-Aufgaben innerhalb des Crawl-Budgets.
ai-budget-warning = Warnschwelle (% des Crawl-Budgets)
ai-budget-warning-invalid = Bitte eine ganze Zahl von 0 bis 100 eingeben.

## Option „KI-Eingaben speichern“ (AI-032, M4-D05)
ai-store-inputs-title = KI-Eingaben
ai-store-inputs-label = KI-Eingaben speichern
ai-store-inputs-hint = Speichert während des Crawls je HTML-Seite mit Status 200 den Text des Inhaltsbereichs, alle H2-Überschriften, die Bildkontexte, die Sprachangabe und die Typen strukturierter Daten in der Crawl-Datei. Ein späterer KI-Lauf sendet dann denselben Text, ohne Seiten neu abzurufen.
