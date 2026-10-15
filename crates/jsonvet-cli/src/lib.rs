//! The jsonvet command line, shared by the `jsonvet` and `check-json5` binaries.

use std::ffi::{OsStr, OsString};
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::process::ExitCode;

use jsonvet::Severity;

mod args;
mod glob;
mod output;
mod sarif;
mod schema;
mod settings;

use args::{Command, ConfigSource};
use output::Printer;
use schema::Schema;
use settings::Settings;

/// Run the command with the process's arguments and environment, and return its exit code.
pub fn run() -> ExitCode {
    let annotate = std::env::var_os("GITHUB_ACTIONS").is_some_and(|v| v == "true");
    let stdout = io::stdout().lock();
    match run_with(
        std::env::args_os().skip(1),
        annotate,
        BufWriter::new(stdout),
    ) {
        Ok(code) => ExitCode::from(code),
        Err(message) => {
            eprintln!("jsonvet: {message}");
            ExitCode::from(2)
        }
    }
}

/// The command, with its inputs passed in: 0 when no file has an error, 1 when one does.
/// A usage error comes back as `Err`, which `run` turns into exit status 2.
fn run_with(
    args: impl IntoIterator<Item = OsString>,
    annotate: bool,
    out: impl Write,
) -> Result<u8, String> {
    let args =
        match args::parse(args).map_err(|e| format!("{e}\n\nRun `jsonvet --help` for usage."))? {
            Command::Help => return print(out, args::HELP),
            Command::Version => return print(out, concat!("jsonvet ", env!("CARGO_PKG_VERSION"))),
            Command::Check(args) => args,
        };
    let from_file = match &args.config {
        ConfigSource::None => Settings::default(),
        ConfigSource::File(path) => settings::load(path)?,
        ConfigSource::Search => {
            let cwd = std::env::current_dir()
                .map_err(|e| format!("cannot read the working directory: {e}"))?;
            match settings::discover(&cwd) {
                Some(path) => settings::load(&path)?,
                None => Settings::default(),
            }
        }
    };
    let settings = from_file.overridden_by(args.settings);
    let schema = match &args.schema {
        Some(path) => Some(Schema::load(path, &settings)?),
        None => None,
    };

    let mut printer = Printer::new(out, args.format, annotate);
    let (mut failed, mut warnings) = (false, 0usize);
    for file in &args.files {
        let result = check_file(file, &settings, schema.as_ref(), &mut printer)
            .map_err(|e| format!("cannot write output: {e}"))?;
        failed |= result.errors > 0;
        warnings += result.warnings;
    }
    printer
        .finish()
        .map_err(|e| format!("cannot write output: {e}"))?;
    let too_many_warnings = args.max_warnings.is_some_and(|max| warnings > max);
    Ok(u8::from(failed || too_many_warnings))
}

#[derive(Default)]
struct Counts {
    errors: usize,
    warnings: usize,
}

/// Check one file, and against `schema` if there is one. The name is opened as given; its lossy
/// UTF-8 form is only for output and glob matching.
fn check_file(
    file: &OsStr,
    settings: &Settings,
    schema: Option<&Schema>,
    printer: &mut Printer<impl Write>,
) -> io::Result<Counts> {
    let text = match std::fs::read(Path::new(file)) {
        Ok(bytes) => String::from_utf8(bytes)
            .map_err(|e| format!("not valid UTF-8 (byte {})", e.utf8_error().valid_up_to())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Err("no such file".to_owned()),
        Err(e) => Err(format!("cannot read: {e}")),
    };
    let name = file.to_string_lossy();
    let text = match text {
        Ok(text) => text,
        Err(message) => {
            printer.unreadable(file, &message)?;
            return Ok(Counts {
                errors: 1,
                warnings: 0,
            });
        }
    };
    let config = settings.config_for(&name);
    let (report, events) = match schema {
        Some(_) => jsonvet::parse(&text, &config),
        None => (jsonvet::check(&text, &config), None),
    };
    let mut counts = Counts::default();
    for problem in report.problems() {
        printer.problem(file, &report, problem)?;
        match problem.severity() {
            Severity::Error => counts.errors += 1,
            Severity::Warning => counts.warnings += 1,
            Severity::Off => {}
        }
    }
    // `events` is `None` after a syntax error, so such a file gets no schema errors.
    if let (Some(schema), Some(events)) = (schema, events) {
        for (offset, message) in schema.errors(&events) {
            printer.schema_error(file, &report, offset, &message)?;
            counts.errors += 1;
        }
    }
    Ok(counts)
}

fn print(mut out: impl Write, text: &str) -> Result<u8, String> {
    writeln!(out, "{text}").map_err(|e| format!("cannot write output: {e}"))?;
    Ok(0)
}
