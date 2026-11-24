//! Einzeltests: Testbild, Modelllisten-Reduktion, Modellwahl, Kostenrechnung, Anfragekörper nach 08.

use probe::plan::{self, ModelNeed, Provider, Via};
use probe::png::{adler32, crc32, test_png, SIZE};
use probe::record::{self, choose_anthropic, choose_openai, choose_openrouter, summarize_models};
use probe::requests::{adapter_request, fallback_schema, limits_schema, raw_body};
use serde_json::{json, Value};

// spec: AI-002 (Bild als PNG mit korrektem media_type)
#[test]
fn test_image_is_a_valid_png() {
    let png = test_png();
    assert_eq!(
        os_ai::adapters::anthropic::sniff_image_media_type(&png),
        Some("image/png")
    );
    // Chunks mit gültiger CRC: IHDR, IDAT, IEND.
    let mut pos = 8;
    let mut kinds = Vec::new();
    while pos < png.len() {
        let len = u32::from_be_bytes(png[pos..pos + 4].try_into().unwrap()) as usize;
        let kind = &png[pos + 4..pos + 8];
        let data = &png[pos + 8..pos + 8 + len];
        let crc = u32::from_be_bytes(png[pos + 8 + len..pos + 12 + len].try_into().unwrap());
        let mut input = kind.to_vec();
        input.extend_from_slice(data);
        assert_eq!(crc32(&input), crc);
        kinds.push(String::from_utf8(kind.to_vec()).unwrap());
        if kind == b"IHDR" {
            assert_eq!(u32::from_be_bytes(data[0..4].try_into().unwrap()), SIZE);
        }
        if kind == b"IDAT" {
            let raw = &data[7..data.len() - 4];
            assert_eq!(raw.len(), (SIZE as usize) * (1 + 3 * SIZE as usize));
            let adler = u32::from_be_bytes(data[data.len() - 4..].try_into().unwrap());
            assert_eq!(adler32(raw), adler);
        }
        pos += 12 + len;
    }
    assert_eq!(kinds, ["IHDR", "IDAT", "IEND"]);
    assert!(png.len() < 4_000, "winziges Testbild");
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
}

// spec: AI-002, AI-005 (Modelllisten: Feldnamen plus wenige Beispiele)
#[test]
fn model_lists_are_reduced_to_field_paths_and_examples() {
    let entries: Vec<Value> = (0..50)
        .map(|i| json!({"id": format!("m{i}"), "pricing": {"prompt": "0.000001"}, "capabilities": {"image_input": {"supported": i % 2 == 0}}, "description": "x".repeat(1000)}))
        .collect();
    let s = summarize_models(&json!({"data": entries, "has_more": false}), true);
    assert_eq!(s["count"], 50);
    assert_eq!(
        s["examples"].as_array().unwrap().len(),
        record::MODEL_EXAMPLES
    );
    assert!(
        s["entry_field_paths"]["capabilities.image_input.supported"]["types"] == json!(["boolean"])
    );
    assert_eq!(s["entry_field_paths"]["pricing.prompt"]["entries"], 50);
    assert_eq!(s["top_level_fields"]["has_more"], false);
    assert_eq!(s["ids"].as_array().unwrap().len(), 50);
    assert!(s.to_string().len() < 5_000);
    assert!(summarize_models(&json!({"data": []}), false)
        .get("ids")
        .is_none());
}

// spec: AI-002 (Modell aus der Liste, Fähigkeiten aus capabilities)
#[test]
fn anthropic_choice_comes_from_the_list() {
    let body = json!({"data": [
        {"id": "big", "capabilities": {"structured_outputs": {"supported": true}, "image_input": {"supported": true}}},
        {"id": "small-haiku-x", "capabilities": {"structured_outputs": {"supported": false}, "image_input": {"supported": true}}},
        {"id": "small-haiku-y", "capabilities": {"structured_outputs": {"supported": true}, "image_input": {"supported": true}}}
    ]});
    assert_eq!(choose_anthropic(&body, None).unwrap().id, "small-haiku-y");
    assert_eq!(choose_anthropic(&body, Some("big")).unwrap().id, "big");
    assert!(choose_anthropic(&body, Some("not-listed")).is_err());
    let no_caps = json!({"data": [{"id": "a"}, {"id": "b"}]});
    let c = choose_anthropic(&no_caps, None).unwrap();
    assert_eq!(c.id, "a");
    assert!(c.rule.contains("kein Modell meldet"));
    assert!(choose_anthropic(&json!({"data": []}), None).is_err());
}

// spec: AI-003, AI-004, AI-010 (gpt-6-luna nur, wenn gelistet)
#[test]
fn openai_choice_prefers_listed_luna_then_small_text_models() {
    let with = json!({"data": [{"id": "gpt-6-luna"}, {"id": "gpt-x-nano"}]});
    assert_eq!(choose_openai(&with, None).unwrap().id, "gpt-6-luna");
    let without = json!({"data": [
        {"id": "gpt-x-nano-2026-01-01"}, {"id": "gpt-x-nano"}, {"id": "gpt-x-mini"},
        {"id": "gpt-x-nano-tts"}, {"id": "text-embedding-nano"}
    ]});
    assert_eq!(choose_openai(&without, None).unwrap().id, "gpt-x-nano");
    let only_mini = json!({"data": [{"id": "gpt-x-mini-transcribe"}, {"id": "gpt-x-mini"}]});
    assert_eq!(choose_openai(&only_mini, None).unwrap().id, "gpt-x-mini");
    assert!(choose_openai(&json!({"data": [{"id": "o9"}]}), None).is_err());
    assert_eq!(
        choose_openai(&with, Some("gpt-x-nano")).unwrap().id,
        "gpt-x-nano"
    );
}

// spec: AI-005 (Preise und Fähigkeiten aus der Liste)
#[test]
fn openrouter_choice_is_cheapest_eligible_per_capability() {
    let body = json!({"data": [
        {"id": "a/free:free", "pricing": {"prompt": "0", "completion": "0"}, "supported_parameters": ["structured_outputs"], "architecture": {"input_modalities": ["text", "image"]}},
        {"id": "a/zero", "pricing": {"prompt": "0", "completion": "0"}},
        {"id": "a/var", "pricing": {"prompt": "-1", "completion": "-1"}},
        {"id": "a/text", "pricing": {"prompt": "0.0000001", "completion": "0.0000001"}, "supported_parameters": ["max_tokens"]},
        {"id": "a/schema", "pricing": {"prompt": "0.0000003", "completion": "0.0000003"}, "supported_parameters": ["response_format"]},
        {"id": "a/vision", "pricing": {"prompt": "0.0000005", "completion": "0.0000005", "image": "0.0001"}, "architecture": {"input_modalities": ["image", "text"], "output_modalities": ["text"]}},
        {"id": "a/pricey-vision", "pricing": {"prompt": "0.00005", "completion": "0.0002"}, "architecture": {"input_modalities": ["image"]}},
        {"id": "a/fee", "pricing": {"prompt": "0.0000001", "completion": "0.0000001", "request": "0.05"}},
        {"id": "a/image-out", "pricing": {"prompt": "0.00000001", "completion": "0.00000001"}, "architecture": {"output_modalities": ["image"]}}
    ]});
    assert_eq!(
        choose_openrouter(&body, None, ModelNeed::Text).unwrap().id,
        "a/text"
    );
    assert_eq!(
        choose_openrouter(&body, None, ModelNeed::Structured)
            .unwrap()
            .id,
        "a/schema"
    );
    let v = choose_openrouter(&body, None, ModelNeed::Vision).unwrap();
    assert_eq!(v.id, "a/vision");
    assert!((v.rates.per_image - 0.0001).abs() < 1e-12);
    assert!(choose_openrouter(&body, Some("a/pricey-vision"), ModelNeed::Vision).is_err());
    assert_eq!(record::router_models(&body).len(), 3);
}

// spec: AI-001 (usage-Felder je Anbieter)
#[test]
fn usage_and_costs() {
    assert_eq!(
        record::tokens_from_usage(
            &json!({"input_tokens": 10, "output_tokens": 3, "cache_read_input_tokens": 5})
        ),
        Some((15, 3))
    );
    assert_eq!(
        record::tokens_from_usage(&json!({"prompt_tokens": 7, "completion_tokens": 2})),
        Some((7, 2))
    );
    assert_eq!(record::tokens_from_usage(&json!({"total_tokens": 7})), None);
    let c = plan::cost_usd(&plan::CONSERVATIVE, 1_000_000, 0, 0);
    assert!((c - 30.0).abs() < 1e-9);
    let text = plan::step_by_id("anthropic.raw.text");
    let models = plan::step_by_id("anthropic.raw.models");
    assert_eq!(
        plan::worst_case_usd(models, 1000, 0, &plan::CONSERVATIVE),
        0.0
    );
    let w = plan::worst_case_usd(text, 200, 0, &plan::CONSERVATIVE);
    assert!((w - ((100 + 64) as f64 * 30e-6 + 128.0 * 150e-6)).abs() < 1e-12);
}

// spec: AI-002, AI-003, AI-004, AI-005, AI-034 (Rohkörper nach Spec-Wortlaut)
#[test]
fn raw_bodies_follow_spec_wording() {
    let img = "QUJD";
    let a = raw_body(plan::step_by_id("anthropic.raw.structured"), "m", img).unwrap();
    assert_eq!(a["output_config"]["format"]["type"], "json_schema");
    assert!(a.get("output_format").is_none());
    assert_eq!(a["system"].as_str().unwrap(), probe::requests::SYSTEM_TEXT);
    let ai = raw_body(plan::step_by_id("anthropic.raw.image"), "m", img).unwrap();
    assert_eq!(ai["messages"][0]["content"][0]["source"]["type"], "base64");
    assert_eq!(
        ai["messages"][0]["content"][0]["source"]["media_type"],
        "image/png"
    );

    let o = raw_body(
        plan::step_by_id("openai.raw.responses_structured"),
        "m",
        img,
    )
    .unwrap();
    assert_eq!(o["text"]["format"]["type"], "json_schema");
    assert_eq!(o["text"]["format"]["strict"], true);
    let oi = raw_body(plan::step_by_id("openai.raw.responses_image"), "m", img).unwrap();
    assert_eq!(oi["input"][0]["content"][1]["type"], "input_image");
    assert!(oi["input"][0]["content"][1]["image_url"]
        .as_str()
        .unwrap()
        .starts_with("data:image/png;base64,"));

    let d = raw_body(
        plan::step_by_id("openai.raw.decisions_3q"),
        "gpt-6-luna",
        img,
    )
    .unwrap();
    let types: Vec<&str> = d["questions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|q| q["type"].as_str().unwrap())
        .collect();
    assert_eq!(types, ["predicate", "choice", "score"]);
    assert!(d["questions"][1]["choices"][0]["value"].is_string());
    assert!(d["questions"][2]["levels"][0]["label"].is_string());
    let d11 = raw_body(
        plan::step_by_id("openai.raw.decisions_11q"),
        "gpt-6-luna",
        img,
    )
    .unwrap();
    assert_eq!(d11["questions"].as_array().unwrap().len(), 11);
    let long = raw_body(
        plan::step_by_id("openai.raw.decisions_long_instructions"),
        "gpt-6-luna",
        img,
    )
    .unwrap();
    assert_eq!(
        long["questions"][0]["instructions"]
            .as_str()
            .unwrap()
            .chars()
            .count(),
        4_000
    );

    let r = raw_body(plan::step_by_id("openrouter.raw.structured"), "m", img).unwrap();
    assert_eq!(r["response_format"]["type"], "json_schema");
    assert_eq!(r["provider"]["require_parameters"], true);
    assert_eq!(r["provider"]["data_collection"], "deny");
    let rt = raw_body(plan::step_by_id("openrouter.raw.text"), "m", img).unwrap();
    assert!(rt["provider"].get("require_parameters").is_none());

    // AI-034/AI-021: strikt-verträgliche Schemas.
    for schema in [fallback_schema(), limits_schema()] {
        let props = schema["properties"].as_object().unwrap();
        let required: Vec<&str> = schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(props.keys().all(|k| required.contains(&k.as_str())));
        assert_eq!(schema["additionalProperties"], false);
        assert!(required.contains(&"abstain"));
    }
    let limits = limits_schema();
    assert_eq!(
        limits["properties"]["page_type"]["enum"]
            .as_array()
            .unwrap()
            .len(),
        20
    );
    assert_eq!(
        limits["properties"]["page_type"]["description"]
            .as_str()
            .unwrap()
            .chars()
            .count(),
        2_000
    );

    // GET-Schritte ohne Körper; jeder Roh-POST hat einen, jeder Adapter-Ausführungsschritt einen Request.
    for step in plan::STEPS {
        match (step.via, step.method) {
            (Via::Raw, "GET") => assert!(raw_body(step, "m", img).is_none(), "{}", step.id),
            (Via::Raw, _) => assert!(raw_body(step, "m", img).is_some(), "{}", step.id),
            (Via::Adapter, "POST") => {
                assert!(adapter_request(step, "m", img).is_some(), "{}", step.id)
            }
            (Via::Adapter, _) => assert!(adapter_request(step, "m", img).is_none(), "{}", step.id),
        }
    }
    let dec = adapter_request(
        plan::step_by_id("openai.adapter.decisions_3q"),
        "ignored",
        img,
    )
    .unwrap();
    assert_eq!(dec.model.id, "gpt-6-luna");
    assert_eq!(dec.questions.len(), 3);
    assert!(plan::steps(Provider::OpenAi).any(|s| s.path == "/decisions"));
}

// spec: AI-004, AI-010; Auswertung Q00: Spec-ID über OpenRouter bevorzugt (Vergleichbarkeit)
#[test]
fn openrouter_prefers_the_spec_model_when_eligible() {
    let body = json!({"data": [
        {"id": "tiny/cheap", "pricing": {"prompt": "0.00000001", "completion": "0.00000001"},
         "supported_parameters": ["structured_outputs"], "architecture": {"input_modalities": ["text", "image"]}},
        {"id": "openai/gpt-6-luna", "pricing": {"prompt": "0.0000001", "completion": "0.0000005"},
         "supported_parameters": ["structured_outputs", "response_format"],
         "architecture": {"input_modalities": ["file", "image", "text"], "output_modalities": ["text"]}}
    ]});
    for need in [ModelNeed::Text, ModelNeed::Structured, ModelNeed::Vision] {
        let c = choose_openrouter(&body, None, need).unwrap();
        assert_eq!(c.id, record::OPENROUTER_PREFERRED);
        assert!(c.rule.contains("Spec-ID"));
    }
    let without = json!({"data": [body["data"][0].clone()]});
    assert_eq!(
        choose_openrouter(&without, None, ModelNeed::Text)
            .unwrap()
            .id,
        "tiny/cheap"
    );
}

// spec: AI-009; Auswertung Q00
#[test]
fn sanitize_keeps_model_ids_and_spec_fields() {
    let v = json!({"id": "claude-haiku-5-5", "model": "gpt-6-luna", "label": "poor", "request_id": "req_1",
                   "budgets": [{"budget_id": null}], "limit_remaining": 50, "usage": {"input_tokens": 3}});
    let s = record::sanitize(&v);
    assert_eq!(s["id"], "claude-haiku-5-5");
    assert_eq!(s["label"], "poor");
    assert_eq!(s["request_id"], "req_…(ausgelassen: Anbieter-ID)");
    assert!(s["budgets"][0]["budget_id"].is_null());
    assert_eq!(s["limit_remaining"], 50);
    assert_eq!(record::sanitize(&s), s);
}
