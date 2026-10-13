//! The `check-json5` binary: the 1.x command name, kept until 3.0 for scripts that call it.

fn main() -> std::process::ExitCode {
    jsonvet_cli::run()
}
