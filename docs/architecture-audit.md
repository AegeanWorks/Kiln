# Architecture audit — Kiln (2026-11-22)

Audit target: the committed design of Kiln against the three architectural constraints. Method: source
inspection plus a marker scan, a transition-table review, and the test suite. All commands were run
from the project root.

## Constraint 1 — Functional Core

`src/lib.rs` exports sixteen modules; all are pure transformations of data:

| Module | Input → output | Effect-free claim |
|---|---|---|
| `model.rs` | types only | no functions with side effects |
| `schema.rs` | `&str` → `Result<Schema, String>` | no file handle; validation is pure |
| `rng.rs` | seed → `Rng` → integers | fixed xorshift64*, no OS randomness or clock |
| `generate.rs` | `&Schema` → `Dataset` | single seeded stream, declaration-order fields |
| `render.rs` | `&Dataset` → `Rendered` | string building only |
| `compare.rs` | two `&str` → `Verdict` | line-ending normalization only |
| `json.rs` | `&str` → `Json` | order-preserving reader for Kiln's own output |
| `drift.rs` | two `&str` + `Format` → report text | recursive structural diff, no I/O |
| `manifest.rs` | text → `Manifest`; `ManifestRun` + event → actions | versioned reader and a second explicit state machine |
| `report.rs` | verdicts/changes → JSON text | stable key order, no serialization dependency |
| `corpus.rs` | schema + seed range → prepared files; `CorpusRun` + event → actions | bounded preparation and a third explicit machine |
| `introspect.rs` | `&Schema` → field descriptors → text/JSON | one-shot, no state to model |
| `jsonschema.rs` | `&Schema` → draft-2020-12 JSON Schema | one-shot, types and bounds mapped deterministically |
| `validate.rs` | `&Schema` + fixture text + format → issues | reuses the JSON/CSV readers; one-shot, no I/O |
| `machine.rs` | `&mut RunState` + `Event` → `Vec<Action>` | returns commands; executes none |
| `cli.rs` | `&[String]` → `Result<Command, String>` | no `std::env` read |

Evidence command:

```
grep -rn "std::fs|std::env|println!|eprintln!|print!|SystemTime|Instant|process::exit|ExitCode|fs::" src
```

Result: 40 matches, **all in `src/main.rs` and `src/shell.rs`**; zero matches in the sixteen core
modules. The core cannot read a file, read argv, write output or exit a process.

## Constraint 2 — Imperative Shell / I/O at the Edges

The shell is deliberately small:

- `src/main.rs` reads argv at line 11 (`std::env::args`), dispatches the parsed `Command`, converts the
  shell's integer result to `ExitCode` at line 29. It performs no analysis.
- `src/shell.rs` owns every side effect: `print!`/`eprintln!` (lines 16, 21), `fs::read_to_string`
  (line 34), `fs::write` (lines 43, 60), `fs::create_dir_all` (line 53), and the human/report output
  (lines 436–480). Each I/O failure is
  diagnosed with `Why:` and `Fix:`.

The shell/core boundary is two frozen DTOs: schema text enters through `Event::Loaded`, and a
`Rendered` value leaves `render::render`. The shell never reaches into core internals; it only feeds
events back from the actions the machine emitted (`Read`, `Parse`, `Validate`, `Generate`, `Render`,
`Compare`, `Write`, `Print`, `Error`, `Exit`).

Audit finding and fix: the first implementation printed generated text to stdout even when `--out`
was given. The audit moved the `Print` action into the no-output branch of `Rendered`; the regression
is covered by `cli.rs::out_writes_a_file_and_keeps_stdout_clean`.

## Constraint 3 — Explicit State Machines

`machine.rs` declares `Phase` with nine explicit states and an `Event`/`Action` pair for every step:

| Phase | Accepting event | Emitted action(s) | Next phase |
|---|---|---|---|
| Idle | Start | Read | Loading |
| Loading | Loaded | Parse | Parsing |
| Parsing | Parsed | Validate | Validating |
| Validating | Validated | Generate(mode) | Generating |
| Generating | Generated | Render | Rendering |
| Rendering | Rendered | Write + Print, or Compare (drift report on mismatch) | Done / Comparing |
| Comparing | Compared | Print verdict + Exit(0/1) | Done |
| any | Failed | Error + Exit(2) | Failed |
| any other | — | Error + Exit(2) | unchanged |

An event received in the wrong phase never silently no-ops: the catch-all arm emits a diagnosis and
exit code 2. `tests/core.rs::machine_refuses_an_event_in_the_wrong_phase` pins that behaviour, and
`machine_check_mode_returns_a_nonzero_code_on_mismatch` pins the comparison branch.

## v0.3 audit: a second explicit state machine

The multi-fixture gate does not hide its loop in the shell. `manifest.rs` declares
`ManifestPhase { Idle, Reading, Parsing, Running { index }, Done, Failed }` and a matching
`ManifestEvent`/`ManifestAction` pair. `Running { index }` carries the position explicitly; each
`EntryDone` event either advances to `index + 1` or emits the aggregate summary and an exit code. An
unknown phase/event pair produces an error action, so no entry is silently skipped. Per-entry work is
delegated to the original single-fixture machine, so both layers remain enumerable.

The `diff` command is a one-shot pure transformation (read two texts, call the pure differ, print),
so it has no stateful behaviour to model; it still performs all I/O in the shell.

The v0.6 corpus command added a third machine: `CorpusRun` moves through
`Idle → Reading → Parsing → Validating → Preparing → Writing{index} → Done`, emitting exactly one
`Write` action per file. All renders are prepared in the pure core before any file is opened, so the
shell never decides which file comes next.

## Determinism evidence

```
cargo test                     # 66 tests pass
cargo run --quiet --bin kiln -- generate examples/users.kiln   # byte-identical on repeat runs
cargo run --quiet --bin kiln -- check examples/users.kiln --fixture examples/users.json
→ fixture MATCH ... verdict: PASS (exit 0)
```

Golden byte tests pin JSON and CSV output (`tests/core.rs::generation_matches_golden_*_bytes`), and the
PRNG test proves seed sensitivity and repeatability.

## Dogfooding through AIKit

`.aikit/project.json` declares a custom `fixtures` gate that runs
`kiln check --manifest examples/kiln.toml`. AIKit's Phase 4 therefore verifies every committed
fixture with the same tool the project ships; `check kiln --all-gates` reported
`6/6 gates passed`. The gate is ordinary project configuration, not an override of a built-in scan,
so hygiene, guidelines, secrets, compat and test still run unchanged.

## Verdict

All three constraints hold in the implementation, with one stdout/edge defect found and fixed during
the audit. The v0.2 additions (boundary generation, JSON reader, drift differ, JSONL renderer) were
added inside the core: a marker scan after the change still found zero I/O calls outside the shell,
and the new generation mode travels through `Event::Start`/`Action::Generate` rather than through a
side channel. The architecture is enforced by the test suite rather than by convention: a future change
that adds I/O to the core would be visible in the marker scan, and a change that breaks the phase
order or the golden bytes fails `cargo test`.
