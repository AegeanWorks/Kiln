//! Explicit state machine for a generation or comparison run.
//!
//! Every phase transition is enumerated. The machine never performs I/O: it
//! returns `Action` commands that the shell interprets at the edge.

use crate::model::{Dataset, Format, GenerationMode, Rendered, Schema, Verdict};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Loading,
    Parsing,
    Validating,
    Generating,
    Rendering,
    Comparing,
    Done,
    Failed,
}

/// All data the machine carries between phases.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunState {
    pub phase: Phase,
    pub text: String,
    pub schema: Option<Schema>,
    pub dataset: Option<Dataset>,
    pub format: Option<Format>,
    pub mode: Option<GenerationMode>,
    pub format_override: Option<Format>,
    pub count_override: Option<usize>,
    pub out_path: Option<String>,
    pub fixture_text: String,
    pub check: bool,
    pub json: bool,
}

impl RunState {
    pub fn new() -> Self {
        Self {
            phase: Phase::Idle,
            text: String::new(),
            schema: None,
            dataset: None,
            format: None,
            mode: None,
            format_override: None,
            count_override: None,
            out_path: None,
            fixture_text: String::new(),
            check: false,
            json: false,
        }
    }
}

/// Inputs the shell can feed back into the machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Start {
        path: String,
        format: Option<Format>,
        count: Option<usize>,
        out: Option<String>,
        fixture_text: String,
        check: bool,
        mode: GenerationMode,
        json: bool,
    },
    Loaded { text: String },
    Parsed { schema: Schema },
    Validated { schema: Schema },
    Generated { dataset: Dataset },
    Rendered { rendered: Rendered },
    Compared { verdict: Verdict, changes: usize },
    Failed { message: String },
}

/// Effects the shell must execute. Pure computations (Parse, Validate,
/// Generate, Render, Compare) are also actions so the phase order is explicit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Read { path: String },
    Parse { text: String },
    Validate { schema: Schema },
    Generate { schema: Schema, mode: GenerationMode },
    Render { dataset: Dataset, format: Format },
    Compare { expected: String, actual: String },
    Write { path: String, text: String },
    Print { text: String },
    Error { message: String },
    Exit { code: i32 },
}

fn with_overrides(schema: Schema, format: Option<Format>, count: Option<usize>) -> Schema {
    let mut schema = schema;
    if let Some(value) = format {
        schema.format = value;
    }
    if let Some(value) = count {
        schema.count = value;
    }
    schema
}

fn unexpected(phase: Phase) -> Vec<Action> {
    vec![
        Action::Error {
            message: format!("unexpected event in phase {:?}", phase),
        },
        Action::Exit { code: 2 },
    ]
}

/// Advance the machine by one event. Returns the actions to execute.
pub fn transition(state: &mut RunState, event: Event) -> Vec<Action> {
    match (state.phase, event) {
        (Phase::Idle, Event::Start { path, format, count, out, fixture_text, check, mode, json }) => {
            state.format_override = format;
            state.count_override = count;
            state.out_path = out;
            state.fixture_text = fixture_text;
            state.check = check;
            state.mode = Some(mode);
            state.json = json;
            state.phase = Phase::Loading;
            vec![Action::Read { path }]
        }
        (Phase::Loading, Event::Loaded { text }) => {
            state.text = text.clone();
            state.phase = Phase::Parsing;
            vec![Action::Parse { text }]
        }
        (Phase::Parsing, Event::Parsed { schema }) => {
            state.schema = Some(schema.clone());
            state.phase = Phase::Validating;
            vec![Action::Validate { schema }]
        }
        (Phase::Validating, Event::Validated { schema }) => {
            let schema = with_overrides(schema, state.format_override, state.count_override);
            state.format = Some(schema.format);
            state.schema = Some(schema.clone());
            state.phase = Phase::Generating;
            let mode = state.mode.unwrap_or(GenerationMode::Seeded);
            vec![Action::Generate { schema, mode }]
        }
        (Phase::Generating, Event::Generated { dataset }) => {
            state.dataset = Some(dataset.clone());
            state.phase = Phase::Rendering;
            let format = state.format.unwrap_or(crate::model::Format::Json);
            vec![Action::Render { dataset, format }]
        }
        (Phase::Rendering, Event::Rendered { rendered }) => {
            state.text = rendered.text.clone();
            if state.check {
                state.phase = Phase::Comparing;
                vec![Action::Compare { expected: state.fixture_text.clone(), actual: rendered.text }]
            } else {
                state.phase = Phase::Done;
                let mut actions = Vec::new();
                if let Some(path) = state.out_path.clone() {
                    actions.push(Action::Write { path, text: state.text.clone() });
                    actions.push(Action::Print { text: "wrote fixture to output file\n".to_string() });
                } else {
                    actions.push(Action::Print { text: rendered.text });
                }
                actions.push(Action::Exit { code: 0 });
                actions
            }
        }
        (Phase::Comparing, Event::Compared { verdict, changes }) => {
            state.phase = Phase::Done;
            let text = if state.json {
                crate::report::render_check(verdict, changes, state.format.unwrap_or(crate::model::Format::Json))
            } else {
                let mut human = crate::render::render_verdict(verdict);
                human.push_str(&format!("verdict: {}\n", verdict.label()));
                human
            };
            vec![
                Action::Print { text },
                Action::Exit { code: if verdict == Verdict::Pass { 0 } else { 1 } },
            ]
        }
        (_, Event::Failed { message }) => {
            state.phase = Phase::Failed;
            vec![Action::Error { message }, Action::Exit { code: 2 }]
        }
        (phase, _) => unexpected(phase),
    }
}
