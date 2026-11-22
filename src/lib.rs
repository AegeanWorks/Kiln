//! Pure functional core of Kiln.
//!
//! No module here reads a file, reads the environment, consults a clock, uses
//! OS randomness or exits a process. The imperative shell lives in `main.rs`
//! and `shell.rs`: it supplies text, interprets the `Action` values emitted by
//! the state machine and performs every side effect at the edge.

pub mod cli;
pub mod compare;
pub mod corpus;
pub mod drift;
pub mod generate;
pub mod introspect;
pub mod jsonschema;
pub mod json;
pub mod machine;
pub mod manifest;
pub mod model;
pub mod render;
pub mod report;
pub mod rng;
pub mod schema;
