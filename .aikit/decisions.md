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
