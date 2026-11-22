//! The executable. It reads argv from the environment (the outer edge),
//! dispatches to the shell and converts the shell's integer code to an exit.

mod shell;

use std::process::ExitCode;

use kiln_core::cli::{self, Command};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match cli::parse(&args) {
        Ok(Command::Help) => {
            shell::print_out(&cli::usage());
            0
        }
        Ok(Command::Explain) => shell::explain(),
        Ok(Command::Inventory) => shell::inventory(),
        Ok(Command::Demo { seed }) => shell::demo(seed),
        Ok(Command::Generate { schema, format, count, out, edges, seed }) => {
            shell::run_generate(schema, format, count, out, edges, seed)
        }
        Ok(Command::Check { schema, fixture, json }) => shell::run_check(schema, fixture, json),
        Ok(Command::Diff { left, right, format, json }) => shell::run_diff(left, right, format, json),
        Ok(Command::Manifest { path, json }) => shell::run_manifest(path, json),
        Ok(Command::Corpus { schema, start, end, format, out_dir }) => {
            shell::run_corpus(schema, start, end, format, out_dir)
        }
        Ok(Command::Fields { schema, json }) => shell::run_fields(schema, json),
        Ok(Command::Export { schema }) => shell::run_export(schema),
        Err(message) => {
            shell::print_err(&message);
            2
        }
    };
    ExitCode::from(code as u8)
}
