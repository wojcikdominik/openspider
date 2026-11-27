-- AI-013/AI-014: money is integer micro-USD; no credentials in crawl files.
CREATE TABLE ai_runs (
 id INTEGER PRIMARY KEY, started_at TEXT NOT NULL, ended_at TEXT,
 budget_at_start_microusd INTEGER CHECK(budget_at_start_microusd >= 0),
 spent_microusd INTEGER NOT NULL DEFAULT 0 CHECK(spent_microusd >= 0),
 config_json TEXT NOT NULL CHECK(json_valid(config_json)),
 -- AI-020 (Nachtrag 27.11.2026, K4-27): Zeitpunkt der erteilten Kostenbestätigung (AI-012); leer = nicht bestätigt bzw.
 -- keine Bestätigung nötig. Fortsetzen setzt dieselbe Zeile fort und übernimmt sie.
 confirmed_at TEXT
);
-- (area, task): ('','') crawl; ('ai','') area; ('ai',task) task.
-- Limits only. Actual consumption is derived from per-area logs, not duplicated here.
CREATE TABLE api_budget (
 area TEXT NOT NULL, task TEXT NOT NULL DEFAULT '',
 limit_microusd INTEGER CHECK(limit_microusd >= 0),
 warning_percent INTEGER NOT NULL DEFAULT 80 CHECK(warning_percent BETWEEN 0 AND 100),
 -- AI-013: in der Bereichszeile (area, '') die quittierte Kennung der letzten Überschreitung einer Reservierung;
 -- spätere Überschreitungen im Protokoll sperren den Ledger auch nach Neuöffnen (bis eine Grenze angehoben wird).
 over_reservation_ack INTEGER CHECK(over_reservation_ack >= 0),
 PRIMARY KEY(area, task)
) WITHOUT ROWID;
CREATE TABLE ai_batches (
 id INTEGER PRIMARY KEY, run_id INTEGER NOT NULL REFERENCES ai_runs(id),
 provider TEXT NOT NULL, endpoint TEXT NOT NULL, provider_batch_id TEXT NOT NULL,
 status TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT,
 input_file_id TEXT, output_file_id TEXT, error_file_id TEXT,
 request_manifest_json TEXT NOT NULL CHECK(json_valid(request_manifest_json)),
 UNIQUE(provider, provider_batch_id)
);
CREATE TABLE ai_requests (
 id INTEGER PRIMARY KEY, run_id INTEGER NOT NULL REFERENCES ai_runs(id),
 timestamp TEXT NOT NULL, task TEXT NOT NULL, provider TEXT NOT NULL,
 model TEXT NOT NULL, endpoint TEXT NOT NULL, url_id INTEGER REFERENCES urls(id) ON DELETE SET NULL,
 definition_version TEXT,
 reservation_microusd INTEGER NOT NULL DEFAULT 0 CHECK(reservation_microusd >= 0),
 redactions INTEGER NOT NULL DEFAULT 0 CHECK(redactions >= 0),
 input_tokens INTEGER CHECK(input_tokens >= 0),
 output_tokens INTEGER CHECK(output_tokens >= 0), cached_input_tokens INTEGER CHECK(cached_input_tokens >= 0),
 cache_write_tokens INTEGER CHECK(cache_write_tokens >= 0), images INTEGER NOT NULL DEFAULT 0 CHECK(images >= 0),
 token_cost_microusd INTEGER NOT NULL CHECK(token_cost_microusd >= 0),
 fixed_fees_microusd INTEGER NOT NULL DEFAULT 0 CHECK(fixed_fees_microusd >= 0),
 regional_uplift_microusd INTEGER NOT NULL DEFAULT 0 CHECK(regional_uplift_microusd >= 0),
 cost_microusd INTEGER NOT NULL CHECK(cost_microusd >= 0),
 price_source TEXT NOT NULL CHECK(price_source IN ('api','table','user','estimate','reported')),
 batch_id INTEGER REFERENCES ai_batches(id), cache_hit INTEGER NOT NULL DEFAULT 0 CHECK(cache_hit IN (0,1)),
 status TEXT NOT NULL, http_status INTEGER, error_code TEXT, error_message TEXT,
 refusal_reason TEXT CHECK(refusal_reason IS NULL OR length(refusal_reason) <= 500),
 duration_ms INTEGER NOT NULL CHECK(duration_ms >= 0), provider_request_id TEXT,
 retries INTEGER NOT NULL DEFAULT 0 CHECK(retries >= 0),
 over_reservation INTEGER NOT NULL DEFAULT 0 CHECK(over_reservation IN (0,1)),
 confidence_mismatch INTEGER NOT NULL DEFAULT 0 CHECK(confidence_mismatch IN (0,1)),
 -- AI-012: geschätzte Eingabe-Tokens vor Korrekturfaktor (Kalibrierung Ist/Schätzung); NULL z. B. bei Cache-Treffern.
 estimated_input_tokens INTEGER CHECK(estimated_input_tokens >= 0),
 -- AI-014 (Nachtrag 24.11.2026, Q00): effektiv gesendete Reasoning-Stufe (NULL bei Decisions, Embeddings, Cache-Treffern),
 -- Reasoning-Tokens als Teilmenge von output_tokens (NULL, wenn der Anbieter nichts meldet), lokal berechnete Kosten
 -- (NULL = gleich cost_microusd; abweichend nur bei price_source 'reported') und Kennzeichen cost_mismatch (AI-005).
 reasoning_level TEXT CHECK(reasoning_level IS NULL OR reasoning_level IN
   ('model-default','off','minimal','low','medium','high','xhigh','max')),
 reasoning_tokens INTEGER CHECK(reasoning_tokens >= 0),
 computed_cost_microusd INTEGER CHECK(computed_cost_microusd >= 0),
 cost_mismatch INTEGER NOT NULL DEFAULT 0 CHECK(cost_mismatch IN (0,1)),
 -- AI-012 (Nachtrag 26.11.2026): Ausgabeschätzung A + E ohne Korrekturfaktor (Kalibrierung Ist/Schätzung je Zeile);
 -- intern wie estimated_input_tokens, keine Exportspalte.
 estimated_output_tokens INTEGER CHECK(estimated_output_tokens >= 0)
);
CREATE INDEX ai_requests_task ON ai_requests(task, model, timestamp);
CREATE INDEX ai_requests_run ON ai_requests(run_id);
CREATE TABLE ai_cache (
 cache_key TEXT PRIMARY KEY, provider TEXT NOT NULL, model TEXT NOT NULL, endpoint TEXT NOT NULL,
 definition_id TEXT NOT NULL, definition_version TEXT NOT NULL, input_hash TEXT NOT NULL,
 params_hash TEXT NOT NULL, result_json TEXT NOT NULL CHECK(json_valid(result_json)),
 status TEXT NOT NULL, created_at TEXT NOT NULL
) WITHOUT ROWID;
CREATE TABLE ai_questions (
 id TEXT NOT NULL, version TEXT NOT NULL, definition_json TEXT NOT NULL CHECK(json_valid(definition_json)),
 PRIMARY KEY(id, version)
) WITHOUT ROWID;
CREATE TRIGGER ai_questions_immutable BEFORE UPDATE ON ai_questions
 BEGIN SELECT RAISE(ABORT, 'AI question versions are immutable'); END;
CREATE TABLE ai_answers (
 url_id INTEGER NOT NULL REFERENCES urls(id) ON DELETE CASCADE,
 question_id TEXT NOT NULL, question_version TEXT NOT NULL, params_hash TEXT NOT NULL DEFAULT '',
 params_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(params_json)),
 answer_json TEXT CHECK(answer_json IS NULL OR json_valid(answer_json)),
 raw_confidence REAL, source TEXT, model TEXT, status TEXT NOT NULL,
 request_id INTEGER REFERENCES ai_requests(id),
 PRIMARY KEY(url_id, question_id, question_version, params_hash),
 FOREIGN KEY(question_id, question_version) REFERENCES ai_questions(id, version)
) WITHOUT ROWID;
CREATE TABLE ai_suggestions (
 id INTEGER PRIMARY KEY, url_id INTEGER NOT NULL REFERENCES urls(id) ON DELETE CASCADE,
 element TEXT NOT NULL, candidate INTEGER NOT NULL CHECK(candidate >= 0), value TEXT,
 status TEXT NOT NULL, violations_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(violations_json)),
 review_status TEXT NOT NULL DEFAULT 'unreviewed', accepted_value TEXT, model TEXT,
 request_id INTEGER REFERENCES ai_requests(id), UNIQUE(url_id, element, candidate)
);
CREATE TABLE ai_redirect_map (
 id INTEGER PRIMARY KEY, source_address TEXT NOT NULL, target_address TEXT, confidence REAL,
 candidates_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(candidates_json)),
 review_status TEXT NOT NULL DEFAULT 'unreviewed', status TEXT NOT NULL,
 request_id INTEGER REFERENCES ai_requests(id)
);
CREATE TABLE ai_link_suggestions (
 id INTEGER PRIMARY KEY, source_url_id INTEGER NOT NULL REFERENCES urls(id) ON DELETE CASCADE,
 target_url_id INTEGER NOT NULL REFERENCES urls(id) ON DELETE CASCADE,
 anchor TEXT, context TEXT, similarity REAL, status TEXT NOT NULL,
 request_id INTEGER REFERENCES ai_requests(id)
);
CREATE TABLE ai_queries (
 id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, question TEXT NOT NULL, sql TEXT NOT NULL,
 created_at TEXT NOT NULL, updated_at TEXT, status TEXT NOT NULL,
 request_id INTEGER REFERENCES ai_requests(id)
);
-- K4-04: prompt results and vectors are separate from decision answers.
CREATE TABLE ai_prompt_results (
 url_id INTEGER NOT NULL REFERENCES urls(id) ON DELETE CASCADE,
 prompt_id TEXT NOT NULL, prompt_version TEXT NOT NULL, result_json TEXT CHECK(result_json IS NULL OR json_valid(result_json)),
 status TEXT NOT NULL, model TEXT, request_id INTEGER REFERENCES ai_requests(id),
 PRIMARY KEY(url_id, prompt_id, prompt_version)
) WITHOUT ROWID;
CREATE TABLE ai_embeddings (
 url_id INTEGER NOT NULL REFERENCES urls(id) ON DELETE CASCADE,
 prompt_id TEXT NOT NULL, prompt_version TEXT NOT NULL, provider TEXT NOT NULL, model TEXT NOT NULL,
 dimensions INTEGER NOT NULL CHECK(dimensions > 0), input_hash TEXT NOT NULL,
 vector BLOB NOT NULL CHECK(length(vector) = dimensions * 4),
 PRIMARY KEY(url_id, prompt_id, prompt_version)
) WITHOUT ROWID;
-- AI-032 (Nachträge K4-24, 26.11.2026, und K4-26, 27.11.2026; M4-D05): gespeicherte KI-Eingaben je interner HTML-Seite
-- mit Status 200. `body` = zstd (wie page_html) eines JSON-Objekts mit dem Text des Inhaltsbereichs (höchstens 20.000
-- Zeichen), allen H2 in Dokumentreihenfolge und dem Bildkontext je Bild; `html_lang` = Rohwert von <html lang> (ohne lang
-- xml:lang); `structured_data_types` = JSON-Liste der Typen, NULL = Extraktion strukturierter Daten war aus.
CREATE TABLE ai_inputs (
 url_id INTEGER PRIMARY KEY REFERENCES urls(id) ON DELETE CASCADE,
 compression TEXT NOT NULL DEFAULT 'zstd' CHECK(compression IN ('zstd')),
 body BLOB NOT NULL,
 html_lang TEXT,
 structured_data_types TEXT CHECK(structured_data_types IS NULL OR json_valid(structured_data_types))
);
