//! Imperative shell: the only layer that performs I/O. Every action it runs
//! comes from the pure state machine; every result goes back as an event.

use std::collections::VecDeque;
use std::fs;

use kiln_core::machine::{self, Action, Event, RunState};
use kiln_core::model::{Format, GenerationMode, Verdict};
use kiln_core::{compare, drift, generate, render, schema};

/// Write to stdout. Kept here so the core never touches a stream.
pub fn print_out(text: &str) {
    print!("{}", text);
}

/// Write a diagnosed error to stderr.
pub fn print_err(message: &str) {
    eprintln!("{}", message);
}

fn read_text(path: &str) -> Result<String, String> {
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

fn parse_failure(error: &str) -> String {
    format!(
        "cannot use schema: {}\nWhy: generation is deterministic only for a valid version-1 schema.\nFix: kiln explain describes the schema grammar.",
        error
    )
}

/// Drive the state machine until it requests an exit. Returns its exit code.
fn run_machine(start: Event) -> i32 {
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
                    if verdict == Verdict::Fail {
                        let format = state.format.unwrap_or(Format::Json);
                        print_out(&drift::drift_report(&expected, &actual, format));
                    }
                    queue.push_back(Event::Compared { verdict });
                }
                Action::Write { path, text } => {
                    if let Err(message) = write_text(&path, &text) {
                        queue.push_back(Event::Failed { message });
                    }
                }
                Action::Print { text } => print_out(&text),
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
) -> i32 {
    run_machine(Event::Start {
        path,
        format,
        count,
        out,
        fixture_text: String::new(),
        check: false,
        mode: if edges { GenerationMode::Edges } else { GenerationMode::Seeded },
    })
}

/// `check` command: read the committed fixture at the edge, then compare.
pub fn run_check(path: String, fixture: String) -> i32 {
    match read_text(&fixture) {
        Ok(text) => run_machine(Event::Start {
            path,
            format: None,
            count: None,
            out: None,
            fixture_text: text,
            check: true,
            mode: GenerationMode::Seeded,
        }),
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
    0
}
