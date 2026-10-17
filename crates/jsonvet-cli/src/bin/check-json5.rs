//! The `check-json5` binary: the 1.x command name, kept through 3.x for scripts that call it.

fn main() -> std::process::ExitCode {
    jsonvet_cli::run()
}
