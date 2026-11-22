# Architecture audit — Kiln (2026-11-22)

Audit target: the committed design of Kiln against the three architectural constraints. Method: source
inspection plus a marker scan, a transition-table review, and the test suite. All commands were run
from the project root.

## Constraint 1 — Functional Core

`src/lib.rs` exports ten modules; all are pure transformations of data:

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
| `machine.rs` | `&mut RunState` + `Event` → `Vec<Action>` | returns commands; executes none |
| `cli.rs` | `&[String]` → `Result<Command, String>` | no `std::env` read |

Evidence command:

```
grep -rn "std::fs|std::env|println!|eprintln!|print!|SystemTime|Instant|process::exit|ExitCode|fs::" src
```

Result: 26 matches, **all in `src/main.rs` and `src/shell.rs`**; zero matches in the ten core
modules. The core cannot read a file, read argv, write output or exit a process.

## Constraint 2 — Imperative Shell / I/O at the Edges

The shell is deliberately small:

- `src/main.rs` reads argv at line 11 (`std::env::args`), dispatches the parsed `Command`, converts the
  shell's integer result to `ExitCode` at line 29. It performs no analysis.
- `src/shell.rs` owns every side effect: `print!`/`eprintln!` (lines 13, 18), `fs::read_to_string`
  (line 22), `fs::write` (line 31), and the human/report output (lines 163–195). Each I/O failure is
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

## Determinism evidence

```
cargo test                     # 29 tests pass
cargo run --quiet --bin kiln -- generate examples/users.kiln   # byte-identical on repeat runs
cargo run --quiet --bin kiln -- check examples/users.kiln --fixture examples/users.json
→ fixture MATCH ... verdict: PASS (exit 0)
```

Golden byte tests pin JSON and CSV output (`tests/core.rs::generation_matches_golden_*_bytes`), and the
PRNG test proves seed sensitivity and repeatability.

## Verdict

All three constraints hold in the implementation, with one stdout/edge defect found and fixed during
the audit. The v0.2 additions (boundary generation, JSON reader, drift differ, JSONL renderer) were
added inside the core: a marker scan after the change still found zero I/O calls outside the shell,
and the new generation mode travels through `Event::Start`/`Action::Generate` rather than through a
side channel. The architecture is enforced by the test suite rather than by convention: a future change
that adds I/O to the core would be visible in the marker scan, and a change that breaks the phase
order or the golden bytes fails `cargo test`.
