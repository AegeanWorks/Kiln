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
| Phase 4: every gate | `python aikit.py check kiln --all-gates` |
| Repair memory and map, then check | `python aikit.py fix kiln` |
| Resume an interrupted task | `python aikit.py resume kiln` |

## Why a fixture forge?

Test data that is written by hand drifts; test data that is generated randomly is not reproducible. Kiln makes fixtures a deterministic function of `(schema, seed)`: regenerate them anywhere, diff them in review, and let CI refuse a committed fixture that no longer matches its schema. It is local-first and dependency-free, so a build or a fixture refresh works with the network unplugged.

## Quickstart

```bash
cargo build
kiln generate examples/users.kiln                 # deterministic JSON to stdout
kiln generate examples/users.kiln --format csv    # same records as CSV
kiln generate examples/users.kiln --out users.json
kiln check examples/users.kiln --fixture users.json   # exit 0 on match, 1 on drift
kiln explain                                     # schema grammar and architecture
```

Cargo places the binary at `target/<target-triple>/debug/kiln.exe` on Windows. To run the committed example without installing, use `cargo run --quiet --bin kiln -- generate examples/users.kiln`.

## Schema format

A `.kiln` file is line oriented. `#` starts a comment. Four keys declare the run, then one `record` block declares the fields:

```text
version = 1
seed = 20261122
count = 4
format = json                 # json or csv

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
| `kiln generate <schema> [--format json\|csv] [--count N] [--out FILE]` | Generate and render fixtures |
| `kiln check <schema> --fixture <FILE>` | Regenerate and compare bytes; CRLF and trailing blank lines are normalized |
| `kiln demo --scripted [--seed N]` | Headless WORLD-DELTA drive used by AIKit's compat gate |
| `kiln inventory` | `kind\|name\|locator` lines describing the command surface |
| `kiln explain` | Architecture and schema reference |

Exit codes: `0` success or fixture match, `1` fixture mismatch, `2` argument, schema or I/O error. Errors name the cause (`Why:`) and the remedy (`Fix:`) instead of a bare failure.

## Determinism contract

1. One xorshift64* stream seeded by `seed`; no OS randomness and no clock.
2. Field order is declaration order; JSON keys and CSV columns follow it exactly.
3. Decimals are integer hundredths, so generation never accumulates floating-point error.
4. Generation never iterates a hash map. Golden byte tests in `tests/core.rs` pin both renderers.

Changing the generator is a deliberate golden update, which is the review this kind of tool should force.

## Architecture: constraints, not style

| Constraint | Implementation |
|---|---|
| Functional Core | `src/schema.rs`, `rng.rs`, `generate.rs`, `render.rs`, `compare.rs`, `machine.rs`, `cli.rs` and `model.rs` are pure: data in, data out. They contain no `std::fs`, `std::env`, clock, OS randomness or process exit. |
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
│   ├── lib.rs                <- exports the eight pure core modules
│   ├── model.rs              <- frozen data types
│   ├── schema.rs             <- versioned schema reader and validator
│   ├── rng.rs                <- fixed xorshift64 stream
│   ├── generate.rs           <- seeded dataset generation
│   ├── render.rs             <- JSON and CSV renderers
│   ├── compare.rs            <- fixture byte comparison
│   ├── machine.rs            <- explicit generation state machine
│   └── cli.rs                <- pure argument parser
├── tests/
│   ├── core.rs               <- golden bytes, PRNG, validation, state machine, CLI parser
│   └── cli.rs                <- binary contracts: inventory, replay, gate exit codes
├── docs/                     <- local documentation
│   ├── manual.html           <- self-contained local command reference
│   └── architecture-audit.md <- FC / shell / edges / state-machine audit evidence
├── examples/
│   ├── users.kiln            <- example schema
│   ├── users.json            <- golden JSON fixture
│   └── users.csv             <- golden CSV fixture
└── .aikit/                   <- this project's memory: record, ledger, decisions
```

## Verification

`cargo test` runs 21 tests: 13 pure-core tests and 8 binary integration tests. The AIKit Phase 4 gate additionally runs hygiene, guidelines, secret, compat and test checks; `python aikit.py check kiln --all-gates` is the canonical full report.

## 📜 License

Free for personal use under the Aegean License 1.1 (AL-1.1); see [LICENSE](LICENSE). Any commercial use needs the Aegean Commercial License 1.0 (ACL-1.0); see [COMMERCIAL.md](COMMERCIAL.md).
