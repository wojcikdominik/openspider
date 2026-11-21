# Unabhängige Abnahmetests: Update-Kanal-Einstellung (K3-16, Fix-Release 1.0.1)

Testagent nach `docs/plan/unabhaengige-tests.md`. Grundlage allein `docs/spec/06-output-ops.md` (OUT-254, OUT-250, OUT-252, OUT-222) und `docs/spec/05-ui.md` (UI-131); Bauer-Bericht, Bauer-Tests und Commit-Texte nicht gelesen. Code nur für Signaturen/Aufruf gelesen.

- Branch: `fix/kanal-app-tests` ab `fix/kanal-app` (`56d66cf6`)
- Ergebnis: **alle Abnahmetests grün** (Rust 22/22, Vitest 14/14, Playwright 22/22 in Chromium und WebKit). Keine Produktbefunde.

## Dateien

| Datei | Art |
|---|---|
| `crates/os-app/tests/acceptance_update_channel.rs` | Rust: Desktop-Kern und CLI-Kern gegen `ChannelFixture` |
| `tests/fixtures/update/acceptance-channel/{missing,stable,beta,nightly}.toml` | Profile ohne Schlüssel, mit `stable`, `beta`, ungültigem Wert |
| `apps/desktop/src/features/settings/sections/updates/channel.acceptance.test.tsx` | Vitest mit gemocktem IPC |
| `apps/desktop/e2e/settings-sections/update-channel.acceptance.spec.ts` | Playwright gegen `settings-sections/harness.tsx` |

## Spec-Kriterium → Test → Ergebnis

| Spec | Kriterium | Test | Ergebnis |
|---|---|---|---|
| OUT-254 | Testmatrix Zeilen 1–13, Desktop-Kern (manuelle Prüfung): angefragter Pfad (genau 1 Request, anderer 0), Ergebnis, gespeicherter Wert, `beta` vor dem Request gespeichert (Z. 5/6), kein Download | `testmatrix_desktop_core_manual_check_rows_1_to_13` | grün |
| OUT-254 / OUT-250 | Testmatrix Zeilen 1–13, CLI (`check_with_context` mit Kontext aus `context_from_settings`): Exit 0/10/Fehler, Pfad, `settings.toml` bytegleich, nichts angelegt | `testmatrix_cli_check_update_rows_1_to_13_never_writes_settings` | grün |
| OUT-254 | Desktop und CLI liefern je Zeile dieselbe Ergebnisart | `testmatrix_desktop_and_cli_agree_per_row` | grün |
| OUT-254 | Standardregel (Vorabversion inkl. rc/alpha/Hotfix-Pre → beta, Build-Metadaten ≠ Vorabversion) | `standard_rule_prerelease_is_beta_otherwise_stable` | grün |
| OUT-254 | gültiger Wert geht der Regel immer vor | `valid_stored_value_always_wins_over_the_rule` | grün |
| OUT-254 | alles außer `stable`/`beta` (auch Groß-/Kleinschreibung, Leerzeichen, leer) gilt als nicht gesetzt | `everything_except_stable_and_beta_counts_as_unset` | grün |
| OUT-254 / OUT-222 | Start speichert nur `beta`, nur bei Vorabversion ohne gültigen Wert; `stable` nie automatisch | `startup_stores_only_beta_and_only_for_a_prerelease_without_valid_value` | grün |
| OUT-254 | Persistenz: ohne Schlüssel, Start 1.1.0-beta.1 → `channel = "beta"` in `[updates]`; danach 1.1.0 → bleibt `beta`, Beta-Pfad angefragt | `beta_stored_on_prerelease_start_survives_update_to_stable` | grün |
| OUT-254 / OUT-222 | ohne Schlüssel, Start und Prüfung mit 1.0.1 → kein Schlüssel `channel` | `stable_version_without_key_writes_no_channel_key` | grün |
| OUT-254 | gespeichertes `stable` auf 1.1.0-beta.3 bleibt über 3 Neustarts, kein Downgrade | `stored_stable_on_prerelease_is_kept_across_restarts` | grün |
| OUT-254 Z. 13 | ungültiger Wert bleibt in der Datei, Warnung im Log (Desktop) | `invalid_value_stays_unchanged_in_the_file_and_is_logged` | grün |
| OUT-254 Z. 13 | Warnung im Log (CLI) | `cli_logs_a_warning_for_an_invalid_value` | grün |
| OUT-254 / UI-131 | Wechsel `stable` → `beta`: sofort gespeichert, 0 Requests, danach manuelle Prüfung genau 1 Request an Beta-Pfad | `switching_stable_to_beta_sends_no_request_then_manual_check_hits_beta_once` | grün |
| OUT-254 / OUT-252 | Wechsel `beta` → `stable` auf Vorabversion: kein Downgrade | `switching_beta_to_stable_on_prerelease_offers_no_downgrade` | grün |
| OUT-254 / OUT-251 | Kanalwechsel lässt `skipped_version` und `check` unverändert | `channel_switch_keeps_skipped_version_and_check_flag` | grün |
| OUT-222 | Kanal in `settings.toml`, nicht in Crawl-Konfiguration / `.osconfig` | `channel_lives_in_settings_toml_and_not_in_any_crawl_config` | grün |
| OUT-222 | `settings.toml` ohne Schlüssel (Stand 1.0.0) ohne Migration gelesen, nichts dazugeschrieben | `settings_without_channel_key_from_1_0_0_read_without_migration` | grün |
| UI-131 / OUT-254 | `update.channel`-Zustand: wirksamer Kanal, Version ohne Build-Nummer, `prerelease` | `channel_state_reports_effective_channel_and_version_without_build_number` | grün |
| OUT-254 / OUT-250 / OUT-253 | Abschaltung per Umgebung in beiden Kanälen, App und CLI: 0 Requests, Wert bleibt | `environment_switch_disables_checks_in_both_channels_for_app_and_cli` | grün |
| OUT-250 / OUT-254 | Anfrage nur `version`, `os`, `arch`; kein Kanal-Parameter (beide Kanäle, App und CLI) | `request_carries_only_version_os_arch_in_both_channels` | grün |
| OUT-254 | eingebaute URLs exakt wie Spec (App- und CLI-Konstanten, Standardkontexte) | `built_in_urls_match_the_spec_exactly_in_app_and_cli` | grün |
| OUT-254 | Kanal nur aus Einstellung und Version, App und CLI gleich | `channel_follows_settings_and_version_only_not_the_build` | grün |
| UI-131 | Vorauswahl ohne Schlüssel: 1.0.1 → Stabil, 1.1.0-beta.1 → Beta | Vitest „preselects the effective channel…“; E2E „preselection follows the standard rule…“ | grün |
| UI-131 | Vorauswahl gespeicherter Wert, auch unpassend zur Version; ungültig → Regel | Vitest „preselects a stored value…“ | grün |
| UI-131 | Beschriftung „Update channel“, Anker, direkt unter dem Schalter, Erklärung immer sichtbar unter der Gruppe | Vitest „labels the group and sits directly below…“ | grün |
| UI-131 | Wechsel auf Beta: sofort gespeichert, kein Dialog, kein Neustart-Hinweis, keine Update-Anfrage, Hinweis „From the next update check on …“ | Vitest „switching to Beta stores at once…“; E2E „switching to Beta … survives a restart“ (keine fremden Requests, keine `update.check`) | grün |
| UI-131 | Beta-Hinweis nur bis zum Verlassen der Seite; nach Neustart Beta weiter gewählt | Vitest „the next-check hint disappears…“; E2E (Reload) | grün |
| UI-131 | Rückweg auf Stabil mit 1.1.0-beta.3: Hinweis als `role=status`, nach Neustart sichtbar, Beta entfernt ihn; kein Knopf/Link zum Zurücksetzen | Vitest „back to Stable on 1.1.0-beta.3…“; E2E gleichnamig | grün |
| UI-131 | 1.0.1 mit Stabil → kein Hinweis | Vitest „shows no stay hint…“; E2E „Stable on 1.0.1 shows no stay hint“ | grün |
| UI-131 | Umgebungssperre: Gruppe ausgegraut wie Schalter, Wert bleibt | Vitest „environment lock…“; E2E gleichnamig | grün |
| UI-131 | „Automatisch nach Updates suchen“ aus → Gruppe bedienbar (Profil und Umschalten auf der Seite) | Vitest (2 Tests); E2E „automatic checks off keeps the group usable“ | grün |
| UI-131 | Speicherfehler → dieselbe `alert`-Meldung wie beim Schalter, gespeicherter Wert wieder gewählt | Vitest „save failure shows the same alert…“; E2E „save failure…“ | grün |
| UI-131 / UI-126 | Tab erreicht Gruppe, Pfeiltasten wechseln und speichern, sichtbarer Fokus, axe ohne critical/serious – hell und dunkel | E2E „keyboard: … (light)“ / „(dark)“; Vitest „arrow keys…“ | grün |
| UI-131 / UI-121 / UI-001 | de-Texte (Gruppe, Optionen, Erklärung, beide Hinweise) bei 1024×640 ohne Abschneiden/Überlauf, axe | Vitest „German texts“; E2E „German texts at 1024×640…“ | grün |
| UI-131 / UI-121 | en-Texte; fr/es/it fallen auf en zurück | Vitest „other interface languages fall back to English“; E2E „English texts“ | grün |

## Befunde

Keine. Kein spec-gerechter Test ist rot.

Zwei rote Zwischenstände waren Testfehler und wurden am Test behoben (nicht gelockert): Playwright klickte das visuell verborgene Eingabefeld der Option statt der sichtbaren Beschriftung; die Abschneideprüfung zählte die verborgenen Eingabefelder ohne Text mit.

## Spec-Lücken und Mehrdeutigkeiten

1. **`updates.check = false` und manuelle Prüfung/CLI.** UI-131 sagt bei ausgeschaltetem „Automatisch nach Updates suchen“: „der Kanal gilt für die manuelle Prüfung und die CLI“. Das setzt voraus, dass manuelle Prüfung und `--check-update` dann noch laufen. OUT-250 sagt nur „Abgeschaltet → kein Netzwerkzugriff“ und nennt `updates.check = false` neben Umgebungsvariable und Richtlinie, ohne „abgeschaltet“ vom „automatisch aus“ abzugrenzen. Nicht getestet. Zu entscheiden: Unterdrückt `updates.check = false` auch die manuelle Prüfung und die CLI?
2. **Sperre durch Richtlinie in der UI.** UI-131 sperrt die Gruppe bei „Umgebungsvariable oder Richtlinie“. Am IPC gibt es nur den Abschaltgrund `environment`. Ob eine Richtlinie (OUT-253) als `environment` gemeldet wird, steht in keiner Spec. Getestet wurde nur die Umgebungsvariable.
3. **OUT-252, abgelehntes Vorabversions-Manifest im Kanal `stable`.** „Beim Start nur Log-Eintrag“ hat kein beobachtbares Kriterium am Kern. Getestet: manuelle Prüfung meldet einen Fehler (Zustand `failed`), kein Download, CLI endet mit Fehler. Die automatische Startprüfung wurde nicht getestet.
4. **Text der Log-Warnung.** OUT-254 schreibt keinen Text vor. Der Test verlangt eine WARN-Zeile, die den Wert oder „Kanal/channel“ nennt.
5. **Konfigurations-Diff (OUT-224).** Das Kriterium „Diff zeigt für `updates.channel` keine Zeile“ wurde nicht getestet: Ich habe keine öffentliche Diff-Schnittstelle für `settings.toml` gefunden. Geprüft wurde nur, dass die Crawl-Konfiguration bzw. eine `.osconfig` den Schlüssel nicht trägt.
6. **CLI-Binary.** Die URLs sind laut Spec nicht konfigurierbar, deshalb lässt sich das Binary `openspider --check-update` nicht gegen den Mock richten. Getestet wurde der Kern des Aufrufs (`os_cli::update::context_from_settings` + `check_with_context`, Rückgabe 0/10/Fehler), nicht die Abbildung auf den Prozess-Exit-Code. Ebenfalls nicht getestet: die bei Exit 10 auf stdout genannte Version.
7. **„Netzwerkmitschnitt“ in der UI-E2E.** Der Harness läuft mit Mock-Transport. Belegt wurde „0 Requests beim Wechsel“ so: keine `update.check`/`download`/`install`-Aufrufe und keine Requests an fremde Ursprünge. Den echten Mock-Update-Server deckt die Rust-Seite ab.

## Ausgeführt / nicht ausgeführt

- Lokal (Haupt-Mac): `cargo fmt --all --check` grün; `npm run -w apps/desktop typecheck` grün; `vitest run …/channel.acceptance.test.tsx` 14/14 grün. `node_modules` wurde dafür nur als nicht eingecheckter Symlink auf den Bau-Worktree angelegt.
- Build-Mini (`/Volumes/osbuild/openspider/wt/kanal-app`, unter `heavy.lock`): `cargo test -p os-app --test acceptance_update_channel -- --test-threads=1` 22/22 grün; `cargo clippy -p os-app --tests -- -D warnings` grün; Playwright `update-channel.acceptance.spec.ts --workers=1` (Chromium und WebKit) 22/22 grün.
- Nicht ausgeführt: `cargo test --workspace`, `cargo clippy --workspace --all-targets`, die gesamte Vitest-/Playwright-Suite, die Tests des Bauers, Windows und Intel.
