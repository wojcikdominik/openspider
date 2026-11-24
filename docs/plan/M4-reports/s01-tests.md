# M4-S01 – Unabhängige Abnahmetests

Zweig `ai/s01-tests` ab Bauer-Stand `ee20f467`; Wiederholung nach Nacharbeit gegen `e0fb5042` (Merge `fa162c64`). Erwartungen nur aus `docs/spec/08-ai.md` (AI-120, AI-121), `docs/spec/01-crawl.md` (CRAWL-098 … 100, CRAWL-114 [M4]) und den Festlegungen des Orchestrators (`ua_pattern` = Teilzeichenkette ohne Groß-/Kleinschreibung und Pflichtfeld; Grundmenge `Blocked %` = gecrawlte interne HTML-URLs des Hosts; `Host` = Schema+Host+Port; 5xx → `robots.txt Error`; Kontroll-Tokens wie Crawler). Bauer-Bericht, Bauer-Tests und Commit-Texte wurden nicht gelesen; vom Produktcode nur Signaturen und Doc-Kommentare. Jeder Test trägt `// spec: <ID>` und `// acceptance: independent`.

## Dateien

- `crates/os-analyze/tests/acceptance_s01_catalog.rs` – Katalog AI-120 (Inhalt, Schema-Regeln, Negativfälle, Benutzereinträge), Voreinstellungs-Tokens CRAWL-114 [M4] auf Bibliotheksebene, Standardauswahl AI-121
- `crates/os-analyze/tests/acceptance_s01_robots.rs` – Auswertung AI-121 über `evaluate` und `access_report` (Crawl-Datei mit `urls`/`robots_files`); eigener kleiner Parser nach CRAWL-098 als Testdouble für `RobotsRules`
- `crates/os-fetch/tests/acceptance_s01_robots_ai_tokens.rs` – Gruppenauswahl/Vorrang aus CRAWL-098 mit KI-Tokens im echten Parser `os_fetch::robots`
- `crates/os-fetch/tests/acceptance_s01_presets.rs` – Voreinstellungen aus dem Katalog, Katalogerweiterung, Benutzerkatalog, Ende-zu-Ende OAI-SearchBot (MiniServer: gesendeter User-Agent, RobotsGate mit Gruppe `User-agent: OAI-SearchBot`)
- Fixtures `tests/fixtures/ai/s01-acceptance/`: `catalog-minimum.json` (eigene Abschrift der Tabelle AI-120), `valid-base.json`, `valid-edge.json`, `invalid/*.json` (21 Negativfälle), Benutzerkataloge, Erweiterungseinträge, `robots/*.txt` (18 robots.txt-Varianten, u. a. die sechs Hosts aus der AI-121-Testfixture)

## Ergebnis

### Wiederholung gegen `e0fb5042` (Lauf `run3`)

Nach den Festlegungen des Orchestrators (s. u.) umgearbeitet und um Grenzfälle ergänzt. Prüfzeile im Log: `PRUEF Bauer e0fb5042 Zweig fa162c64`. fmt grün, clippy grün. **73 Tests: 73 grün, 0 rot, 0 ignoriert** (catalog 28, robots 24, presets 10, robots_ai_tokens 11).

Änderungen gegenüber dem ersten Lauf:

- `ua_pattern` (Festlegung 1): statt „nicht leer in jedem Eintrag“ jetzt `every_entry_has_ua_pattern_key_with_null_or_non_empty_string` (Rohdatei: Schlüssel überall, Wert `null` oder nicht leer), `entries_with_ua_example_have_a_ua_pattern` (dokumentierter User-Agent ⇒ Muster), `control_token_ua_pattern_is_null_or_non_empty`, `empty_ua_pattern_is_rejected` (`""` abgelehnt), `missing_ua_pattern_is_rejected` (unverändert), `null_ua_pattern_is_accepted_without_documented_user_agent` (Fixture `valid-ua-pattern-null.json`; `invalid/ua-pattern-null.json` entfernt). Alle grün.
- Schema am Spec-Pfad (Festlegung 2): `json_schema_file_exists_at_the_spec_path` unverändert, jetzt grün.
- Voreinstellungen ohne `ua_example` (Festlegung 3): `presets_without_ua_example_resolve_per_decision` (alle betroffenen Tokens: neue → OpenSpider-Standard-User-Agent, feste → User-Agent der Voreinstellungsdatei, Token = Katalog-Token) und Ende-zu-Ende `presets_without_ua_example_end_to_end` (MiniServer, Claude-SearchBot neu und ClaudeBot fest: gesendeter User-Agent auch beim robots.txt-Abruf, eigene Gruppe greift, `*` nicht). Grün.
- `Access` (Festlegung 4): `homepage_blocked_but_other_urls_allowed_is_partially_blocked`, `homepage_allowed_but_all_crawled_urls_blocked_is_partially_blocked`, `homepage_and_all_crawled_urls_blocked_is_blocked_without_homepage_row`, `without_html_urls_the_homepage_decides`, `robots_error_counts_follow_the_policy_for_every_token` (beide Policies, mit und ohne HTML-URL, auch Kategorien je URL), `robots_error_policy_from_crawl_config`. Grün.
- CRLF (Festlegung 6): Zeile `tests/fixtures/ai/s01-acceptance/robots/ai-bom-crlf.txt -text` in `.gitattributes`, Datei mit CRLF neu eingecheckt (Index geprüft); `bom_crlf_comments_and_spaces_with_ai_token` prüft jetzt zusätzlich, dass die gelesene Datei BOM und ausschließlich CRLF enthält. Grün.

### Erster Lauf gegen `ee20f467` (Lauf `run2`, historisch)

Lauf auf dem Mini (`run2`): fmt grün, clippy (`-p os-analyze -p os-fetch --all-targets -D warnings`) grün. **63 Tests: 59 grün, 4 rot, 0 ignoriert.**

| Datei | grün | rot |
|---|---|---|
| acceptance_s01_catalog | 22 | 4 |
| acceptance_s01_robots | 18 | 0 |
| acceptance_s01_presets | 8 | 0 |
| acceptance_s01_robots_ai_tokens | 11 | 0 |

### AI-120 AI-Crawler-Katalog

| Kriterium | Test | Ergebnis |
|---|---|---|
| Datei gültig, eingebettete Fassung = `resources/ai/ai-crawlers.json` | `shipped_catalog_is_valid_and_embedded`, `embedded_catalog_is_the_resource_file` | grün |
| alle 27 Tokens mit Betreiber, Kategorie, deprecated/replaced_by, docs_url, docs_note (anthropic-ai, Bytespider, cohere-ai, Diffbot) | `shipped_catalog_contains_all_27_table_tokens_with_their_attributes` | grün |
| respects_robots: Google-CloudVertexBot = yes; GeminiNotebook/NotebookLM/Agent = no | `google_respects_robots_values_follow_the_decision` | grün |
| Gemini-Deep-Research nicht aufgenommen | `gemini_deep_research_is_not_in_the_catalog` | grün |
| Rohdatei erfüllt die Schema-Regeln (eigene Prüfung am JSON) | `shipped_catalog_raw_json_satisfies_the_schema_rules` | grün |
| `ua_pattern` Pflicht (Festlegung) – crawlende Einträge | `every_crawling_entry_has_a_non_empty_ua_pattern` | **rot** (Befund 1) |
| `ua_pattern` Pflicht (Festlegung) – Kontroll-Tokens | `control_tokens_have_a_non_empty_ua_pattern` | **rot** (Befund 1, s. Frage 1) |
| `ua_pattern` erkennt eigenen `ua_example` case-insensitiv, Teilzeichenkette wörtlich | `ua_pattern_matches_own_example_case_insensitively`, `ua_pattern_is_a_plain_substring` | grün |
| JSON-Schema unter `resources/ai/schemas/ai-crawlers.schema.json` | `json_schema_file_exists_at_the_spec_path` | **rot** (Befund 2) |
| gültige Grenzfälle (docs_note neben URL, replaced_by gültig, null+note, alle Werte) | `valid_edge_cases_are_accepted` | grün |
| doppelte Tokens case-insensitiv | `duplicate_tokens_are_rejected_case_insensitively` | grün |
| ungültige/fehlende Kategorie | `invalid_or_missing_category_is_rejected` | grün |
| docs_url nicht https (http, ftp, leer) | `docs_url_without_https_is_rejected` | grün |
| docs_url null ohne/mit leerem docs_note | `null_docs_url_needs_a_non_empty_docs_note` | grün |
| replaced_by auf unbekanntes Token | `replaced_by_must_name_an_existing_token` | grün |
| verified_at kein Datum (13.10.2026, 2026-02-30, Text, fehlt) | `verified_at_must_be_a_date` | grün |
| Pflichtfelder token/operator/respects_robots, ungültiger respects_robots | `required_fields_and_respects_robots_values_are_enforced` | grün |
| ua_pattern fehlt → abgelehnt | `missing_ua_pattern_is_rejected` | grün |
| ua_pattern null bzw. leer → abgelehnt (Festlegung) | `null_or_empty_ua_pattern_is_rejected` | **rot** (Befund 1) |
| Benutzereintrag mit vorhandenem Token (auch andere Schreibweise, auch als Datei) abgelehnt, nichts überschrieben | `user_entry_with_existing_token_is_rejected`, `new_user_entry_is_added_once` | grün |
| Konsistenz mit CRAWL-114 (Gleichheit) | siehe CRAWL-114 | grün |

### AI-121 robots.txt-Auswertung je AI-Crawler

| Kriterium | Test | Ergebnis |
|---|---|---|
| GPTBot `Blocked` (Matched Group GPTBot, Matched Rule `Disallow: /`), OAI-SearchBot `Allowed` (Matched Group `*`) | `gptbot_blocked_and_oai_searchbot_allowed` | grün |
| ClaudeBot `/intern/` 10 von 40 → `Partially Blocked`, `25.00` | `claudebot_partial_ten_of_forty` | grün |
| dasselbe aus einer Crawl-Datei; Bilder unter /intern/ und externe URLs zählen nicht; 404 → `No robots.txt`, 503 → `robots.txt Error`; Kategorien je URL | `access_report_from_crawl_file_uses_internal_html_urls` | grün |
| 404 → alle Tokens `No robots.txt` (= erlaubt) | `host_without_robots_txt_is_no_robots_txt_for_all_tokens` | grün |
| 5xx → `robots.txt Error` bei beiden Policies; Behandlung nach CRAWL-100 | `unreachable_robots_txt_is_robots_txt_error_under_both_policies`, `unreachable_robots_txt_follows_the_unreachable_policy` | grün |
| Kontroll-Tokens wie Crawler, Spalte `Note` | `control_tokens_are_evaluated_like_crawlers_with_a_note` | grün |
| Report-Spalten | `report_has_the_spec_columns` | grün |
| Hosts der Testfixture: block-all, mixed, search-blocked; nur User-Fetch gesperrt; Internal-Spalten je Kategorie | `block_all_host_blocks_every_token`, `mixed_host_blocks_training_only`, `search_blocked_host_blocks_search_only`, `user_fetch_blocked_host_and_matched_group_none` | grün |
| Grenzfälle: leere robots.txt (`none`), nur `*`, Groß-/Kleinschreibung, mehrere Gruppen je Token | `empty_robots_txt_allows_everything`, `star_only_robots_txt_applies_to_every_token`, `token_case_in_robots_txt_does_not_matter`, `multiple_groups_for_the_same_token_are_merged` | grün |
| Standardauswahl ohne veraltete Tokens; explizite Auswahl (Schreibweise egal, auch veraltete) | `token_selection_default_and_explicit`, `default_selection_is_all_non_deprecated_entries` | grün |
| Host = Schema+Host+Port | `host_means_scheme_host_and_port` | grün |
| Gruppenauswahl und Vorrang aus CRAWL-098 mit KI-Tokens (echter Parser): crawl-robots-Fälle mit OAI-SearchBot, Gleichstand → Allow, Zusammenführung, Gruppengrenzen mit `Sitemap:`, Groß-/Kleinschreibung, ähnliche Tokens (Applebot/Applebot-Extended, ClaudeBot/Claude-SearchBot, Google-Extended/Google-Agent), leeres `Disallow:`, BOM/CRLF/Kommentar, ohne `*`, leere Datei, Zeilennummer+Regeltext | 11 Tests in `acceptance_s01_robots_ai_tokens.rs` | grün |

### CRAWL-114 [M4]

| Kriterium | Test | Ergebnis |
|---|---|---|
| Menge der KI-Voreinstellungen = {nicht veraltete search/user-fetch} ∪ {GPTBot, ClaudeBot, Google-Extended} (Gleichheit, Schreibweise identisch, jedes Token genau einmal) | `ai_preset_set_equals_catalog_selection`, `ai_preset_tokens_equal_the_spec_set` | grün |
| veraltete Einträge (anthropic-ai, Google-NotebookLM) fehlen, Training und Applebot-Extended auch | `deprecated_and_training_entries_are_not_presets` | grün |
| HTTP-User-Agent = `ua_example`, Token = `token`; feste Einträge übernehmen Katalogwerte | `ai_presets_use_ua_example_and_token` | grün (nur Einträge mit `ua_example`, s. Befund 3) |
| neue Voreinstellungen in Gruppe „KI-Crawler“ | `catalog_presets_are_grouped_under_ki_crawler` | grün |
| im Katalog ergänzter search-Eintrag erscheint ohne Code-Änderung, veralteter nicht | `added_search_entry_appears_without_code_change`, `added_search_entry_appears_as_preset_token` | grün |
| Benutzereinträge search/user-fetch erscheinen, training nicht; Benutzereintrag GPTBot überschreibt nichts | `user_catalog_entries_feed_presets_without_overriding`, `user_catalog_search_entries_feed_presets` | grün |
| Voreinstellung OAI-SearchBot sendet `ua_example` (auch beim robots.txt-Abruf) und wertet `User-agent: OAI-SearchBot` aus | `oai_searchbot_preset_sends_ua_example_and_uses_its_robots_group` | grün |

## Befunde

Stand nach Wiederholung: Befunde 1 und 2 erledigt (Festlegung 1 bzw. Umzug des Bauers), 3 und 4 durch Festlegungen 3 und 4 geklärt und getestet, 8 behoben; 5 bleibt (Sache der 1.2-Etappe), 6 erledigt (Umzug), 7 akzeptiert (kommt in die Spec). Ursprünglicher Text:


1. **B1 – `ua_pattern` ist nicht Pflicht.** Festlegung des Orchestrators: „`ua_pattern` ist … Pflichtfeld“. Beobachtet: 11 von 27 Einträgen haben `"ua_pattern": null` (ClaudeBot, Claude-SearchBot, Claude-User, anthropic-ai, Google-NotebookLM, Bytespider, cohere-ai, YouBot, Diffbot sowie die Kontroll-Tokens Google-Extended, Applebot-Extended), und `Catalog::parse` nimmt `null` und `""` an (nur ein fehlender Schlüssel wird abgelehnt; Doc-Kommentar `REQUIRED_FIELDS`: „`ua_pattern` … dürfen `null` sein“). Erwartet: nicht leere Zeichenkette in jedem Eintrag, Ablehnung von `null`/`""`. Rote Tests: `every_crawling_entry_has_a_non_empty_ua_pattern`, `control_tokens_have_a_non_empty_ua_pattern`, `null_or_empty_ua_pattern_is_rejected`. Siehe Frage 1.
2. **B1 – JSON-Schema fehlt am Spec-Pfad.** AI-120 „Testfixture: … Schema `resources/ai/schemas/ai-crawlers.schema.json`“. Beobachtet: Datei fehlt; ein Schema liegt nur unter `tests/fixtures/ai/search/ai-crawlers.schema.json`. Vermutlich Teil des angekündigten Umzugs; Test bleibt rot, bis die Datei am Spec-Pfad liegt. Roter Test: `json_schema_file_exists_at_the_spec_path`.
3. **B2 – Voreinstellungen ohne `ua_example`.** CRAWL-114 [M4]: „HTTP-User-Agent = `ua_example`“. Für ClaudeBot, Claude-SearchBot, Claude-User, Google-CloudVertexBot (und Google-Extended) ist `ua_example` `null`. Beobachtet: feste Voreinstellung ClaudeBot behält ihren alten HTTP-User-Agent aus `user-agents.toml`; neue Voreinstellungen Claude-SearchBot, Claude-User, Google-CloudVertexBot sind „nur Token“ und senden damit laut Doc-Kommentar von `resolve` den OpenSpider-Standard-User-Agent. Die Spec regelt diesen Fall nicht (Frage 2).
4. **B2 – Abgrenzung `Blocked` / `Partially Blocked` nicht spezifiziert.** Offen sind z. B. Startseite gesperrt, aber andere URLs erlaubt; Host ohne gecrawlte HTML-URL; Werte von `Homepage Allowed`/`Blocked URLs` bei `robots.txt Error`. Getestet wurden nur die eindeutigen Fälle (alles gesperrt, Teilsperre mit erlaubter Startseite, nichts gesperrt).
5. **B3 – Kein Produktadapter Parser → `RobotsRules`.** `os_fetch::robots::RobotsTxt` legt die Gruppenauswahl nicht öffentlich offen (nur `evaluate`), und in dieser Etappe gibt es keinen Adapter vom Parser des Crawl-Kerns auf das Trait (`ai_search::run` ist Stub für S02). „Gleicher Parser“ ist daher zweigeteilt geprüft: Regeln im echten Parser (os-fetch), Auswertung mit Testdouble (os-analyze). Die Verbindung muss S02 liefern und prüfen.
6. **B3 – Spec-Fixture-Pfade fehlen noch.** `tests/fixtures/ai/catalog-minimum.json`, `tests/fixtures/ai/catalog-invalid/`, `tests/fixtures/ai/robots/` existieren nicht (Bauer-Fixtures unter `tests/fixtures/ai/search/`, Umzug angekündigt). Diese Tests nutzen eigene Fixtures.
7. **B3 – Zusätzliche Regeln der Bibliothek ohne Spec-Grundlage:** unbekannte Felder werden abgelehnt (`UnknownField`), `replaced_by` nur bei `deprecated = true`, Token-Zeichensatz `A-Z a-z 0-9 - _`. Nicht falsch, aber unbekannte Felder abzulehnen erschwert künftige Formaterweiterungen über Benutzerkataloge.

8. **B3 – CRLF der Fixture `robots/ai-bom-crlf.txt` geht im Repo verloren:** Die Zeilenende-Normalisierung von git speichert die Datei mit LF; der Test prüft nach dem Auschecken nur noch BOM, Kommentar und Leerzeichen vor dem Doppelpunkt. CRLF selbst deckt die bestehende `crawl-robots`-Variante `bom.site.test` ab. Ein `-text`-Eintrag in `.gitattributes` wäre eine fremde Datei und wurde nicht angelegt.

## Offene Fragen an die Spec

Alle drei durch die Festlegungen 1, 3 und 4 des Orchestrators beantwortet. Ursprünglicher Text:


1. Gilt die Pflicht für `ua_pattern` auch für Kontroll-Tokens (crawlen nicht, kein User-Agent) und für Betreiber ohne dokumentierten User-Agent (z. B. Anthropic-Seite nennt keinen)? Falls nicht: Festlegung auf „Pflicht, außer Kategorie `control-token`“ oder „Schlüssel Pflicht, Wert darf `null` sein“.
2. Was sendet eine KI-Voreinstellung, deren Katalogeintrag keinen `ua_example` hat: den bisherigen festen User-Agent, den OpenSpider-Standard oder soll die Voreinstellung entfallen bzw. eine Warnung zeigen?
3. Genaue Definition von `Blocked` gegenüber `Partially Blocked` (s. Befund 4).

## Nicht in dieser Etappe geprüft

- AI-120: Katalogansicht und Benutzerkatalog in der App (Oberfläche), nächtlicher Erreichbarkeitslauf der `docs_url` (CI), 180-Tage-Warnung im Release-Check (CI).
- AI-121: Report im Export, Internal-Spalten in der Tabelle, „keine zusätzlichen HTTP-Requests“ im Request-Log eines Crawls (die Auswertung ist in dieser Etappe nicht in den Nachlauf eingehängt; `evaluate`/`access_report` lesen nur Eingaben bzw. die Crawl-Datei), Ende-zu-Ende mit dem Parser des Crawl-Kerns (Befund 5).
- CRAWL-114 [M4]: Voreinstellungsliste in der App (`presets.ts`), Abruf als KI-Crawler (AI-125).
