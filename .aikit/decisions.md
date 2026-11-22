# Decisions

Why this project is built the way it is, oldest first. One entry per decision that changes a
contract, a dependency, a data format or the architecture; IDs are consecutive and never reused.

<!-- Entry format:
### ADR-NNN: <decision in one line>
- **Date**: YYYY-MM-DD
- **Status**: Accepted
- **Context**: <what forced the decision>
- **Decision**: <what was decided>
- **Consequences**: <what follows, including what it costs>
-->

### ADR-001: Implement Kiln in Rust with zero external crates
- **Date**: 2026-11-22
- **Status**: Accepted
- **Context**: Directive requires C++ or Rust only and the Rust 1.100 toolchain is present; offline reproducibility is a core promise
- **Decision**: Use Rust 2021 with std only; no crates.io dependencies and no network at build or run time
- **Consequences**: Reproducible offline builds; schema parser, PRNG and JSON/CSV renderers are hand-written and golden-tested

### ADR-002: Functional core, imperative shell, edges, explicit state machines
- **Date**: 2026-11-22
- **Status**: Accepted
- **Context**: The architectural directive makes these constraints, not style choices
- **Decision**: kiln_core is pure (model, schema, rng, generate, render, machine, cli args); kiln's binary owns all fs/env/stdout I/O; a GenerateState machine emits Action commands the shell interprets
- **Consequences**: Core is testable without I/O or a filesystem; every generation phase is enumerable and invalid transitions are impossible

### ADR-003: Pin determinism with a xorshift64 stream and golden tests
- **Date**: 2026-11-22
- **Status**: Accepted
- **Context**: Fixtures are useless for regression tests if a library upgrade or platform change alters their bytes
- **Decision**: Use a fixed xorshift64 stream seeded by the schema, preserve declared field order, avoid any clock/locale/hash-map iteration in generation, and golden-test output bytes
- **Consequences**: Same schema and seed always yield identical JSON/CSV on every platform; changes to generation require deliberate golden updates

### ADR-004: Replace AIKit's Python compat scaffold with Rust drivers
- **Date**: 2026-11-22
- **Status**: Accepted
- **Context**: python aikit.py new created demo.py and inventory.py, but the project must be Rust-only; compat commands are configurable in .aikit/compat.json
- **Decision**: Delete the Python files and point compat commands at the kiln binary's demo and inventory subcommands
- **Consequences**: Rust-only tree; the compat gate still proves inventory format and seeded replay determinism; the AIKit gap is recorded in reports

### ADR-005: Add deterministic boundary-value generation mode
- **Date**: 2026-11-22
- **Status**: Accepted
- **Context**: Seeded random fixtures rarely hit min/max values; programmers still hand-write boundary tests
- **Decision**: --edges generates all-low, all-high and one-field-at-a-time-high records from schema bounds without using the seeded stream
- **Consequences**: Edge mode is independent of seed and count; golden tests pin its exact record order

### ADR-006: Report structural fixture drift instead of a bare mismatch
- **Date**: 2026-11-22
- **Status**: Accepted
- **Context**: kiln check said only DIFFER, leaving the programmer to find what changed in a large fixture
- **Decision**: Parse both JSON or CSV documents into order-preserving generic trees and emit a deterministic path-qualified change list on mismatch
- **Consequences**: A small restricted parser is added to the pure core; diff output is golden-tested and never affects the MATCH verdict

### ADR-007: Add JSONL as a third output format
- **Date**: 2026-11-22
- **Status**: Accepted
- **Context**: Streaming test pipelines and log-style fixtures want one JSON object per line
- **Decision**: Add Format::Jsonl with a pure line-oriented renderer that preserves declaration-order fields
- **Consequences**: Schema format key, CLI flag, docs and golden fixtures are extended together

### ADR-008: Add a structural diff command for committed fixtures
- **Date**: 2026-11-22
- **Status**: Accepted
- **Context**: Reviewers need to compare two fixture revisions without regenerating or running the generator
- **Decision**: kiln diff reads two fixture files, infers format from extension unless overridden, and prints the existing pure drift report with exit 0 for identical bytes and 1 for differences
- **Consequences**: The drift differ becomes reusable outside check; no new state machine is needed for a one-shot transformation

### ADR-009: Gate multiple fixtures with a versioned manifest and an explicit machine
- **Date**: 2026-11-22
- **Status**: Accepted
- **Context**: Projects keep several schema/fixture pairs, and a CI gate must cover all of them deterministically
- **Decision**: A kiln.toml manifest lists fixture entries; ManifestRun moves Idle > Reading > Parsing > Running(index) > Done with Failed on any error, and delegates each entry to the existing single-fixture machine
- **Consequences**: Aggregate verdict is explicit and ordered; a new parser and a second state machine are added to the pure core

### ADR-010: Run Kiln's manifest gate inside AIKit Phase 4
- **Date**: 2026-11-22
- **Status**: Accepted
- **Context**: A project that sells fixture determinism should have its own fixtures gated by the same tool
- **Decision**: Declare a custom 'fixtures' gate in .aikit/project.json that runs kiln check --manifest examples/kiln.toml
- **Consequences**: Every AIKit check of Kiln now proves all committed fixtures; the gate is visible alongside hygiene, test and compat

### ADR-011: Support seed overrides and stdin schemas at the edge
- **Date**: 2026-11-22
- **Status**: Accepted
- **Context**: Scripted pipelines need to sweep seeds and pipe a schema without writing temporary files
- **Decision**: Add --seed to generate and accept '-' as a schema path, read by the shell from stdin; the seed override travels through Event::Start into the pure override step
- **Consequences**: The core stays pure; stdin is one more edge adapter in shell.rs with the same error diagnostics

### ADR-012: Add a corpus command with an explicit write state machine
- **Date**: 2026-11-22
- **Status**: Accepted
- **Context**: Programmers need many deterministic fixtures for property tests, and a silent shell loop would hide partial failure
- **Decision**: Prepare all seed renders in the pure core, then let CorpusRun emit one Write action per file in seed order with an explicit index
- **Consequences**: The corpus is bounded, ordered and reviewable; writes remain at the shell edge with diagnosed errors

### ADR-013: Add schema field introspection
- **Date**: 2026-11-22
- **Status**: Accepted
- **Context**: Documentation and CI need the declared field types and bounds without parsing a .kiln file themselves
- **Decision**: kiln fields reads and validates a schema, then renders a stable text table or JSON array from a pure introspection module
- **Consequences**: A one-shot pure command; no extra state machine is warranted
