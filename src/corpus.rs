//! Deterministic multi-seed corpus preparation and its explicit write machine.

use crate::model::{Format, Schema};
use crate::{generate, render};

/// One file to write, prepared entirely in the pure core.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedFile {
    pub name: String,
    pub text: String,
    pub seed: u64,
}

/// Render one fixture per seed, in ascending order. Bounded so a typo cannot
/// fill a disk; names are stable and sortable.
pub fn prepare(schema: &Schema, start: u64, end: u64, format: Format) -> Result<Vec<PreparedFile>, String> {
    if start > end {
        return Err("seed range is inverted; start must not exceed end".to_string());
    }
    if end - start >= 1000 {
        return Err("a corpus may contain at most 1000 seeds".to_string());
    }
    let extension = format.label();
    let mut files = Vec::with_capacity((end - start + 1) as usize);
    for seed in start..=end {
        let mut seeded = schema.clone();
        seeded.seed = seed;
        let dataset = generate::generate(&seeded);
        let rendered = render::render(&dataset, format);
        files.push(PreparedFile {
            name: format!("{}_{}.{}", schema.record.to_ascii_lowercase(), seed, extension),
            text: rendered.text,
            seed,
        });
    }
    Ok(files)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CorpusPhase {
    Idle,
    Reading,
    Parsing,
    Validating,
    Preparing,
    Writing { index: usize },
    Done,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CorpusRun {
    pub phase: CorpusPhase,
    pub schema: Option<Schema>,
    pub files: Vec<PreparedFile>,
    pub start: u64,
    pub end: u64,
    pub format: Format,
    pub out_dir: String,
    pub written: usize,
}

impl CorpusRun {
    pub fn new() -> Self {
        Self {
            phase: CorpusPhase::Idle,
            schema: None,
            files: Vec::new(),
            start: 0,
            end: 0,
            format: Format::Json,
            out_dir: String::new(),
            written: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CorpusEvent {
    Start {
        path: String,
        start: u64,
        end: u64,
        format: Option<Format>,
        out_dir: String,
    },
    Loaded { text: String },
    Parsed { schema: Schema },
    Validated { schema: Schema },
    Prepared { files: Vec<PreparedFile> },
    Written,
    Failed { message: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CorpusAction {
    Read { path: String },
    Parse { text: String },
    Validate { schema: Schema },
    Prepare { schema: Schema, start: u64, end: u64, format: Format },
    Write { dir: String, name: String, text: String },
    Print { text: String },
    Error { message: String },
    Exit { code: i32 },
}

fn unexpected(phase: CorpusPhase) -> Vec<CorpusAction> {
    vec![
        CorpusAction::Error { message: format!("unexpected corpus event in phase {:?}", phase) },
        CorpusAction::Exit { code: 2 },
    ]
}

fn next_write(state: &CorpusRun, index: usize) -> Vec<CorpusAction> {
    let file = &state.files[index];
    vec![CorpusAction::Write {
        dir: state.out_dir.clone(),
        name: file.name.clone(),
        text: file.text.clone(),
    }]
}

/// Advance the corpus machine. Exactly one write action is emitted per event,
/// so a failed write never leaves an unaccounted file.
pub fn transition(state: &mut CorpusRun, event: CorpusEvent) -> Vec<CorpusAction> {
    match (state.phase, event) {
        (CorpusPhase::Idle, CorpusEvent::Start { path, start, end, format, out_dir }) => {
            state.start = start;
            state.end = end;
            state.format = format.unwrap_or(Format::Json);
            state.out_dir = out_dir;
            state.phase = CorpusPhase::Reading;
            vec![CorpusAction::Read { path }]
        }
        (CorpusPhase::Reading, CorpusEvent::Loaded { text }) => {
            state.phase = CorpusPhase::Parsing;
            vec![CorpusAction::Parse { text }]
        }
        (CorpusPhase::Parsing, CorpusEvent::Parsed { schema }) => {
            state.schema = Some(schema.clone());
            state.phase = CorpusPhase::Validating;
            vec![CorpusAction::Validate { schema }]
        }
        (CorpusPhase::Validating, CorpusEvent::Validated { schema }) => {
            state.schema = Some(schema.clone());
            state.phase = CorpusPhase::Preparing;
            vec![CorpusAction::Prepare { schema, start: state.start, end: state.end, format: state.format }]
        }
        (CorpusPhase::Preparing, CorpusEvent::Prepared { files }) => {
            state.files = files;
            if state.files.is_empty() {
                return vec![CorpusAction::Error { message: "corpus has no files".to_string() }, CorpusAction::Exit { code: 2 }];
            }
            state.phase = CorpusPhase::Writing { index: 0 };
            next_write(state, 0)
        }
        (CorpusPhase::Writing { index }, CorpusEvent::Written) => {
            state.written += 1;
            if index + 1 < state.files.len() {
                state.phase = CorpusPhase::Writing { index: index + 1 };
                next_write(state, index + 1)
            } else {
                state.phase = CorpusPhase::Done;
                vec![
                    CorpusAction::Print {
                        text: format!("corpus: {} files written to {}\n", state.written, state.out_dir),
                    },
                    CorpusAction::Exit { code: 0 },
                ]
            }
        }
        (_, CorpusEvent::Failed { message }) => {
            state.phase = CorpusPhase::Failed;
            vec![CorpusAction::Error { message }, CorpusAction::Exit { code: 2 }]
        }
        (phase, _) => unexpected(phase),
    }
}
