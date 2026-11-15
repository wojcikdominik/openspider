## Integrationen: API-Zugriff, Verbindungsdialoge, API-Fortschritt, APIs-Modus (M2-U26)

api-ga4 = Google Analytics 4
api-gsc = Search Console
api-psi = PageSpeed Insights
api-majestic = Majestic
api-ahrefs = Ahrefs
api-moz = Moz
api-drive = Google Drive

integ-short-ga4 = GA4
integ-short-gsc = GSC
integ-short-psi = PSI
integ-short-majestic = Majestic
integ-short-ahrefs = Ahrefs
integ-short-moz = Moz
integ-short-drive = Drive

integ-title = API-Zugriff
integ-description = Datenquellen verbinden und festlegen, was sie liefern. Schlüssel und Tokens bleiben im Schlüsselbund des Systems.
integ-list = Integrationen
integ-source-sections = Einstellungen für { $source }
integ-tab-account = Konto
integ-tab-metrics = Metriken
integ-tab-advanced = Erweitert
integ-open = API-Zugriff…
integ-gear = { $source } einrichten

integ-status-disconnected = Nicht verbunden
integ-status-connecting = Verbinde…
integ-status-connected = Verbunden
integ-status-connected-as = Verbunden ({ $account })
integ-status-error = Fehler: { $message }
integ-error-unknown = unbekannte Ursache

integ-connect = Verbinden
integ-connect-saved = Gespeicherten Schlüssel verwenden
integ-connect-account = Als { $account } verbinden
integ-connect-google = Neues Google-Konto verbinden
integ-connect-oauth = Mit { $source } anmelden
integ-connecting = Verbindung wird geprüft…
integ-connecting-browser = Bitte die Anmeldung im Browser abschließen.
integ-disconnect = Trennen
integ-remove-account = Konto entfernen
integ-api-key = API-Schlüssel
integ-remember = Schlüssel merken
integ-saved-keys = Gespeicherte Schlüssel
integ-google-accounts = Gemerkte Google-Konten
integ-client-secret = Client-Geheimnis (eigener Google-Cloud-Client)
integ-client-secret-hint = Nur für einen eigenen OAuth-Client nötig. Es wird an die Anmeldung übergeben und nie in ein Profil geschrieben.
integ-enabled = Daten dieser Quelle beim Crawlen abrufen

integ-catalog-loading = Datenquellen werden geladen…
integ-catalog-unavailable = Die Datenquellenliste lässt sich hier noch nicht laden. Bitte die Kennung von Hand eintragen.
integ-catalog-error = Die Datenquellenliste konnte nicht geladen werden. Kennung von Hand eintragen oder später erneut versuchen.
integ-ga4-account = Konto
integ-ga4-property = Datenquelle
integ-ga4-property-id = GA4-Datenquellen-ID
integ-ga4-property-search = Datenquellen durchsuchen
integ-ga4-stream = Datenstream
integ-ga4-all-streams = Alle Datenstreams
integ-ga4-no-properties = Keine GA4-Datenquellen gefunden ({ $account }).
integ-gsc-property = Datenquelle
integ-gsc-property-id = Search-Console-Datenquelle (sc-domain:… oder URL-Präfix)
integ-gsc-kind-domain = Domainweit
integ-gsc-kind-prefix = URL-Präfix
integ-gsc-unverified = nicht bestätigt
integ-gsc-no-properties = Dieses Google-Konto hat keine Search-Console-Datenquellen.
integ-gsc-mismatch = Datenquelle passt nicht zur Crawl-Domain.

integ-psi-source = Quelle
integ-psi-remote = Remote (PageSpeed-Insights-API)
integ-psi-local = Lokal (Lighthouse auf diesem Rechner)
integ-psi-remote-hint = Lighthouse läuft auf Google-Servern und liefert zusätzlich CrUX-Felddaten. Benötigt einen kostenlosen API-Schlüssel mit freigeschalteter PageSpeed-Insights-API.
integ-psi-local-hint = Lighthouse läuft auf diesem Rechner, ohne Schlüssel und ohne Tageslimit; CrUX-Felddaten gibt es dann nicht.
integ-psi-lighthouse-path = Pfad zu Lighthouse
integ-psi-lighthouse-path-hint = Vollständiger Pfad zur installierten Lighthouse-Kommandozeile, z. B. die Ausgabe von „which lighthouse“ (macOS/Linux) oder die Datei „lighthouse\cli\index.js“ im globalen npm-Ordner (Windows).
integ-psi-denied = PSI-Zugriff abgelehnt ({ $reason })
integ-psi-checklist = Bitte prüfen:
integ-psi-check-enabled = Ist im Google-Cloud-Projekt des Schlüssels die PageSpeed-Insights-API freigeschaltet?
integ-psi-check-new = Wurde der Schlüssel eben erst angelegt? Dann einige Minuten warten und erneut verbinden.
integ-psi-check-restricted = Ist der Schlüssel auf bestimmte APIs oder Referrer beschränkt?

integ-levels = Ebenen
integ-level-not-offered = Bietet dieser Anbieter nicht an
integ-level-not-available = Noch nicht verfügbar
integ-metrics = Metriken
integ-columns = Entstehende Spalten
integ-columns-count = { $count ->
    [one] 1 Spalte
   *[other] { $count } Spalten
}

integ-concurrency = Gleichzeitige Anfragen
integ-concurrency-hint = Standard 5
integ-retries = Wiederholungen
integ-range = Erlaubt: { $min } bis { $max }
integ-client-id = Eigene OAuth-Client-ID
integ-client-id-hint = Client-ID Ihres Google-Cloud-Projekts (Desktop-App).
integ-client-id-note = Solange keine gemeinsame Client-ID ausgeliefert wird, braucht die Google-Anmeldung eine eigene Client-ID.

integ-invalidate-question = Es wurden Einstellungen geändert, die die Daten von { $sources } beeinflussen. Vorhandene Daten löschen und neu laden oder behalten?
integ-invalidate-reload = Löschen und neu laden
integ-invalidate-keep = Behalten

integ-progress-name = API-Fortschritt
integ-progress-label = API: { $percent } %
integ-progress-finished = fertig
integ-progress-part = { $name } { $amount } ({ $errors } Fehler)

integ-apimode = APIs-Modus
integ-apimode-urls = URLs (eine je Zeile)
integ-apimode-file = Datei laden…
integ-apimode-start = API-Daten abrufen
integ-apimode-count = { $count ->
    [one] 1 URL
   *[other] { $count } URLs
}, { $duplicates } Doppelungen entfernt
integ-apimode-preview = { $valid } gültig, { $invalid } ungültig, { $duplicates } doppelt
integ-apimode-sources = Daten von: { $sources }
integ-apimode-none-connected = Keine Integration ist verbunden.
integ-apimode-no-integration = Zuerst mindestens eine Integration verbinden. Es wurde nichts gestartet.
integ-apimode-empty = Mindestens eine URL eintragen.
integ-apimode-started = API-Daten für { $count } URLs werden abgerufen; die URLs selbst werden nicht angefragt.

integ-menu-api-access = API-Zugriff
integ-menu-overview = Alle Integrationen…
integ-menu-mode-apis = APIs

err-api-connect-timeout = Zeitüberschreitung nach { $seconds } s
err-api-test-timeout = Der Verbindungstest hat das Zeitlimit überschritten.
err-api-reconnect = Die Anmeldung wurde abgelehnt oder ist abgelaufen. Bitte neu verbinden.
err-api-disconnected = Die Integration ist nicht verbunden.
err-api-source-unavailable = Diese Datenquelle ist noch nicht verfügbar.
err-api-request = Die Anfrage an den Anbieter ist fehlgeschlagen.
err-api-key = Bitte einen API-Schlüssel eingeben.
err-api-lighthouse-missing = Lighthouse-Laufzeit nicht gefunden
err-api-client-id = Bitte zuerst unter „Erweitert“ eine eigene OAuth-Client-ID eintragen.
err-api-connecting = Es läuft bereits ein Verbindungsversuch.
err-api-account = Das gewählte Konto gibt es nicht mehr.
err-api-kind = Die Einstellungen gehören zu einer anderen Integration.
err-api-settings = Die Einstellungen sind ungültig.
err-api-secret-in-settings = Schlüssel und Tokens lassen sich nicht in Einstellungen speichern.
notice-api-session-only = Der Schlüsselbund ist nicht verfügbar; der Schlüssel gilt nur für diese Sitzung.
notice-api-revoke-failed = Das Konto wurde lokal entfernt, der Anbieter konnte den Zugriff aber nicht widerrufen.
op-api-connect = Integration wird verbunden
op-api-fetch = API-Daten werden abgerufen
