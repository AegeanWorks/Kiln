//! Versioned multi-fixture manifest and its explicit run state machine.
//! Pure: text in, data and actions out.

use crate::model::{Format, GenerationMode, Verdict};

/// One (schema, committed fixture, format) triple to verify.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FixtureEntry {
    pub schema: String,
    pub fixture: String,
    pub format: Format,
    pub mode: GenerationMode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    pub version: u32,
    pub entries: Vec<FixtureEntry>,
}

fn strip_comment(line: &str) -> &str {
    match line.find('#') {
        Some(index) => &line[..index],
        None => line,
    }
}

/// Parse the line-oriented `kiln.toml` manifest. Unknown keys fail; the source
/// text is never modified by this reader.
pub fn parse(text: &str) -> Result<Manifest, String> {
    let mut version = 1u32;
    let mut entries = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let number = index + 1;
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| format!("line {}: expected 'key = value'", number))?;
        match key.trim() {
            "version" => {
                version = value
                    .trim()
                    .parse()
                    .map_err(|_| format!("line {}: version must be an integer", number))?;
            }
            "fixture" => {
                let parts: Vec<&str> = value.split(',').map(str::trim).collect();
                if parts.len() < 3 || parts.len() > 4 || parts.iter().any(|part| part.is_empty()) {
                    return Err(format!(
                        "line {}: fixture needs schema, fixture, format and optional mode (seeded|edges)",
                        number
                    ));
                }
                let format = Format::parse(parts[2]).map_err(|error| format!("line {}: {}", number, error))?;
                let mode = match parts.get(3).copied() {
                    Some("edges") => GenerationMode::Edges,
                    Some("seeded") | None => GenerationMode::Seeded,
                    Some(other) => {
                        return Err(format!("line {}: unknown generation mode '{}'", number, other));
                    }
                };
                entries.push(FixtureEntry {
                    schema: parts[0].to_string(),
                    fixture: parts[1].to_string(),
                    format,
                    mode,
                });
            }
            other => return Err(format!("line {}: unknown key '{}'", number, other)),
        }
    }
    Ok(Manifest { version, entries })
}

/// Refuse an unsupported version or an empty/duplicated fixture set.
pub fn validate(manifest: &Manifest) -> Result<(), String> {
    if manifest.version != 1 {
        return Err(format!(
            "unsupported manifest version {}; this reader handles 1 only",
            manifest.version
        ));
    }
    if manifest.entries.is_empty() {
        return Err("manifest needs at least one fixture entry".to_string());
    }
    let mut seen = Vec::new();
    for entry in &manifest.entries {
        let key = format!("{}|{}", entry.schema, entry.fixture);
        if seen.contains(&key) {
            return Err(format!("duplicate fixture entry '{}'", key));
        }
        seen.push(key);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManifestPhase {
    Idle,
    Reading,
    Parsing,
    Running { index: usize },
    Done,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManifestRun {
    pub phase: ManifestPhase,
    pub text: String,
    pub manifest: Option<Manifest>,
    pub verdicts: Vec<Verdict>,
    pub json: bool,
}

impl ManifestRun {
    pub fn new() -> Self {
        Self {
            phase: ManifestPhase::Idle,
            text: String::new(),
            manifest: None,
            verdicts: Vec::new(),
            json: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManifestEvent {
    Start { path: String, json: bool },
    Loaded { text: String },
    Parsed { manifest: Manifest },
    EntryDone { verdict: Verdict },
    Failed { message: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManifestAction {
    Read { path: String },
    Parse { text: String },
    RunEntry { entry: FixtureEntry },
    Print { text: String },
    Error { message: String },
    Exit { code: i32 },
}

fn unexpected(phase: ManifestPhase) -> Vec<ManifestAction> {
    vec![
        ManifestAction::Error { message: format!("unexpected manifest event in phase {:?}", phase) },
        ManifestAction::Exit { code: 2 },
    ]
}

fn summary(verdicts: &[Verdict]) -> String {
    let matched = verdicts.iter().filter(|verdict| **verdict == Verdict::Pass).count();
    let failed = verdicts.len() - matched;
    format!(
        "manifest: {}/{} fixtures matched, {} failed\n",
        matched,
        verdicts.len(),
        failed
    )
}

/// Advance the manifest machine by one event. Entries are visited in declared
/// order; an error at any point stops the run with exit code 2.
pub fn transition(state: &mut ManifestRun, event: ManifestEvent) -> Vec<ManifestAction> {
    match (state.phase, event) {
        (ManifestPhase::Idle, ManifestEvent::Start { path, json }) => {
            state.json = json;
            state.phase = ManifestPhase::Reading;
            vec![ManifestAction::Read { path }]
        }
        (ManifestPhase::Reading, ManifestEvent::Loaded { text }) => {
            state.text = text.clone();
            state.phase = ManifestPhase::Parsing;
            vec![ManifestAction::Parse { text }]
        }
        (ManifestPhase::Parsing, ManifestEvent::Parsed { manifest }) => {
            if manifest.entries.is_empty() {
                state.phase = ManifestPhase::Failed;
                return vec![
                    ManifestAction::Error { message: "manifest has no fixture entries".to_string() },
                    ManifestAction::Exit { code: 2 },
                ];
            }
            state.manifest = Some(manifest.clone());
            state.phase = ManifestPhase::Running { index: 0 };
            vec![ManifestAction::RunEntry { entry: manifest.entries[0].clone() }]
        }
        (ManifestPhase::Running { index }, ManifestEvent::EntryDone { verdict }) => {
            state.verdicts.push(verdict);
            let manifest = state.manifest.as_ref().expect("running implies a parsed manifest");
            if index + 1 < manifest.entries.len() {
                state.phase = ManifestPhase::Running { index: index + 1 };
                vec![ManifestAction::RunEntry { entry: manifest.entries[index + 1].clone() }]
            } else {
                state.phase = ManifestPhase::Done;
                let code = if state.verdicts.iter().all(|value| *value == Verdict::Pass) { 0 } else { 1 };
                let text = if state.json {
                    crate::report::render_manifest(&state.verdicts)
                } else {
                    summary(&state.verdicts)
                };
                vec![ManifestAction::Print { text }, ManifestAction::Exit { code }]
            }
        }
        (_, ManifestEvent::Failed { message }) => {
            state.phase = ManifestPhase::Failed;
            vec![ManifestAction::Error { message }, ManifestAction::Exit { code: 2 }]
        }
        (phase, _) => unexpected(phase),
    }
}
