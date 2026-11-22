//! Pure-core tests. No filesystem, no process spawning, no clock.

use kiln_core::cli::{self, Command};
use kiln_core::compare::compare_fixture;
use kiln_core::generate::generate;
use kiln_core::machine::{self, Action, Event, Phase, RunState};
use kiln_core::model::{Format, Verdict};
use kiln_core::render::{render, render_csv, render_json};
use kiln_core::rng::Rng;
use kiln_core::{schema, schema::validate};

const EXAMPLE_SCHEMA: &str = include_str!("../examples/users.kiln");
const GOLDEN_JSON: &str = include_str!("../examples/users.json");
const GOLDEN_CSV: &str = include_str!("../examples/users.csv");

fn example_schema() -> kiln_core::model::Schema {
    let parsed = schema::parse(EXAMPLE_SCHEMA).expect("example schema must parse");
    validate(&parsed).expect("example schema must validate");
    parsed
}

#[test]
fn rng_is_deterministic_for_a_seed() {
    let mut a = Rng::new(42);
    let mut b = Rng::new(42);
    let mut c = Rng::new(43);
    let first: Vec<u64> = (0..8).map(|_| a.next_u64()).collect();
    let second: Vec<u64> = (0..8).map(|_| b.next_u64()).collect();
    let other: Vec<u64> = (0..8).map(|_| c.next_u64()).collect();
    assert_eq!(first, second);
    assert_ne!(first, other);
}

#[test]
fn example_schema_parses_and_validates() {
    let parsed = example_schema();
    assert_eq!(parsed.record, "User");
    assert_eq!(parsed.seed, 20_261_122);
    assert_eq!(parsed.count, 4);
    assert_eq!(parsed.format, Format::Json);
    assert_eq!(parsed.fields.len(), 6);
    assert_eq!(parsed.fields[0].name, "id");
}

#[test]
fn unknown_schema_version_is_refused() {
    let text = EXAMPLE_SCHEMA.replace("version = 1", "version = 2");
    let parsed = schema::parse(&text).expect("parse itself still succeeds");
    let error = validate(&parsed).expect_err("version 2 must be refused");
    assert!(error.contains("unsupported schema version 2"), "{}", error);
}

#[test]
fn inverted_range_is_refused() {
    let text = EXAMPLE_SCHEMA.replace("int(1..9999)", "int(9..2)");
    let parsed = schema::parse(&text).expect("parse itself still succeeds");
    assert!(validate(&parsed).is_err());
}

#[test]
fn generation_matches_golden_json_bytes() {
    let dataset = generate(&example_schema());
    assert_eq!(render_json(&dataset), GOLDEN_JSON);
}

#[test]
fn generation_matches_golden_csv_bytes() {
    let mut parsed = example_schema();
    parsed.format = Format::Csv;
    let dataset = generate(&parsed);
    assert_eq!(render_csv(&dataset), GOLDEN_CSV);
}

#[test]
fn render_metadata_reports_records_and_bytes() {
    let rendered = render(&generate(&example_schema()), Format::Json);
    assert_eq!(rendered.records, 4);
    assert_eq!(rendered.bytes, GOLDEN_JSON.len());
}

#[test]
fn machine_applies_count_and_format_overrides() {
    let mut state = RunState::new();
    let mut actions = machine::transition(&mut state, Event::Start {
        path: "unused".to_string(),
        format: Some(Format::Csv),
        count: Some(1),
        out: None,
        fixture_text: String::new(),
        check: false,
    });
    assert_eq!(state.phase, Phase::Loading);
    assert!(matches!(actions[0], Action::Read { .. }));

    actions = machine::transition(&mut state, Event::Loaded { text: EXAMPLE_SCHEMA.to_string() });
    assert!(matches!(actions[0], Action::Parse { .. }));
    actions = machine::transition(&mut state, Event::Parsed { schema: example_schema() });
    assert!(matches!(actions[0], Action::Validate { .. }));
    actions = machine::transition(&mut state, Event::Validated { schema: example_schema() });
    match &actions[0] {
        Action::Generate { schema } => {
            assert_eq!(schema.count, 1);
            assert_eq!(schema.format, Format::Csv);
        }
        other => panic!("expected Generate, got {:?}", other),
    }
    actions = machine::transition(&mut state, Event::Generated { dataset: generate(&example_schema()) });
    assert!(matches!(actions[0], Action::Render { format: Format::Csv, .. }));

    let rendered = render(&generate(&example_schema()), Format::Csv);
    actions = machine::transition(&mut state, Event::Rendered { rendered });
    assert_eq!(state.phase, Phase::Done);
    assert!(actions.iter().any(|action| matches!(action, Action::Exit { code: 0 })));
}

#[test]
fn machine_refuses_an_event_in_the_wrong_phase() {
    let mut state = RunState::new();
    let actions = machine::transition(&mut state, Event::Loaded { text: String::new() });
    assert!(actions.iter().any(|action| matches!(
        action,
        Action::Error { .. }
    )));
    assert!(actions.iter().any(|action| matches!(action, Action::Exit { code: 2 })));
}

#[test]
fn machine_check_mode_returns_a_nonzero_code_on_mismatch() {
    let mut state = RunState::new();
    machine::transition(&mut state, Event::Start {
        path: "unused".to_string(),
        format: None,
        count: None,
        out: None,
        fixture_text: "stale bytes".to_string(),
        check: true,
    });
    machine::transition(&mut state, Event::Loaded { text: EXAMPLE_SCHEMA.to_string() });
    machine::transition(&mut state, Event::Parsed { schema: example_schema() });
    machine::transition(&mut state, Event::Validated { schema: example_schema() });
    machine::transition(&mut state, Event::Generated { dataset: generate(&example_schema()) });
    let rendered = render(&generate(&example_schema()), Format::Json);
    let actions = machine::transition(&mut state, Event::Rendered { rendered });
    assert_eq!(state.phase, Phase::Comparing);
    assert!(actions.iter().any(|action| matches!(action, Action::Compare { .. })));
    let actions = machine::transition(&mut state, Event::Compared { verdict: Verdict::Fail });
    assert!(actions.iter().any(|action| matches!(action, Action::Exit { code: 1 })));
}

#[test]
fn compare_normalizes_line_endings_and_trailing_blank_lines() {
    assert_eq!(compare_fixture("a\r\n", "a\n"), Verdict::Pass);
    assert_eq!(compare_fixture("a\n", "b\n"), Verdict::Fail);
}

#[test]
fn cli_parses_every_command() {
    let args = vec![
        "generate".to_string(), "s.kiln".to_string(),
        "--format".to_string(), "csv".to_string(),
        "--count".to_string(), "3".to_string(),
    ];
    assert_eq!(
        cli::parse(&args).unwrap(),
        Command::Generate {
            schema: "s.kiln".to_string(),
            format: Some(Format::Csv),
            count: Some(3),
            out: None,
        }
    );
    let check = vec!["check".to_string(), "s.kiln".to_string(), "--fixture".to_string(), "f.json".to_string()];
    assert!(matches!(cli::parse(&check).unwrap(), Command::Check { .. }));
    let demo = vec!["demo".to_string(), "--scripted".to_string(), "--seed".to_string(), "7".to_string()];
    assert_eq!(cli::parse(&demo).unwrap(), Command::Demo { seed: 7 });
    assert_eq!(cli::parse(&[]).unwrap(), Command::Help);
}

#[test]
fn cli_rejects_unknown_commands_with_a_diagnosis() {
    let error = cli::parse(&["bogus".to_string()]).expect_err("unknown command must fail");
    assert!(error.contains("Why:"), "{}", error);
    assert!(error.contains("Fix:"), "{}", error);
}
