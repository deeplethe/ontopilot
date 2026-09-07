use utopia_extract::{
    json_block, normalize_attr_value, parse_adjudication, parse_response, parse_time,
    AdjudicationPair, AdjudicationSide,
};

// ---------------------------------------------------------------------------
// json_block
// ---------------------------------------------------------------------------

#[test]
fn json_block_bare_json() {
    let raw = r#"{"entities":[],"facts":[]}"#;
    let got = json_block(raw).unwrap();
    assert_eq!(got, raw);
}

#[test]
fn json_block_strips_json_fence() {
    let raw = "```json\n{\"entities\":[]}\n```";
    let got = json_block(raw).unwrap();
    assert_eq!(got, r#"{"entities":[]}"#);
}

#[test]
fn json_block_strips_plain_fence() {
    let raw = "```\n{\"entities\":[]}\n```";
    let got = json_block(raw).unwrap();
    assert_eq!(got, r#"{"entities":[]}"#);
}

#[test]
fn json_block_tolerates_leading_prose() {
    let raw = "Here is the extraction:\n{\"entities\":[],\"facts\":[]}";
    let got = json_block(raw).unwrap();
    assert_eq!(got, r#"{"entities":[],"facts":[]}"#);
}

#[test]
fn json_block_no_braces_returns_error() {
    let err = json_block("no json here").unwrap_err();
    assert!(err.to_string().contains("No JSON found"));
}

// ---------------------------------------------------------------------------
// parse_response — happy paths
// ---------------------------------------------------------------------------

#[test]
fn parse_response_empty_arrays() {
    let raw = r#"{"entities":[],"facts":[]}"#;
    let ex = parse_response(raw).unwrap();
    assert!(ex.entities.is_empty());
    assert!(ex.facts.is_empty());
    assert_eq!(ex.skipped_entities, 0);
    assert_eq!(ex.skipped_facts, 0);
    assert!(!ex.truncated);
}

#[test]
fn parse_response_parses_entity_fields() {
    let raw = r#"{
        "entities": [{
            "local_id": "e1",
            "name": "Acme Corp",
            "type": "organization",
            "specific_type": "technology company"
        }],
        "facts": []
    }"#;
    let ex = parse_response(raw).unwrap();
    assert_eq!(ex.entities.len(), 1);
    let e = &ex.entities[0];
    assert_eq!(e.local_id.as_deref(), Some("e1"));
    assert_eq!(e.name, "Acme Corp");
    assert_eq!(e.type_key, "organization");
    assert_eq!(e.specific_type.as_deref(), Some("technology company"));
}

#[test]
fn parse_response_parses_fact_fields() {
    let raw = r#"{
        "entities": [
            {"local_id":"e1","name":"Alice","type":"person","specific_type":"engineer"},
            {"local_id":"e2","name":"Acme","type":"organization","specific_type":"company"}
        ],
        "facts": [{
            "subject": "Alice",
            "subject_ref": "e1",
            "predicate": "works_at",
            "object": "Acme",
            "object_ref": "e2",
            "valid_from": "2020-01",
            "confidence": 0.9,
            "quote": "Alice works at Acme."
        }]
    }"#;
    let ex = parse_response(raw).unwrap();
    assert_eq!(ex.facts.len(), 1);
    let f = &ex.facts[0];
    assert_eq!(f.subject, "Alice");
    assert_eq!(f.subject_ref.as_deref(), Some("e1"));
    assert_eq!(f.predicate, "works_at");
    assert_eq!(f.object.as_deref(), Some("Acme"));
    assert_eq!(f.object_ref.as_deref(), Some("e2"));
    assert_eq!(f.valid_from.as_deref(), Some("2020-01"));
    assert!((f.confidence.unwrap() - 0.9).abs() < 1e-6);
    assert_eq!(f.quote.as_deref(), Some("Alice works at Acme."));
}

#[test]
fn parse_response_fact_without_object_is_attribute_fact() {
    let raw = r#"{
        "entities": [{"local_id":"e1","name":"Alice","type":"person","specific_type":"engineer"}],
        "facts": [{
            "subject": "Alice",
            "subject_ref": "e1",
            "predicate": "salary",
            "value": 120000,
            "quote": "earns 120,000"
        }]
    }"#;
    let ex = parse_response(raw).unwrap();
    let f = &ex.facts[0];
    assert!(f.object.is_none());
    assert!(f.value.is_some());
}

#[test]
fn parse_response_inside_code_fence() {
    let raw = "```json\n{\"entities\":[],\"facts\":[]}\n```";
    let ex = parse_response(raw).unwrap();
    assert!(ex.entities.is_empty());
}

// ---------------------------------------------------------------------------
// parse_response — edge cases
// ---------------------------------------------------------------------------

#[test]
fn parse_response_empty_string_errors() {
    let err = parse_response("").unwrap_err();
    assert!(err.to_string().contains("No JSON found"));
}

#[test]
fn parse_response_malformed_json_errors() {
    // Not repairable — no complete object at all
    let err = parse_response("{broken json [[[").unwrap_err();
    assert!(err.to_string().to_lowercase().contains("json") || !err.to_string().is_empty());
}

#[test]
fn parse_response_missing_required_name_skips_entity() {
    // ExtractedEntity.name is required; missing it should increment skipped_entities
    let raw = r#"{
        "entities": [
            {"local_id":"e1","type":"organization","specific_type":"company"},
            {"local_id":"e2","name":"Acme","type":"organization","specific_type":"company"}
        ],
        "facts": []
    }"#;
    let ex = parse_response(raw).unwrap();
    // The bad entity is skipped; the good one is kept
    assert_eq!(ex.entities.len(), 1);
    assert_eq!(ex.entities[0].name, "Acme");
    assert_eq!(ex.skipped_entities, 1);
}

#[test]
fn parse_response_missing_required_predicate_skips_fact() {
    let raw = r#"{
        "entities": [],
        "facts": [
            {"subject":"Alice","quote":"q"},
            {"subject":"Bob","predicate":"knows","object":"Carol","quote":"q2"}
        ]
    }"#;
    let ex = parse_response(raw).unwrap();
    assert_eq!(ex.facts.len(), 1);
    assert_eq!(ex.facts[0].subject, "Bob");
    assert_eq!(ex.skipped_facts, 1);
}

#[test]
fn parse_response_duplicate_local_ids_both_dropped() {
    // Two entities share local_id "e1" — neither should survive deduplication
    let raw = r#"{
        "entities": [
            {"local_id":"e1","name":"Alice","type":"person","specific_type":"engineer"},
            {"local_id":"e1","name":"Alice Clone","type":"person","specific_type":"engineer"}
        ],
        "facts": []
    }"#;
    let ex = parse_response(raw).unwrap();
    assert!(ex.entities.is_empty(), "duplicated handles must both be dropped");
    assert_eq!(ex.skipped_entities, 2);
}

#[test]
fn parse_response_entity_without_local_id_is_kept() {
    // local_id is optional; absence is fine (legacy)
    let raw = r#"{
        "entities": [{"name":"Unnamed Corp","type":"organization","specific_type":"company"}],
        "facts": []
    }"#;
    let ex = parse_response(raw).unwrap();
    assert_eq!(ex.entities.len(), 1);
    assert!(ex.entities[0].local_id.is_none());
}

#[test]
fn parse_response_truncated_json_is_repaired() {
    // Truncated after the first entity — the brackets are not closed
    let raw = r#"{"entities":[{"local_id":"e1","name":"Alice","type":"person","specific_type":"x"}],"facts":["#;
    let ex = parse_response(raw).unwrap();
    assert!(ex.truncated);
    // At minimum the completed entity should survive
    assert_eq!(ex.entities.len(), 1);
    assert_eq!(ex.entities[0].name, "Alice");
}

#[test]
fn parse_response_missing_arrays_treated_as_empty() {
    // The model omits the arrays entirely (serde default kicks in)
    let raw = r#"{}"#;
    let ex = parse_response(raw).unwrap();
    assert!(ex.entities.is_empty());
    assert!(ex.facts.is_empty());
}

// ---------------------------------------------------------------------------
// parse_adjudication
// ---------------------------------------------------------------------------

#[test]
fn parse_adjudication_happy_path() {
    let raw = r#"{"verdicts":[
        {"i":0,"verdict":"same","confidence":0.9,"why":"shared parent"},
        {"i":1,"verdict":"different","confidence":0.85,"why":"distinct roles"}
    ]}"#;
    let vs = parse_adjudication(raw).unwrap();
    assert_eq!(vs.len(), 2);
    assert_eq!(vs[0].i, 0);
    assert_eq!(vs[0].verdict, "same");
    assert_eq!(vs[1].verdict, "different");
}

#[test]
fn parse_adjudication_in_code_fence() {
    let raw = "```json\n{\"verdicts\":[{\"i\":0,\"verdict\":\"unsure\"}]}\n```";
    let vs = parse_adjudication(raw).unwrap();
    assert_eq!(vs[0].verdict, "unsure");
}

#[test]
fn parse_adjudication_empty_verdicts() {
    let raw = r#"{"verdicts":[]}"#;
    let vs = parse_adjudication(raw).unwrap();
    assert!(vs.is_empty());
}

#[test]
fn parse_adjudication_invalid_json_errors() {
    let err = parse_adjudication("not json").unwrap_err();
    assert!(!err.to_string().is_empty());
}

// ---------------------------------------------------------------------------
// parse_time
// ---------------------------------------------------------------------------

#[test]
fn parse_time_year_only() {
    let (dt, prec) = parse_time("2023").unwrap();
    assert_eq!(prec, "year");
    assert_eq!(dt.format("%Y-%m-%d").to_string(), "2023-01-01");
}

#[test]
fn parse_time_year_month() {
    let (dt, prec) = parse_time("2023-06").unwrap();
    assert_eq!(prec, "month");
    assert_eq!(dt.format("%Y-%m-%d").to_string(), "2023-06-01");
}

#[test]
fn parse_time_full_date() {
    let (dt, prec) = parse_time("2023-06-15").unwrap();
    assert_eq!(prec, "day");
    assert_eq!(dt.format("%Y-%m-%d").to_string(), "2023-06-15");
}

#[test]
fn parse_time_datetime_with_z() {
    let (dt, prec) = parse_time("2023-06-15T14:30Z").unwrap();
    assert_eq!(prec, "minute");
    assert_eq!(dt.format("%Y-%m-%dT%H:%M").to_string(), "2023-06-15T14:30");
}

#[test]
fn parse_time_datetime_with_positive_offset() {
    // +08:00 → subtract 8 hours to get UTC
    let (dt, prec) = parse_time("2023-06-15T22:00+08:00").unwrap();
    assert_eq!(prec, "minute");
    assert_eq!(dt.format("%Y-%m-%dT%H:%M").to_string(), "2023-06-15T14:00");
}

#[test]
fn parse_time_datetime_no_zone_falls_back_to_day() {
    // A time-of-day without a zone must be treated as a plain date
    let (dt, prec) = parse_time("2023-06-15T14:30").unwrap();
    assert_eq!(prec, "day");
    assert_eq!(dt.format("%Y-%m-%d").to_string(), "2023-06-15");
}

#[test]
fn parse_time_empty_string_returns_none() {
    assert!(parse_time("").is_none());
}

#[test]
fn parse_time_null_literal_returns_none() {
    assert!(parse_time("null").is_none());
}

#[test]
fn parse_time_garbage_returns_none() {
    assert!(parse_time("not-a-date").is_none());
}

// ---------------------------------------------------------------------------
// normalize_attr_value
// ---------------------------------------------------------------------------

#[test]
fn normalize_number_from_json_number() {
    let v = serde_json::json!(42.5);
    let out = normalize_attr_value("number", &v).unwrap();
    assert_eq!(out, serde_json::json!(42.5));
}

#[test]
fn normalize_number_from_string_with_separators() {
    let v = serde_json::json!("1,200,000");
    let out = normalize_attr_value("number", &v).unwrap();
    assert_eq!(out, serde_json::json!(1_200_000.0_f64));
}

#[test]
fn normalize_number_from_garbage_string_returns_none() {
    let v = serde_json::json!("not-a-number");
    assert!(normalize_attr_value("number", &v).is_none());
}

#[test]
fn normalize_date_valid() {
    let v = serde_json::json!("2023-06");
    let out = normalize_attr_value("date", &v).unwrap();
    assert_eq!(out, serde_json::json!("2023-06"));
}

#[test]
fn normalize_date_invalid_returns_none() {
    let v = serde_json::json!("not-a-date");
    assert!(normalize_attr_value("date", &v).is_none());
}

#[test]
fn normalize_bool_from_json_bool() {
    assert_eq!(
        normalize_attr_value("bool", &serde_json::json!(true)).unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        normalize_attr_value("bool", &serde_json::json!(false)).unwrap(),
        serde_json::json!(false)
    );
}

#[test]
fn normalize_bool_from_string_yes_no() {
    assert_eq!(
        normalize_attr_value("bool", &serde_json::json!("yes")).unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        normalize_attr_value("bool", &serde_json::json!("no")).unwrap(),
        serde_json::json!(false)
    );
}

#[test]
fn normalize_bool_garbage_returns_none() {
    assert!(normalize_attr_value("bool", &serde_json::json!("maybe")).is_none());
}

#[test]
fn normalize_text_type_trims_and_accepts_string() {
    let v = serde_json::json!("  hello world  ");
    let out = normalize_attr_value("text", &v).unwrap();
    assert_eq!(out, serde_json::json!("hello world"));
}

#[test]
fn normalize_text_empty_string_returns_none() {
    let v = serde_json::json!("");
    assert!(normalize_attr_value("text", &v).is_none());
}

#[test]
fn normalize_unknown_type_accepts_number_as_string() {
    let v = serde_json::json!(99);
    let out = normalize_attr_value("custom_type", &v).unwrap();
    assert_eq!(out, serde_json::json!("99"));
}
