//! Imperative shell: the only layer that performs I/O. Every action it runs
//! comes from the pure state machine; every result goes back as an event.

use std::collections::VecDeque;
use std::fs;
use std::io::Read;

use kiln_core::corpus::{self, CorpusAction, CorpusEvent, CorpusRun};
use kiln_core::machine::{self, Action, Event, RunState};
use kiln_core::manifest::{self, ManifestAction, ManifestEvent, ManifestRun};
use kiln_core::model::{Format, GenerationMode, Verdict};
use kiln_core::{compare, drift, generate, render, report, schema};

/// Write to stdout. Kept here so the core never touches a stream.
pub fn print_out(text: &str) {
    print!("{}", text);
}

/// Write a diagnosed error to stderr.
pub fn print_err(message: &str) {
    eprintln!("{}", message);
}

fn read_text(path: &str) -> Result<String, String> {
    if path == "-" {
        let mut input = String::new();
        return std::io::stdin().read_to_string(&mut input).map(|_| input).map_err(|error| {
            format!(
                "cannot read stdin: {}\nWhy: the piped schema must be valid UTF-8.\nFix: pipe a .kiln schema into kiln generate -.",
                error
            )
        });
    }
    fs::read_to_string(path).map_err(|error| {
        format!(
            "cannot read '{}': {}\nWhy: the input must exist and be valid UTF-8.\nFix: pass the path to a readable .kiln schema or fixture.",
            path, error
        )
    })
}

fn write_text(path: &str, text: &str) -> Result<(), String> {
    fs::write(path, text).map_err(|error| {
        format!(
            "cannot write '{}': {}\nWhy: the output directory must exist and be writable.\nFix: check the parent directory or omit --out to write to stdout.",
            path, error
        )
    })
}

fn write_corpus_file(dir: &str, name: &str, text: &str) -> Result<(), String> {
    let folder = std::path::Path::new(dir);
    fs::create_dir_all(folder).map_err(|error| {
        format!(
            "cannot create corpus directory '{}': {}\nWhy: the output directory must be creatable.\nFix: pass a writable --out-dir path.",
            dir, error
        )
    })?;
    let path = folder.join(name);
    fs::write(&path, text).map_err(|error| {
        format!(
            "cannot write corpus file '{}': {}\nWhy: the output directory must be writable.\nFix: check permissions or choose another --out-dir.",
            path.display(), error
        )
    })
}

fn parse_failure(error: &str) -> String {
    format!(
        "cannot use schema: {}\nWhy: generation is deterministic only for a valid version-1 schema.\nFix: kiln explain describes the schema grammar.",
        error
    )
}

/// Drive the state machine until it requests an exit. Returns its exit code.
fn run_machine(start: Event, quiet: bool) -> i32 {
    let mut state = RunState::new();
    let mut queue: VecDeque<Event> = VecDeque::new();
    queue.push_back(start);
    let mut code = 0;

    while let Some(event) = queue.pop_front() {
        for action in machine::transition(&mut state, event) {
            match action {
                Action::Read { path } => match read_text(&path) {
                    Ok(text) => queue.push_back(Event::Loaded { text }),
                    Err(message) => queue.push_back(Event::Failed { message }),
                },
                Action::Parse { text } => match schema::parse(&text) {
                    Ok(parsed) => queue.push_back(Event::Parsed { schema: parsed }),
                    Err(error) => queue.push_back(Event::Failed { message: parse_failure(&error) }),
                },
                Action::Validate { schema: parsed } => match schema::validate(&parsed) {
                    Ok(()) => queue.push_back(Event::Validated { schema: parsed }),
                    Err(error) => queue.push_back(Event::Failed { message: parse_failure(&error) }),
                },
                Action::Generate { schema: parsed, mode } => {
                    let dataset = match mode {
                        GenerationMode::Edges => generate::generate_edges(&parsed),
                        GenerationMode::Seeded => generate::generate(&parsed),
                    };
                    queue.push_back(Event::Generated { dataset });
                }
                Action::Render { dataset, format } => {
                    queue.push_back(Event::Rendered { rendered: render::render(&dataset, format) });
                }
                Action::Compare { expected, actual } => {
                    let verdict = compare::compare_fixture(&expected, &actual);
                    let format = state.format.unwrap_or(Format::Json);
                    let count = drift::count_changes(&expected, &actual, format);
                    if verdict == Verdict::Fail && !state.json && !quiet {
                        print_out(&drift::drift_report(&expected, &actual, format));
                    }
                    queue.push_back(Event::Compared { verdict, changes: count });
                }
                Action::Write { path, text } => {
                    if let Err(message) = write_text(&path, &text) {
                        queue.push_back(Event::Failed { message });
                    }
                }
                Action::Print { text } => {
                    if !quiet {
                        print_out(&text);
                    }
                }
                Action::Error { message } => print_err(&message),
                Action::Exit { code: requested } => {
                    code = requested;
                    queue.clear();
                    break;
                }
            }
        }
    }
    code
}

/// `generate` command: read a schema, then let the machine own the pipeline.
pub fn run_generate(
    path: String,
    format: Option<Format>,
    count: Option<usize>,
    out: Option<String>,
    edges: bool,
    seed: Option<u64>,
) -> i32 {
    run_machine(Event::Start {
        path,
        format,
        count,
        out,
        fixture_text: String::new(),
        check: false,
        mode: if edges { GenerationMode::Edges } else { GenerationMode::Seeded },
        json: false,
        seed,
    }, false)
}

/// One fixture check. The format override is used by manifest entries, which
/// declare their own format rather than inheriting it from the schema.
fn run_single_check(path: String, fixture: String, format: Option<Format>, mode: GenerationMode, json: bool, quiet: bool, seed: Option<u64>) -> i32 {
    match read_text(&fixture) {
        Ok(text) => run_machine(Event::Start {
            path,
            format,
            count: None,
            out: None,
            fixture_text: text,
            check: true,
            mode,
            json,
            seed,
        }, quiet),
        Err(message) => {
            print_err(&message);
            2
        }
    }
}

/// `check` command: read the committed fixture at the edge, then compare.
pub fn run_check(path: String, fixture: String, json: bool, seed: Option<u64>) -> i32 {
    run_single_check(path, fixture, None, GenerationMode::Seeded, json, false, seed)
}

/// `diff` command: compare two committed fixtures without regenerating.
pub fn run_diff(left: String, right: String, format: Option<Format>, json: bool) -> i32 {
    let chosen = match format {
        Some(value) => value,
        None => match Format::from_path(&left) {
            Ok(value) => value,
            Err(message) => {
                print_err(&format!("{}\nWhy: drift needs a format to parse the fixture.\nFix: pass --format json|jsonl|csv.", message));
                return 2;
            }
        },
    };
    let first = match read_text(&left) {
        Ok(text) => text,
        Err(message) => {
            print_err(&message);
            return 2;
        }
    };
    let second = match read_text(&right) {
        Ok(text) => text,
        Err(message) => {
            print_err(&message);
            return 2;
        }
    };
    let verdict = compare::compare_fixture(&first, &second);
    if json {
        let found = match drift::changes(&first, &second, chosen) {
            Ok(value) => value,
            Err(error) => vec![kiln_core::drift::Change {
                path: "$".to_string(),
                kind: kiln_core::drift::ChangeKind::Changed,
                expected: "parseable fixture".to_string(),
                actual: error,
            }],
        };
        print_out(&report::render_diff(verdict, &found));
    } else if verdict == Verdict::Pass {
        print_out("fixtures MATCH: byte-identical after normalization\n");
    } else {
        print_out(&drift::drift_report(&first, &second, chosen));
        print_out("fixtures DIFFER\n");
    }
    if verdict == Verdict::Pass { 0 } else { 1 }
}

/// `check --manifest` command: an explicit machine walks every declared entry.
pub fn run_manifest(path: String, json: bool) -> i32 {
    let mut state = ManifestRun::new();
    let mut queue: VecDeque<ManifestEvent> = VecDeque::new();
    queue.push_back(ManifestEvent::Start { path, json });
    let mut code = 0;
    while let Some(event) = queue.pop_front() {
        for action in manifest::transition(&mut state, event) {
            match action {
                ManifestAction::Read { path } => match read_text(&path) {
                    Ok(text) => queue.push_back(ManifestEvent::Loaded { text }),
                    Err(message) => queue.push_back(ManifestEvent::Failed { message }),
                },
                ManifestAction::Parse { text } => match manifest::parse(&text)
                    .and_then(|parsed| {
                        manifest::validate(&parsed)?;
                        Ok(parsed)
                    }) {
                    Ok(parsed) => queue.push_back(ManifestEvent::Parsed { manifest: parsed }),
                    Err(error) => queue.push_back(ManifestEvent::Failed {
                        message: format!(
                            "cannot use manifest: {}\nWhy: a version-1 manifest with at least one fixture is required.\nFix: each fixture line is 'schema, fixture, format'.",
                            error
                        ),
                    }),
                },
                ManifestAction::RunEntry { entry } => {
                    let verdict = Verdict::from_equal(
                        run_single_check(entry.schema, entry.fixture, Some(entry.format), entry.mode, false, state.json, None) == 0,
                    );
                    queue.push_back(ManifestEvent::EntryDone { verdict });
                }
                ManifestAction::Print { text } => print_out(&text),
                ManifestAction::Error { message } => print_err(&message),
                ManifestAction::Exit { code: requested } => {
                    code = requested;
                    queue.clear();
                    break;
                }
            }
        }
    }
    code
}

/// `corpus` command: prepare all seed renders in the core, then write them
/// one at a time through the explicit corpus machine.
pub fn run_corpus(path: String, start: u64, end: u64, format: Option<Format>, out_dir: String) -> i32 {
    let mut state = CorpusRun::new();
    let mut queue: VecDeque<CorpusEvent> = VecDeque::new();
    queue.push_back(CorpusEvent::Start { path, start, end, format, out_dir });
    let mut code = 0;
    while let Some(event) = queue.pop_front() {
        for action in corpus::transition(&mut state, event) {
            match action {
                CorpusAction::Read { path } => match read_text(&path) {
                    Ok(text) => queue.push_back(CorpusEvent::Loaded { text }),
                    Err(message) => queue.push_back(CorpusEvent::Failed { message }),
                },
                CorpusAction::Parse { text } => match schema::parse(&text) {
                    Ok(parsed) => queue.push_back(CorpusEvent::Parsed { schema: parsed }),
                    Err(error) => queue.push_back(CorpusEvent::Failed { message: parse_failure(&error) }),
                },
                CorpusAction::Validate { schema: parsed } => match schema::validate(&parsed) {
                    Ok(()) => queue.push_back(CorpusEvent::Validated { schema: parsed }),
                    Err(error) => queue.push_back(CorpusEvent::Failed { message: parse_failure(&error) }),
                },
                CorpusAction::Prepare { schema: parsed, start, end, format } => {
                    match corpus::prepare(&parsed, start, end, format) {
                        Ok(files) => queue.push_back(CorpusEvent::Prepared { files }),
                        Err(error) => queue.push_back(CorpusEvent::Failed {
                            message: format!(
                                "cannot prepare corpus: {}\nWhy: a corpus is bounded to 1000 ordered seeds.\nFix: use a smaller A..B range.",
                                error
                            ),
                        }),
                    }
                }
                CorpusAction::Write { dir, name, text } => {
                    match write_corpus_file(&dir, &name, &text) {
                        Ok(()) => queue.push_back(CorpusEvent::Written),
                        Err(message) => queue.push_back(CorpusEvent::Failed { message }),
                    }
                }
                CorpusAction::Print { text } => print_out(&text),
                CorpusAction::Error { message } => print_err(&message),
                CorpusAction::Exit { code: requested } => {
                    code = requested;
                    queue.clear();
                    break;
                }
            }
        }
    }
    code
}

/// `validate` command: check a fixture's structure against a schema without regenerating.
pub fn run_validate(schema_path: String, fixture_path: String, json: bool) -> i32 {
    let loaded = read_text(&schema_path).and_then(|text| {
        let parsed = schema::parse(&text).map_err(|error| parse_failure(&error))?;
        schema::validate(&parsed).map_err(|error| parse_failure(&error))?;
        Ok(parsed)
    });
    let parsed_schema = match loaded {
        Ok(value) => value,
        Err(message) => {
            print_err(&message);
            return 2;
        }
    };
    let format = match Format::from_path(&fixture_path) {
        Ok(value) => value,
        Err(message) => {
            print_err(&format!("{}\nWhy: validation needs a format to parse the fixture.\nFix: use a .json, .jsonl or .csv fixture path.", message));
            return 2;
        }
    };
    let fixture = match read_text(&fixture_path) {
        Ok(text) => text,
        Err(message) => {
            print_err(&message);
            return 2;
        }
    };
    match kiln_core::validate::validate(&parsed_schema, &fixture, format) {
        Ok(issues) => {
            let text = if json {
                kiln_core::validate::render_json(format, &issues)
            } else {
                kiln_core::validate::render_text(format, &issues)
            };
            print_out(&text);
            if issues.is_empty() { 0 } else { 1 }
        }
        Err(error) => {
            print_err(&format!(
                "cannot parse fixture as {}: {}\nWhy: validation needs a readable fixture of the inferred format.\nFix: regenerate the fixture with kiln generate.",
                format.label(), error
            ));
            2
        }
    }
}

/// `export` command: emit a standard JSON Schema contract for a schema.
pub fn run_export(path: String) -> i32 {
    let parsed = read_text(&path).and_then(|text| {
        let schema = schema::parse(&text).map_err(|error| parse_failure(&error))?;
        schema::validate(&schema).map_err(|error| parse_failure(&error))?;
        Ok(schema)
    });
    match parsed {
        Ok(schema) => {
            print_out(&kiln_core::jsonschema::render(&schema));
            0
        }
        Err(message) => {
            print_err(&message);
            2
        }
    }
}

/// `fields` command: describe a schema without generating any data.
pub fn run_fields(path: String, json: bool) -> i32 {
    let parsed = read_text(&path).and_then(|text| {
        let schema = schema::parse(&text).map_err(|error| parse_failure(&error))?;
        schema::validate(&schema).map_err(|error| parse_failure(&error))?;
        Ok(schema)
    });
    match parsed {
        Ok(schema) => {
            let fields = kiln_core::introspect::describe(&schema);
            let text = if json {
                kiln_core::introspect::render_json(&schema, &fields)
            } else {
                kiln_core::introspect::render_text(&schema, &fields)
            };
            print_out(&text);
            0
        }
        Err(message) => {
            print_err(&message);
            2
        }
    }
}

fn demo_schema(seed: i64) -> String {
    let positive = seed.max(0) as u64;
    let mut text = String::from("version = 1\nseed = ");
    text.push_str(&positive.to_string());
    text.push_str("\ncount = 2\nformat = json\nrecord Demo {\n  id: int(0..1000)\n  name: enum(Alpha, Beta, Gamma)\n}\n");
    text
}

/// Headless scripted drive used by AIKit's compat gate. Its WORLD-DELTA lines
/// are derived from the real core, so they change whenever generation changes.
pub fn demo(seed: i64) -> i32 {
    let source = demo_schema(seed);
    let generated = schema::parse(&source)
        .and_then(|parsed| {
            schema::validate(&parsed)?;
            Ok(parsed)
        })
        .map(|parsed| generate::generate(&parsed));
    match generated {
        Ok(dataset) => {
            let rendered = render::render(&dataset, Format::Json);
            println!("SEED: {}", seed);
            println!("WORLD-DELTA: records=0->{}", rendered.records);
            println!("WORLD-DELTA: bytes=0->{}", rendered.bytes);
            println!("OK: scripted drive complete");
            0
        }
        Err(error) => {
            print_err(&parse_failure(&error));
            2
        }
    }
}

/// Machine-readable inventory of the command surface.
pub fn inventory() -> i32 {
    println!("cli|generate|kiln generate <schema.kiln> [--edges]");
    println!("cli|check|kiln check <schema.kiln> --fixture <file>");
    println!("cli|manifest|kiln check --manifest <kiln.toml>");
    println!("cli|diff|kiln diff <left> <right> [--format FMT]");
    println!("cli|corpus|kiln corpus <schema> --seeds A..B --out-dir DIR");
    println!("cli|fields|kiln fields <schema.kiln> [--json]");
    println!("cli|export|kiln export <schema.kiln>");
    println!("cli|validate|kiln validate <fixture> --schema <schema.kiln>");
    println!("cli|demo|--scripted [--seed N]");
    println!("handler|run|kiln-run");
    0
}

/// Human-readable architecture and schema reference.
pub fn explain() -> i32 {
    println!("Kiln: deterministic fixture generation in four parts");
    println!("  Functional core   - schema, rng, generate, render, compare and machine are pure");
    println!("  Imperative shell  - only shell.rs reads files, writes files and prints");
    println!("  I/O at the edges  - the core receives text and returns Action values");
    println!("  Explicit machine  - Idle > Loading > Parsing > Validating > Generating > Rendering > Done");
    println!("Modes: seeded (default, uses seed and count) or --edges (min/max boundaries, ignores seed and count)");
    println!("Schema keys: version, seed, count, format (json|jsonl|csv), record Name {{ field: type }}");
    println!("Field types: int(a..b), float(a..b) in hundredths, bool, enum(a,b), string(\"pre\",a..b), list(type,a..b)");
    println!("Drift: kiln check prints path-qualified changes when a committed fixture no longer matches");
    println!("Review: kiln diff compares two committed fixtures; kiln check --manifest gates them all");
    println!("Manifest lines: version = 1, then 'fixture = <schema>, <fixture>, <format>' per entry");
    println!("Corpus: kiln corpus <schema> --seeds A..B --out-dir DIR writes one fixture per seed");
    println!("Fields: kiln fields <schema> [--json] lists field names, types, details and bounds");
    println!("Export: kiln export <schema> emits a draft-2020-12 JSON Schema contract");
    println!("Validate: kiln validate <fixture> --schema <schema> [--json] checks types and bounds");
    0
}
