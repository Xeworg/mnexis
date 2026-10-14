use jsonschema::draft202012::{meta, options};
use serde_json::{Value, json};

const CONTRACT_SCHEMAS: [(&str, &str); 2] = [
    (
        "memory.schema.json",
        include_str!("../docs/contracts/v1/memory.schema.json"),
    ),
    (
        "operation.schema.json",
        include_str!("../docs/contracts/v1/operation.schema.json"),
    ),
];

const SCENARIO_SCHEMA: &str = include_str!("../fixtures/e0/scenario.schema.json");

const E0_FIXTURES: [(&str, &str); 4] = [
    (
        "no-answer-and-scope.json",
        include_str!("../fixtures/e0/no-answer-and-scope.json"),
    ),
    (
        "revision-conflict.json",
        include_str!("../fixtures/e0/revision-conflict.json"),
    ),
    (
        "superseded-decision.json",
        include_str!("../fixtures/e0/superseded-decision.json"),
    ),
    (
        "two-agents-shared-profile.json",
        include_str!("../fixtures/e0/two-agents-shared-profile.json"),
    ),
];

#[test]
fn e0_contract_schemas_validate_as_draft_2020_12() {
    for (name, source) in CONTRACT_SCHEMAS {
        let schema: Value = serde_json::from_str(source).expect(name);
        meta::validate(&schema).unwrap_or_else(|error| panic!("{name}: {error}"));
        options()
            .should_validate_formats(true)
            .build(&schema)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
    }
}

#[test]
fn e0_contract_schemas_accept_valid_instances_and_reject_invalid_formats() {
    let memory_schema: Value = serde_json::from_str(CONTRACT_SCHEMAS[0].1).unwrap();
    let memory_validator = options()
        .should_validate_formats(true)
        .build(&memory_schema)
        .unwrap();
    let valid_memory = json!({
        "id": "11111111-1111-4111-8111-111111111111",
        "namespace": "project-a",
        "type": "decision",
        "content": "Use the custom graph engine.",
        "state": "active",
        "revision": 1,
        "assertion_basis": "observed",
        "provenance": {
            "actor": "agent-a",
            "channel": "test",
            "created_at": "2026-10-13T12:00:00Z"
        }
    });
    assert!(memory_validator.is_valid(&valid_memory));
    let mut invalid_uuid_memory = valid_memory.clone();
    invalid_uuid_memory["id"] = json!("not-a-uuid");
    assert!(!memory_validator.is_valid(&invalid_uuid_memory));

    let mut invalid_date_time_memory = valid_memory.clone();
    invalid_date_time_memory["provenance"]["created_at"] = json!("not-a-date-time");
    assert!(!memory_validator.is_valid(&invalid_date_time_memory));

    let operation_schema: Value = serde_json::from_str(CONTRACT_SCHEMAS[1].1).unwrap();
    let operation_validator = options()
        .should_validate_formats(true)
        .build(&operation_schema)
        .unwrap();
    for instance in [
        json!({
            "task": "Find the accepted graph engine.",
            "namespace": "project-a",
            "token_budget": 1200,
            "tokenizer_id": "test-tokenizer",
            "max_items": 10
        }),
        json!({
            "id": "11111111-1111-4111-8111-111111111111",
            "expected_revision": 1,
            "reason": "Corrected evidence.",
            "content": "Use the custom graph engine.",
            "state": "active"
        }),
    ] {
        assert!(operation_validator.is_valid(&instance));
    }
    let invalid_operation = json!({
        "task": "Find the accepted graph engine.",
        "namespace": "project-a",
        "token_budget": 0,
        "tokenizer_id": "test-tokenizer",
        "max_items": 10
    });
    assert!(!operation_validator.is_valid(&invalid_operation));
}

#[test]
fn all_e0_scenario_fixtures_match_their_scenario_schema() {
    let schema: Value = serde_json::from_str(SCENARIO_SCHEMA).expect("scenario.schema.json");
    meta::validate(&schema).expect("scenario.schema.json must follow Draft 2020-12");
    let validator = options()
        .should_validate_formats(true)
        .build(&schema)
        .expect("scenario.schema.json must compile");

    for (name, source) in E0_FIXTURES {
        let fixture: Value = serde_json::from_str(source).expect(name);
        assert!(
            validator.is_valid(&fixture),
            "fixture does not match: {name}"
        );
    }
    assert!(!validator.is_valid(&json!({ "scenario": "unknown" })));
}
