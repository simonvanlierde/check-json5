//! The jsonvet command line, shared by the `jsonvet` and `check-json5` binaries.
//! A stub until the parser lands: it only proves the packaging.

use std::process::ExitCode;

/// Run the command with the process's arguments and return its exit code.
pub fn run() -> ExitCode {
    let files: Vec<String> = std::env::args().skip(1).collect();
    println!(
        "jsonvet {} (stub): {} file(s)",
        jsonvet::VERSION,
        files.len()
    );
    ExitCode::SUCCESS
}
