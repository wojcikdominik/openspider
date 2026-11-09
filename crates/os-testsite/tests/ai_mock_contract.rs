mod common;
use common::Resp;
use os_testsite::{
    mocks::{
        ai::{embedding, token_count, AiMock, AnswerBook},
        MockServer,
    },
    TestSite,
};
use serde_json::{json, Value};
use tokio::{io::AsyncWriteExt, net::TcpStream};

async fn call(
    server: &MockServer,
    host: &str,
    method: &str,
    path: &str,
    scenario: &str,
    body: &str,
) -> Resp {
    let mut stream = TcpStream::connect(server.addr()).await.unwrap();
    let head = format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nContent-Type: application/json\r\nAuthorization: Bearer fixture-secret\r\nx-api-key: fixture-anthropic-secret\r\nX-Scenario: {scenario}\r\nContent-Length: {}\r\n\r\n", body.len());
    stream.write_all(head.as_bytes()).await.unwrap();
    stream.write_all(body.as_bytes()).await.unwrap();
    let (raw, err) = common::read_all(&mut stream).await;
    assert!(err.is_none(), "{err:?}");
    common::parse(&raw).unwrap()
}
fn value(r: &Resp) -> Value {
    serde_json::from_slice(&r.body).unwrap()
}
async fn server() -> (AiMock, MockServer) {
    let mock = AiMock::fixtures().unwrap();
    let s = MockServer::start(mock.registrations()).await.unwrap();
    (mock, s)
}

// spec: INTEG-019 [M4]
#[tokio::test]
async fn provider_routes_and_four_required_scenarios_are_offline() {
    let (_, s) = server().await;
    for path in [
        "/anthropic/v1/messages",
        "/openai/v1/responses",
        "/openai/v1/decisions",
        "/openrouter/api/v1/chat/completions",
        "/ollama/api/chat",
    ] {
        for (scenario, status) in [("ok", 200), ("empty", 200), ("4xx", 401), ("throttle", 429)] {
            let r = call(
                &s,
                "openai.mock.test",
                "POST",
                path,
                scenario,
                r#"{"input":"MOCK-ID:home","questions":[{"id":"q","type":"predicate"}]}"#,
            )
            .await;
            assert_eq!(r.status, status, "{path} {scenario}: {}", r.text());
            if scenario == "ok" {
                assert!(value(&r).is_object());
            }
            if scenario == "empty" {
                assert_eq!(value(&r)["data"], json!([]));
            }
        }
    }
    for path in [
        "/anthropic/v1/models",
        "/openai/v1/models",
        "/openrouter/api/v1/models",
        "/openrouter/api/v1/key",
        "/ollama/api/tags",
    ] {
        assert_eq!(
            call(&s, "openai.mock.test", "GET", path, "ok", "")
                .await
                .status,
            200
        );
    }
    assert_eq!(
        call(
            &s,
            "openai.mock.test",
            "POST",
            "/openai/v1/unknown",
            "ok",
            "{}"
        )
        .await
        .status,
        404
    );
    s.stop().await;
}
// spec: INTEG-019 [M4]
#[tokio::test]
async fn quota_and_region_matrix_matches_provider_error_bodies() {
    let (_, s) = server().await;
    let cases = [
        (
            "anthropic",
            "insufficient-quota",
            400,
            "invalid_request_error",
            "Your credit balance is too low",
        ),
        (
            "anthropic",
            "spend-limit-user",
            400,
            "invalid_request_error",
            "You have reached your specified API usage limits",
        ),
        (
            "anthropic",
            "spend-limit-workspace",
            400,
            "invalid_request_error",
            "You have reached your specified workspace API usage limits",
        ),
        ("anthropic", "402-billing", 402, "billing_error", ""),
        (
            "anthropic",
            "429-spend-cap",
            429,
            "rate_limit_error",
            "enforced_spend_limit_reached",
        ),
        ("anthropic", "529", 529, "overloaded_error", ""),
        (
            "openai",
            "insufficient-quota",
            429,
            "insufficient_quota",
            "",
        ),
        ("openrouter", "402-credits", 402, "payment_required", ""),
        ("openrouter", "402-key-limit", 402, "payment_required", ""),
        ("openrouter", "402-weight", 402, "payment_required", ""),
        ("openrouter", "402-in-flight", 402, "payment_required", ""),
        ("openrouter", "403-guardrail", 403, "permission_error", ""),
        (
            "openrouter",
            "refusal-403",
            403,
            "refusal",
            "provider reported a model refusal as an error",
        ),
    ];
    for (provider, scenario, status, kind, message) in cases {
        let endpoint = match provider {
            "anthropic" => "v1/messages",
            "openai" => "v1/responses",
            _ => "api/v1/chat/completions",
        };
        let r = call(
            &s,
            "openai.mock.test",
            "POST",
            &format!("/{provider}/{endpoint}"),
            scenario,
            "{}",
        )
        .await;
        assert_eq!(r.status, status, "{scenario}");
        let v = value(&r);
        assert_eq!(v["error"]["type"], kind);
        assert!(v["error"]["message"].as_str().unwrap().starts_with(message));
        match scenario {
            "429-spend-cap" => {
                assert!(r.header("retry-after").is_none());
                assert_eq!(
                    v["error"]["details"]["error_code"],
                    "enforced_spend_limit_reached"
                );
            }
            "insufficient-quota" if provider == "openai" => {
                assert_eq!(v["error"]["code"], "credit_balance_exhausted")
            }
            "402-credits" | "402-weight" => {
                assert_eq!(v["error"]["metadata"]["limit_source"], "openrouter_credits")
            }
            "402-key-limit" => assert_eq!(
                v["error"]["metadata"]["limit_source"],
                "openrouter_key_limit"
            ),
            "403-guardrail" => assert_eq!(
                v["error"]["metadata"]["limit_source"],
                "openrouter_guardrail_budget"
            ),
            "402-in-flight" => {
                assert_eq!(r.header("retry-after"), Some("3"));
                assert_eq!(
                    call(
                        &s,
                        "openai.mock.test",
                        "POST",
                        &format!("/{provider}/{endpoint}"),
                        scenario,
                        "{}"
                    )
                    .await
                    .status,
                    200
                );
            }
            _ => {}
        }
    }
    for (scenario, status) in [("region-denied", 403), ("region-no-endpoint", 404)] {
        assert_eq!(
            call(
                &s,
                "eu.openrouter.mock.test",
                "POST",
                "/openrouter/api/v1/chat/completions",
                scenario,
                "{}"
            )
            .await
            .status,
            status
        );
        assert_eq!(
            call(
                &s,
                "openrouter.mock.test",
                "POST",
                "/openrouter/api/v1/chat/completions",
                scenario,
                "{}"
            )
            .await
            .status,
            200
        );
    }
}
// spec: INTEG-019 [M4]
#[tokio::test]
async fn scenarios_retries_overrides_logs_and_provider_usage() {
    let (mock, s) = server().await;
    let path = "/openai/v1/responses";
    for expected in [429, 429, 200] {
        assert_eq!(
            call(&s, "openai.mock.test", "POST", path, "429x2", "{}")
                .await
                .status,
            expected
        );
    }
    let r = call(&s, "openai.mock.test", "POST", path, "retry-after-7", "{}").await;
    assert_eq!(r.header("retry-after"), Some("7"));
    assert_eq!(
        call(&s, "openai.mock.test", "POST", path, "retry-after-7", "{}")
            .await
            .status,
        200
    );
    assert_eq!(
        call(
            &s,
            "x",
            "POST",
            "/__mock/scenario",
            "ok",
            r#"{"provider":"openai","scenario":"500-always","count":2}"#
        )
        .await
        .status,
        200
    );
    // No X-Scenario header: exercise the global counter through a raw request.
    for status in [500, 500, 200] {
        let mut stream = TcpStream::connect(s.addr()).await.unwrap();
        stream.write_all(b"POST /openai/v1/responses HTTP/1.1\r\nHost: openai.mock.test\r\nConnection: close\r\nContent-Length: 2\r\n\r\n{}").await.unwrap();
        let (raw, _) = common::read_all(&mut stream).await;
        assert_eq!(common::parse(&raw).unwrap().status, status);
    }
    let text = json!({"messages":[{"role":"user","content":[{"type":"text","text":"12345"},{"type":"image","source":{"data":"not-counted"}}]}]});
    assert_eq!(token_count(&text), 87);
    assert_eq!(
        token_count(
            &json!({"input":[{"type":"input_text","text":"ä🙂abc"},{"type":"input_image","image_url":"data:image/png;base64,xxx"}]})
        ),
        87
    );
    for (p, field) in [
        ("/anthropic/v1/messages", "input_tokens"),
        (path, "input_tokens"),
        ("/openrouter/api/v1/chat/completions", "prompt_tokens"),
    ] {
        let r = call(&s, "openai.mock.test", "POST", p, "ok", &text.to_string()).await;
        assert_eq!(value(&r)["usage"][field], 87);
    }
    let r = call(&s, "x", "POST", "/ollama/api/chat", "ok", &text.to_string()).await;
    assert_eq!(value(&r)["prompt_eval_count"], 87);
    let log = call(&s, "x", "GET", "/__mock/log", "ok", "").await.text();
    assert!(!log.contains("fixture-secret"));
    assert!(!log.contains("fixture-anthropic-secret"));
    assert!(log.contains("[REDACTED]"));
    assert!(log.contains("openai.mock.test"));
    assert!(mock.requests().iter().all(|r| r.timestamp_ms > 0));
}
// spec: INTEG-019 [M4]
#[tokio::test]
async fn answer_selection_refusals_missing_schema_and_models() {
    let (_, s) = server().await;
    let input = json!({"input":"MOCK-ID:product-1","questions":[{"id":"page-type","type":"choice"},{"id":"other","type":"predicate"}]});
    let p = "/openai/v1/decisions";
    let r = call(&s, "x", "POST", p, "ok", &input.to_string()).await;
    assert_eq!(value(&r)["answers"][0]["choice"], "product");
    assert_eq!(value(&r)["answers"][0]["probabilities"][0], 0.93);
    assert_eq!(
        value(&call(&s, "x", "POST", p, "missing-answer", &input.to_string()).await)["answers"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        value(&call(&s, "x", "POST", p, "refusal", &input.to_string()).await)["answers"][0]["type"],
        "refusal"
    );
    assert!(
        value(&call(&s, "x", "POST", p, "missing-usage", &input.to_string()).await)
            .get("usage")
            .is_none()
    );
    let too_many = json!({"questions":vec![json!({"id":"q"});11]});
    assert_eq!(
        call(&s, "x", "POST", p, "ok", &too_many.to_string())
            .await
            .status,
        400
    );
    for (path, key) in [
        ("/anthropic/v1/messages", "stop_reason"),
        ("/openai/v1/responses", "output"),
        ("/openrouter/api/v1/chat/completions", "choices"),
    ] {
        assert!(!value(&call(&s, "x", "POST", path, "refusal", "{}").await)[key].is_null());
    }
    for scenario in ["refusal", "refusal-filter-only"] {
        let r = call(
            &s,
            "x",
            "POST",
            "/openrouter/api/v1/chat/completions",
            scenario,
            "{}",
        )
        .await;
        assert_eq!(value(&r)["choices"][0]["finish_reason"], "content_filter");
        assert!(value(&r)["choices"][0]["message"]["content"].is_null());
    }
    let r = call(
        &s,
        "x",
        "POST",
        "/openai/v1/responses",
        "malformed-json",
        "{}",
    )
    .await;
    assert!(serde_json::from_slice::<Value>(&r.body).is_err());
    let r = call(
        &s,
        "x",
        "POST",
        "/openai/v1/responses",
        "schema-violation",
        "{}",
    )
    .await;
    assert!(value(&r)["output"][0]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("unexpected"));
    assert_eq!(
        call(
            &s,
            "x",
            "POST",
            "/openai/v1/responses",
            "context-length",
            "{}"
        )
        .await
        .status,
        400
    );
    let m = value(&call(&s, "x", "GET", "/anthropic/v1/models", "ok", "").await);
    assert_eq!(
        m["data"][0]["capabilities"]["effort"]["max"]["supported"],
        true
    );
    assert_eq!(
        m["data"][1]["capabilities"]["effort"]["max"]["supported"],
        false
    );
    assert_eq!(m["data"][2]["capabilities"]["effort"]["supported"], false);
    assert!(m["data"][3].get("capabilities").is_none());
}
// spec: INTEG-019 [M4]
#[tokio::test]
async fn embeddings_are_deterministic_and_presets_support_threshold_pairs() {
    let (_, s) = server().await;
    for path in [
        "/openai/v1/embeddings",
        "/openrouter/api/v1/embeddings",
        "/ollama/api/embed",
    ] {
        let input = json!({"input":["MOCK-ID:product-1","MOCK-ID:product-2","MOCK-ID:product-3"],"dimensions":3});
        let r = value(&call(&s, "x", "POST", path, "ok", &input.to_string()).await);
        let v = if path.contains("ollama") {
            r["embeddings"].clone()
        } else {
            json!(r["data"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v["embedding"].clone())
                .collect::<Vec<_>>())
        };
        assert_eq!(v[0], json!([1.0, 0.0, 0.0]));
        assert_eq!(v[1][0], 0.95);
        assert_eq!(v[2][0], 0.9499);
        assert_eq!(
            r,
            value(&call(&s, "x", "POST", path, "ok", &input.to_string()).await)
        );
    }
    let a = embedding("fixture", 768);
    assert_eq!(a, embedding("fixture", 768));
    assert_ne!(a, embedding("different", 768));
    assert!((a.iter().map(|v| v * v).sum::<f64>() - 1.0).abs() < 1e-12);
    let root = os_testsite::fixtures_root()
        .join("ai")
        .join("mock-answers.d");
    assert!(AnswerBook::load(root).is_ok());
}
// spec: CRAWL-123 [M4]
#[tokio::test]
async fn host_header_survives_explicit_loopback_connection_and_regions_are_distinct() {
    let (mock, s) = server().await;
    for host in [
        "openai.mock.test",
        "eu.openai.mock.test",
        "openrouter.mock.test",
        "eu.openrouter.mock.test",
    ] {
        let r = call(
            &s,
            host,
            "POST",
            "/openai/v1/responses",
            "region-denied",
            "{}",
        )
        .await;
        assert_eq!(r.status, if host.starts_with("eu.") { 403 } else { 200 });
    }
    let hosts: Vec<_> = mock.requests().into_iter().map(|r| r.host).collect();
    assert_eq!(hosts.len(), 4);
    assert!(hosts.contains(&"eu.openai.mock.test".into()));
    let failed = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        tokio::net::lookup_host("openai.mock.test:80"),
    )
    .await;
    assert!(
        matches!(failed, Ok(Err(_))),
        "reserved .test must not resolve without override"
    );
}
// spec: INTEG-019 [M4]
#[tokio::test]
async fn fixture_site_counts_statuses_images_and_partial_changes() {
    let root = os_testsite::fixtures_root().join("ai");
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("site").join("manifest.json")).unwrap(),
    )
    .unwrap();
    let pages = manifest["pages"].as_array().unwrap();
    assert_eq!(pages.len(), 25);
    assert_eq!(pages.iter().filter(|p| p["indexable"] == true).count(), 22);
    let mut changed = 0;
    for name in ["site", "site-partial"] {
        let site = TestSite::start_dir(root.join(name), 0).await.unwrap();
        for p in pages {
            let r = common::get(&site, "ai.test", p["url"].as_str().unwrap()).await;
            assert_eq!(
                r.status,
                u16::try_from(p["status"].as_u64().unwrap()).unwrap()
            );
            assert!(r
                .text()
                .contains(&format!("MOCK-ID:{}", p["mock_id"].as_str().unwrap())));
            if name == "site-partial" && r.text().contains("MOCK-SCENARIO:") {
                changed += 1;
            }
        }
        let imgs = manifest["images"].as_array().unwrap();
        assert_eq!(imgs.len(), 6);
        for image in imgs {
            assert_eq!(
                common::get(&site, "ai.test", image.as_str().unwrap())
                    .await
                    .status,
                200
            );
        }
    }
    assert_eq!(changed, 3);
    let prices: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("pricing-test.json")).unwrap())
            .unwrap();
    assert_eq!(prices["prices"][2]["input_per_million"], 0.1);
    assert_eq!(
        prices["prices"][2]["long_context"]["threshold_input_tokens"],
        272000
    );
}
// spec: INTEG-019 [M4]
#[tokio::test]
async fn batch_files_polling_partial_and_expired_are_provider_shaped() {
    let (_, s) = server().await;
    for provider in ["anthropic", "openai"] {
        for scenario in ["ok", "batch-partial", "batch-expired"] {
            let requests:Vec<_>=(0..10).map(|i|if provider=="anthropic" {json!({"custom_id":format!("r{i}"),"params":{"messages":[{"role":"user","content":"MOCK-ID:home"}]}})} else {json!({"custom_id":format!("r{i}"),"method":"POST","url":"/v1/responses","body":{"input":"MOCK-ID:home"}})}).collect();
            let (base, body) = if provider == "anthropic" {
                (
                    "/anthropic/v1/messages/batches",
                    json!({"requests":requests}),
                )
            } else {
                let jsonl = requests
                    .iter()
                    .map(Value::to_string)
                    .collect::<Vec<_>>()
                    .join("\n");
                let f = value(&call(&s, "x", "POST", "/openai/v1/files", "ok", &jsonl).await);
                (
                    "/openai/v1/batches",
                    json!({"input_file_id":f["id"],"endpoint":"/v1/responses","completion_window":"24h"}),
                )
            };
            let b = value(&call(&s, "x", "POST", base, scenario, &body.to_string()).await);
            let id = b["id"].as_str().unwrap();
            let path = format!("{base}/{id}");
            let mut result = Value::Null;
            for n in 0..3 {
                result = value(&call(&s, "x", "GET", &path, "ok", "").await);
                assert_eq!(
                    result["processing_status"],
                    if n < 2 { "in_progress" } else { "ended" }
                );
            }
            let r = if provider == "anthropic" {
                call(&s, "x", "GET", &format!("{path}/results"), "ok", "").await
            } else {
                call(
                    &s,
                    "x",
                    "GET",
                    &format!(
                        "/openai/v1/files/{}/content",
                        result["output_file_id"].as_str().unwrap()
                    ),
                    "ok",
                    "",
                )
                .await
            };
            assert_eq!(r.status, 200);
            let lines: Vec<Value> = r
                .text()
                .lines()
                .map(|l| serde_json::from_str(l).unwrap())
                .collect();
            assert_eq!(lines.len(), 10);
            let failures = lines
                .iter()
                .filter(|l| {
                    if provider == "anthropic" {
                        l["result"]["type"] != "succeeded"
                    } else {
                        !l["error"].is_null()
                    }
                })
                .count();
            assert_eq!(
                failures,
                match scenario {
                    "ok" => 0,
                    "batch-partial" => 1,
                    _ => 10,
                }
            );
        }
    }
    assert_eq!(
        call(
            &s,
            "x",
            "POST",
            "/openai/v1/batches",
            "ok",
            r#"{"endpoint":"/v1/decisions"}"#
        )
        .await
        .status,
        400
    );
}
// spec: INTEG-019 [M4]
#[tokio::test]
async fn per_url_markers_timeout_and_delay() {
    let (_, s) = server().await;
    // Empty X-Scenario is an explicit override; omit it to test the page marker.
    let mut stream = TcpStream::connect(s.addr()).await.unwrap();
    let body = r#"{"input":"MOCK-ID:home MOCK-SCENARIO:500-always"}"#;
    stream.write_all(format!("POST /openai/v1/responses HTTP/1.1\r\nHost: x\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
    let (raw, _) = common::read_all(&mut stream).await;
    assert_eq!(common::parse(&raw).unwrap().status, 500);
    let started = std::time::Instant::now();
    assert_eq!(
        call(&s, "x", "POST", "/openai/v1/responses", "delay=1", "{}")
            .await
            .status,
        200
    );
    assert!(started.elapsed() >= std::time::Duration::from_secs(1));
    assert!(tokio::time::timeout(
        std::time::Duration::from_millis(30),
        call(&s, "x", "POST", "/openai/v1/responses", "hang", "{}")
    )
    .await
    .is_err());
}
// spec: INTEG-019 [M4]
#[tokio::test]
async fn multipart_upload_and_token_counter_use_real_provider_shapes() {
    let (_, s) = server().await;
    let jsonl = r#"{"custom_id":"r1","method":"POST","url":"/v1/responses","body":{"input":"MOCK-ID:home"}}"#;
    let body=format!("--fixture-boundary\r\nContent-Disposition: form-data; name=\"purpose\"\r\n\r\nbatch\r\n--fixture-boundary\r\nContent-Disposition: form-data; name=\"file\"; filename=\"batch.jsonl\"\r\nContent-Type: application/jsonl\r\n\r\n{jsonl}\r\n--fixture-boundary--\r\n");
    let mut stream = TcpStream::connect(s.addr()).await.unwrap();
    stream.write_all(format!("POST /openai/v1/files HTTP/1.1\r\nHost: openai.mock.test\r\nConnection: close\r\nContent-Type: multipart/form-data; boundary=fixture-boundary\r\nContent-Length: {}\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
    let (raw, _) = common::read_all(&mut stream).await;
    let uploaded = value(&common::parse(&raw).unwrap());
    assert_eq!(uploaded["bytes"], jsonl.len());
    let id = uploaded["id"].as_str().unwrap();
    assert_eq!(
        call(
            &s,
            "x",
            "GET",
            &format!("/openai/v1/files/{id}/content"),
            "ok",
            ""
        )
        .await
        .text(),
        jsonl
    );
    let input = json!({"system":"ab","messages":[{"role":"user","content":[{"type":"text","text":"cde"},{"type":"image","source":{"type":"base64","data":"ignore"}}]}]});
    let counted = value(
        &call(
            &s,
            "x",
            "POST",
            "/anthropic/v1/messages/count_tokens",
            "ok",
            &input.to_string(),
        )
        .await,
    );
    assert_eq!(counted["input_tokens"], 87);
    let r = value(&call(&s, "x", "POST", "/openai/v1/responses", "refusal", "{}").await);
    assert_eq!(r["usage"]["output_tokens"], 4);
    let ollama = value(&call(&s, "x", "POST", "/ollama/api/chat", "refusal", "{}").await);
    assert_eq!(ollama["message"]["content"], "Fixture refusal");
    assert_eq!(ollama["eval_count"], 4);
    let r = value(
        &call(
            &s,
            "x",
            "POST",
            "/openai/v1/decisions",
            "ok",
            r#"{"input":"abcd","questions":[{"id":"q","type":"predicate","question":"efgh"}]}"#,
        )
        .await,
    );
    assert_eq!(r["usage"]["input_tokens"], 2);
    assert_eq!(
        r["usage"]["output_tokens"],
        r["answers"].to_string().chars().count().div_ceil(4)
    );
}

// spec: INTEG-019 [M4]
#[tokio::test]
async fn model_catalog_changes_are_visible_without_restarting_the_mock() {
    let (mock, s) = server().await;
    let path = "/anthropic/v1/models";
    let mut catalog = value(&call(&s, "x", "GET", path, "ok", "").await);
    assert_eq!(
        catalog["data"][0]["capabilities"]["image_input"]["supported"],
        true
    );
    catalog["data"][0]["capabilities"]["image_input"]["supported"] = json!(false);
    mock.set_models("anthropic", catalog.clone());
    assert_eq!(value(&call(&s, "x", "GET", path, "ok", "").await), catalog);
}

// spec: INTEG-019 [M4]
#[tokio::test]
async fn openrouter_eu_catalog_is_a_five_model_subset_of_eight_global_models() {
    let (mock, s) = server().await;
    let path = "/openrouter/api/v1/models";
    let global = value(&call(&s, "openrouter.mock.test", "GET", path, "ok", "").await);
    let eu = value(&call(&s, "eu.openrouter.mock.test", "GET", path, "ok", "").await);
    let global = global["data"].as_array().unwrap();
    let eu = eu["data"].as_array().unwrap();
    assert_eq!(global.len(), 8);
    assert_eq!(eu.len(), 5);
    assert!(eu.iter().all(|model| global.contains(model)));
    let vision = eu
        .iter()
        .find(|model| model["id"] == "mock/vision")
        .unwrap();
    assert_eq!(vision["pricing"]["request"], "0.001");
    assert_eq!(vision["pricing"]["image"], "0.002");
    assert_eq!(mock.requests()[0].host, "openrouter.mock.test");
    assert_eq!(mock.requests()[1].host, "eu.openrouter.mock.test");
}
