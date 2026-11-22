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
        Ok(Command::Generate { schema, format, count, out, edges }) => {
            shell::run_generate(schema, format, count, out, edges)
        }
        Ok(Command::Check { schema, fixture }) => shell::run_check(schema, fixture),
        Err(message) => {
            shell::print_err(&message);
            2
        }
    };
    ExitCode::from(code as u8)
}
