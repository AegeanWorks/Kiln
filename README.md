# Kiln

[![Built with AIKit](https://img.shields.io/badge/built%20with-AIKit-6f42c1)](https://github.com/AegeanWorks/AIKit)

Kiln is a zero-dependency, deterministic Rust CLI that forges reproducible JSON and CSV test fixtures from a tiny typed schema and a seed, so programmers get stable, reviewable test data offline.

## ⚡ 30-Second Cold-Start Quick Reference

- **Runtime Prerequisites**: Rust 1.70+ (edition 2021; verified on 1.100); a C linker for the host target (mingw-w64 gcc on Windows, configured in `.cargo/config.toml`). No crates.io dependencies, no network, no runtime service.
- **Canonical Regression Shield Gate**:
  ```bash
  cargo test
  ```

### 🛠️ AIKit Commands (run from the AIKit folder)
| Role | Command |
|---|---|
| Status, task and gates | `python aikit.py status kiln` |
| Phase 4: every gate (incl. Kiln's own fixtures) | `python aikit.py check kiln --all-gates` |
| Repair memory and map, then check | `python aikit.py fix kiln` |
| Resume an interrupted task | `python aikit.py resume kiln` |

## Why a fixture forge?

Test data that is written by hand drifts; test data that is generated randomly is not reproducible. Kiln makes fixtures a deterministic function of `(schema, seed)`: regenerate them anywhere, diff them in review, and let CI refuse a committed fixture that no longer matches its schema. It is local-first and dependency-free, so a build or a fixture refresh works with the network unplugged.

## Quickstart

```bash
cargo build
kiln generate examples/users.kiln                 # deterministic JSON to stdout
kiln generate examples/users.kiln --format csv    # same records as CSV
kiln generate examples/users.kiln --format jsonl  # one JSON object per line
kiln generate examples/users.kiln --edges         # min/max boundary records
kiln generate examples/users.kiln --out users.json
kiln check examples/users.kiln --fixture users.json   # exit 0 on match, 1 on drift
kiln check --manifest examples/kiln.toml          # gate every committed fixture at once
kiln diff examples/users.json examples/users-edges.json  # structural review of two fixtures
kiln explain                                     # schema grammar and architecture
```

Cargo places the binary at `target/<target-triple>/debug/kiln.exe` on Windows. To run the committed example without installing, use `cargo run --quiet --bin kiln -- generate examples/users.kiln`.

## Schema format

A `.kiln` file is line oriented. `#` starts a comment. Four keys declare the run, then one `record` block declares the fields:

```text
version = 1
seed = 20261122
count = 4
format = json                 # json, jsonl or csv

record User {
  id: int(1..9999)
  name: enum(Alice, Bob, Cara, Deniz)
  active: bool
  score: float(0..10000)      # hundredths: 10000 renders as 100.00
  handle: string("u_", 3..6)  # optional prefix, inclusive length range
  tags: list(enum(red, green, blue), 0..3)
}
```

| Field type | Syntax | Notes |
|---|---|---|
| Integer | `int(min..max)` | Inclusive; range at most 1,000,000 wide |
| Decimal | `float(min..max)` | Non-negative hundredths, rendered with two decimals |
| Boolean | `bool` | One bit from the seeded stream |
| Enumeration | `enum(a, b, c)` | 1–32 variants |
| String | `string("prefix", min..max)` | Prefix optional; generated length ≤ 64 |
| List | `list(type, min..max)` | Nested type; length ≤ 16 |

Validation refuses an unknown schema version, an inverted or oversized range, duplicate fields, an empty record, or a count outside 1–10,000. An unknown version is refused without modifying or quarantining the file.

## Commands and exit codes

| Command | Effect |
|---|---|
| `kiln generate <schema> [--format json\|jsonl\|csv] [--count N] [--out FILE] [--edges] [--seed N]` | Generate and render fixtures; `--edges` emits boundary records and ignores `seed`/`count`; `--seed` overrides the schema seed. A schema path of `-` reads stdin |
| `kiln check <schema> --fixture <FILE> [--seed N]` | Regenerate and compare bytes; `--seed` matches a fixture made by `generate --seed`; on mismatch print path-qualified drift; CRLF and trailing blank lines are normalized |
| `kiln check --manifest <kiln.toml>` | Verify every declared fixture in order; exit 0 if all match, 1 if any differs, 2 on a manifest error |
| `kiln diff <left> <right> [--format FMT]` | Structurally compare two committed fixtures without regenerating; format is inferred from the left extension |
| `kiln corpus <schema> --seeds A..B --out-dir DIR [--format FMT]` | Write one deterministic fixture per seed (≤1000), named `<record>_<seed>.<ext>` |
| `kiln fields <schema> [--json]` | List field names, types, details and bounds in declaration order |
| `kiln export <schema>` | Emit a draft-2020-12 JSON Schema contract for the record |
| `kiln validate <fixture> --schema <schema> [--json]` | Check types and bounds of a hand-edited fixture without regenerating (exit 0 clean, 1 issues, 2 parse error) |
| `kiln demo --scripted [--seed N]` | Headless WORLD-DELTA drive used by AIKit's compat gate |
| `kiln inventory` | `kind\|name\|locator` lines describing the command surface |
| `kiln explain` | Architecture and schema reference |

`check`, `diff` and `check --manifest` accept `--json` and print one stable, versioned JSON object
(`"schema":1`) with the verdict and, where relevant, the ordered change list. Exit codes are unchanged.

Exit codes: `0` success or fixture match, `1` fixture mismatch, `2` argument, schema or I/O error. Errors name the cause (`Why:`) and the remedy (`Fix:`) instead of a bare failure.

## Boundary fixtures and drift reports

`--edges` switches from seeded random records to deterministic boundary records: one all-minimum row, one all-maximum row, then one row per field where only that field is raised to its maximum. It is independent of `seed` and `count`, so the same schema always produces the same edge suite.

`kiln check` still decides match by bytes, but a mismatch now prints a bounded, path-qualified report: for JSON/JSONL, changes are located as `$.rows[0].id` or `lines[2]`; for CSV, as `rows[0].id`. Added fields get `+`, removed fields `-`, changed values `~`. The report is produced by a pure order-preserving reader and differ in the core, so it cannot change the verdict.

## Manifest and review workflow

A `kiln.toml` manifest lets one CI command gate the whole fixture set:

```text
version = 1
fixture = examples/users.kiln, examples/users.json, json
fixture = examples/users.kiln, examples/users.jsonl, jsonl
fixture = examples/users.kiln, examples/users-edges.json, json, edges
```

The optional fourth value selects `seeded` (default) or `edges` generation. The manifest is versioned;
an unknown version is refused without changing the file. Its own state machine walks entries in
declared order and prints an aggregate `3/3 fixtures matched` summary.

`kiln diff` reuses the same pure differ on two committed fixtures, so a reviewer can see what a
fixture change means without running the generator. It infers the format from the left file's
extension unless `--format` is given.

## Determinism contract

1. One xorshift64* stream seeded by `seed`; no OS randomness and no clock.
2. Field order is declaration order; JSON keys, JSONL objects and CSV columns follow it exactly.
3. Decimals are integer hundredths, so generation never accumulates floating-point error.
4. Generation never iterates a hash map. Golden byte tests in `tests/core.rs` pin all three renderers.

Changing the generator is a deliberate golden update, which is the review this kind of tool should force.

## Architecture: constraints, not style

| Constraint | Implementation |
|---|---|
| Functional Core | `src/schema.rs`, `rng.rs`, `generate.rs`, `render.rs`, `compare.rs`, `json.rs`, `jsonschema.rs`, `drift.rs`, `validate.rs`, `report.rs`, `manifest.rs`, `corpus.rs`, `introspect.rs`, `machine.rs`, `cli.rs` and `model.rs` are pure: data in, data out. They contain no `std::fs`, `std::env`, clock, OS randomness or process exit. |
| Imperative Shell | `src/shell.rs` owns every file read/write and every print. `src/main.rs` reads argv and converts the shell's integer code to `ExitCode`. |
| I/O at the Edges | The boundary is two frozen DTOs: schema text enters, a `Rendered` value leaves. Side effects are represented as `Action` values emitted by the pure machine and executed only in the shell. |
| Explicit State Machines | `GenerateState` moves through `Idle → Loading → Parsing → Validating → Generating → Rendering → Done`, with `Comparing` for the fixture gate and `Failed` for any diagnosed error. Each event has one transition; an event in the wrong phase yields an error action, never a silent no-op. |

This lets the whole generation pipeline be tested without a filesystem, while the shell stays small enough to read in one sitting.

## 📦 What's Inside

```text
kiln/
├── .cargo/                   <- Cargo build configuration
│   └── config.toml           <- GNU target (this host has no MSVC link.exe)
├── .gitattributes            <- LF line endings on every platform
├── .gitignore                <- transient AIKit state excluded
├── LICENSE                   <- Aegean License 1.1 (AL-1.1): free personal use
├── COMMERCIAL.md             <- Commercial licensing terms
├── LICENSES/                 <- REUSE license texts
├── REUSE.toml                <- Machine-readable SPDX metadata
├── Cargo.toml                <- no external crates
├── Cargo.lock                <- pinned lockfile (only this crate)
├── README.md
├── src/                      <- pure core plus thin executable edges
│   ├── main.rs               <- executable edge: argv in, ExitCode out
│   ├── shell.rs              <- imperative shell: all fs and stdout/stderr I/O
│   ├── lib.rs                <- exports the sixteen pure core modules
│   ├── model.rs              <- frozen data types and generation mode
│   ├── schema.rs             <- versioned schema reader and validator
│   ├── rng.rs                <- fixed xorshift64 stream
│   ├── generate.rs           <- seeded and boundary-value generation
│   ├── render.rs             <- JSON, JSONL and CSV renderers
│   ├── compare.rs            <- fixture byte comparison
│   ├── json.rs               <- order-preserving JSON reader for drift
│   ├── drift.rs              <- path-qualified structural drift report
│   ├── manifest.rs           <- versioned manifest and its run state machine
│   ├── report.rs             <- versioned JSON reports for CI
│   ├── corpus.rs             <- bounded seed corpus and its write machine
│   ├── introspect.rs         <- schema field descriptions for docs and CI
│   ├── jsonschema.rs         <- draft-2020-12 contract export
│   ├── validate.rs           <- fixture type and bound validation
│   ├── machine.rs            <- explicit generation state machine
│   └── cli.rs                <- pure argument parser
├── tests/
│   ├── core.rs               <- golden bytes, PRNG, validation, state machine, CLI parser
│   └── cli.rs                <- binary contracts: inventory, replay, gate exit codes
├── docs/                     <- local documentation
│   ├── manual.html           <- self-contained local command reference
│   └── architecture-audit.md <- FC / shell / edges / state-machine audit evidence
├── examples/
│   ├── kiln.toml             <- example multi-fixture manifest
│   ├── users.kiln            <- example schema
│   ├── users.json            <- golden JSON fixture
│   ├── users.jsonl           <- golden JSONL fixture
│   ├── users-edges.json      <- golden boundary-value fixture
│   └── users.csv             <- golden CSV fixture
└── .aikit/                   <- this project's memory: record, ledger, decisions
```

## Verification

`cargo test` runs 64 tests: 36 pure-core tests and 28 binary integration tests. The AIKit Phase 4 gate runs hygiene, guidelines, secrets, compat, test and a project-declared `fixtures` gate that verifies every committed fixture with `kiln check --manifest`; `python aikit.py check kiln --all-gates` reported `6/6 gates passed`.

## 📜 License

Free for personal use under the Aegean License 1.1 (AL-1.1); see [LICENSE](LICENSE). Any commercial use needs the Aegean Commercial License 1.0 (ACL-1.0); see [COMMERCIAL.md](COMMERCIAL.md).
