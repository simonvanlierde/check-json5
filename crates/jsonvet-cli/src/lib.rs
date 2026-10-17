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
mod settings;

use args::{Command, ConfigSource};
use output::Printer;
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

    let mut printer = Printer::new(out, args.format, annotate);
    let mut counts = Counts::default();
    let written = args
        .files
        .iter()
        .try_for_each(|file| check_file(file, &settings, &mut printer, &mut counts))
        .and_then(|()| printer.finish());
    // A reader that has gone, as with `| head`, wants no more output: the verdict so far stands.
    match written {
        Err(e) if e.kind() != io::ErrorKind::BrokenPipe => {
            return Err(format!("cannot write output: {e}"));
        }
        _ => {}
    }
    let too_many_warnings = args.max_warnings.is_some_and(|max| counts.warnings > max);
    Ok(u8::from(counts.errors > 0 || too_many_warnings))
}

#[derive(Default)]
struct Counts {
    errors: usize,
    warnings: usize,
}

/// Check one file and add its problems to `counts`, each before it is printed, so a failed
/// write still leaves the verdict. The name is opened as given; its lossy UTF-8 form is only
/// for output and glob matching.
fn check_file(
    file: &OsStr,
    settings: &Settings,
    printer: &mut Printer<impl Write>,
    counts: &mut Counts,
) -> io::Result<()> {
    let text = read(Path::new(file));
    let name = file.to_string_lossy();
    let text = match text {
        Ok(text) => text,
        Err(message) => {
            counts.errors += 1;
            return printer.unreadable(file, &message);
        }
    };
    let report = jsonvet::check(&text, &settings.config_for(&name));
    for problem in report.problems() {
        match problem.severity() {
            Severity::Error => counts.errors += 1,
            Severity::Warning => counts.warnings += 1,
            _ => {}
        }
        printer.problem(file, &report, problem)?;
    }
    Ok(())
}

/// A file's text, or why it has none. Only regular files are read, so a FIFO cannot hang the
/// run and a device such as `/dev/zero` cannot fill memory. A symlink to a regular file is
/// followed, as `cat` would.
fn read(path: &Path) -> Result<String, String> {
    let read = std::fs::metadata(path).and_then(|meta| {
        if meta.is_file() {
            std::fs::read(path).map(Some)
        } else {
            Ok(None)
        }
    });
    match read {
        Ok(Some(bytes)) => String::from_utf8(bytes)
            .map_err(|e| format!("not valid UTF-8 (byte {})", e.utf8_error().valid_up_to())),
        Ok(None) => Err("not a regular file".to_owned()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Err("no such file".to_owned()),
        Err(e) => Err(format!("cannot read: {e}")),
    }
}

fn print(mut out: impl Write, text: &str) -> Result<u8, String> {
    match writeln!(out, "{text}").and_then(|()| out.flush()) {
        Err(e) if e.kind() != io::ErrorKind::BrokenPipe => Err(format!("cannot write output: {e}")),
        _ => Ok(0),
    }
}
