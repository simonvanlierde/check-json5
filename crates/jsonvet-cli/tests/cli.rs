//! The jsonvet binary end to end: output, exit codes, the 1.x options and config discovery.

#![allow(
    clippy::expect_used,
    reason = "a broken test setup should fail the test loudly"
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A fresh directory for one test, inside the target directory so it is never in a repo the
/// config search could reach.
fn workdir(files: &[(&str, &str)]) -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "cli-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    for (name, text) in files {
        let path = dir.join(name);
        fs::create_dir_all(path.parent().expect("a parent")).expect("create dirs");
        fs::write(path, text).expect("write file");
    }
    fs::create_dir_all(dir.join(".git")).expect("mark the repository root");
    dir
}

/// Run jsonvet in `dir`; return its exit code and standard output.
fn jsonvet(dir: &Path, args: &[&str]) -> (i32, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_jsonvet"))
        .args(args)
        .current_dir(dir)
        .env_remove("GITHUB_ACTIONS")
        .output()
        .expect("run jsonvet");
    (
        out.status.code().expect("an exit code"),
        String::from_utf8(out.stdout).expect("UTF-8 output"),
    )
}

#[test]
fn reports_and_exit_codes() {
    let dir = workdir(&[
        ("ok.json", "{\"a\": 1, // c\n}"),
        ("dup.json", "{\n  \"a\": 1,\n  \"a\": 2\n}"),
        ("bom.json", "\u{FEFF}{}"),
    ]);
    assert_eq!(jsonvet(&dir, &["ok.json"]), (0, String::new()));
    assert_eq!(
        jsonvet(&dir, &["ok.json", "dup.json", "gone.json"]),
        (
            1,
            "dup.json:3:3: error[duplicate-key] duplicate key $.a (first at 2:3)\ngone.json: error: no such file\n"
                .to_owned()
        )
    );
    let (code, out) = jsonvet(&dir, &["--strict", "*.json", "bom.json"]);
    assert_eq!(
        (code, out.as_str()),
        (
            0,
            "bom.json:1:1: warning[byte-order-mark] byte order mark at the start of the file\n"
        )
    );
    assert_eq!(
        jsonvet(
            &dir,
            &["--strict", "*.json", "--max-warnings", "0", "bom.json"]
        )
        .0,
        1
    );
    assert_eq!(
        jsonvet(
            &dir,
            &["--strict", "*.json", "--max-warnings", "1", "bom.json"]
        )
        .0,
        0
    );
    assert_eq!(
        jsonvet(
            &dir,
            &[
                "--strict",
                "*.json",
                "--ignore",
                "byte-order-mark",
                "bom.json"
            ]
        ),
        (0, String::new())
    );
}

#[test]
fn github_annotations_follow_each_problem() {
    let dir = workdir(&[
        ("bom.json", "\u{FEFF}{}"),
        ("dup.json", "{\"a\":1,\"a\":2}"),
    ]);
    let run = |actions: &str| {
        let out = Command::new(env!("CARGO_BIN_EXE_jsonvet"))
            .args(["--strict", "*.json", "bom.json", "dup.json"])
            .current_dir(&dir)
            .env("GITHUB_ACTIONS", actions)
            .output()
            .expect("run jsonvet");
        String::from_utf8(out.stdout).expect("UTF-8 output")
    };
    assert_eq!(
        run("true"),
        "bom.json:1:1: warning[byte-order-mark] byte order mark at the start of the file\n\
         ::warning file=bom.json,line=1,col=1,title=jsonvet (byte-order-mark)::byte order mark at the start of the file\n\
         dup.json:1:8: error[duplicate-key] duplicate key $.a (first at 1:2)\n\
         ::error file=dup.json,line=1,col=8,title=jsonvet (duplicate-key)::duplicate key $.a (first at 1:2)\n"
    );
    assert_eq!(
        run("false"),
        "bom.json:1:1: warning[byte-order-mark] byte order mark at the start of the file\n\
         dup.json:1:8: error[duplicate-key] duplicate key $.a (first at 1:2)\n"
    );
}

// Unix only: Windows refuses `:` in a file name.
#[cfg(unix)]
#[test]
fn a_file_name_cannot_start_a_workflow_command_or_send_escapes() {
    let dir = workdir(&[
        ("::stop-commands::tok.json", "[1,"),
        ("\u{1b}]0;x\u{7}.json", "[1,"),
    ]);
    let out = Command::new(env!("CARGO_BIN_EXE_jsonvet"))
        .args([
            "::stop-commands::tok.json",
            "\u{1b}]0;x\u{7}.json",
            "gone\u{1b}[2J.json",
        ])
        .current_dir(&dir)
        .env("GITHUB_ACTIONS", "true")
        .output()
        .expect("run jsonvet");
    assert_eq!(
        String::from_utf8(out.stdout).expect("UTF-8 output"),
        "./::stop-commands::tok.json:1:4: error[syntax] expected a value, found the end of the file\n\
         ::error file=%3A%3Astop-commands%3A%3Atok.json,line=1,col=4,title=jsonvet (syntax)::expected a value, found the end of the file\n\
         \\u001b]0;x\\u0007.json:1:4: error[syntax] expected a value, found the end of the file\n\
         ::error file=\\u001b]0;x\\u0007.json,line=1,col=4,title=jsonvet (syntax)::expected a value, found the end of the file\n\
         gone\\u001b[2J.json: error: no such file\n\
         ::error file=gone\\u001b[2J.json,title=jsonvet::no such file\n"
    );
}

/// A bidi control would reorder how the rest of the line displays, so neither a file name nor a
/// key in a message may print one, in text or annotations.
#[test]
fn output_never_holds_a_bidi_control() {
    let bidi = [
        '\u{61c}', '\u{200e}', '\u{200f}', '\u{202a}', '\u{202b}', '\u{202c}', '\u{202d}',
        '\u{202e}', '\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}',
    ];
    let names: Vec<String> = bidi.iter().map(|c| format!("{c}.json")).collect();
    let texts: Vec<String> = bidi
        .iter()
        .map(|c| format!("{{\"{c}\": 1, \"{c}\": 2}}"))
        .collect();
    let files: Vec<(&str, &str)> = names
        .iter()
        .zip(&texts)
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .collect();
    let dir = workdir(&files);
    for actions in ["false", "true"] {
        let out = Command::new(env!("CARGO_BIN_EXE_jsonvet"))
            .args(&names)
            .current_dir(&dir)
            .env("GITHUB_ACTIONS", actions)
            .output()
            .expect("run jsonvet");
        let out = String::from_utf8(out.stdout).expect("UTF-8 output");
        assert!(!out.contains(bidi), "{out}");
        let expected: String = bidi
            .iter()
            .map(|&c| {
                let c = format!("\\u{:04x}", u32::from(c));
                let text = format!("{c}.json:1:10: error[duplicate-key] duplicate key $[\"{c}\"] (first at 1:2)\n");
                if actions == "true" {
                    format!("{text}::error file={c}.json,line=1,col=10,title=jsonvet (duplicate-key)::duplicate key $[\"{c}\"] (first at 1:2)\n")
                } else {
                    text
                }
            })
            .collect();
        assert_eq!(out, expected);
    }
}

/// GitHub Actions also runs the older `##[command]` form found anywhere in a line, so neither a
/// file name nor a key in a message may print one, in text, annotations or SARIF.
#[test]
fn output_never_holds_a_legacy_workflow_command() {
    let dir = workdir(&[(
        "##[add-mask]a.json",
        "{\"##[add-mask]s\": 1, \"##[add-mask]s\": 2}",
    )]);
    let run = |format: &str| {
        let out = Command::new(env!("CARGO_BIN_EXE_jsonvet"))
            .args(["--format", format, "##[add-mask]a.json"])
            .current_dir(&dir)
            .env("GITHUB_ACTIONS", "true")
            .output()
            .expect("run jsonvet");
        String::from_utf8(out.stdout).expect("UTF-8 output")
    };
    assert_eq!(
        run("text"),
        "##\\u005badd-mask]a.json:1:22: error[duplicate-key] duplicate key $[\"##\\u005badd-mask]s\"] (first at 1:2)\n\
         ::error file=##\\u005badd-mask]a.json,line=1,col=22,title=jsonvet (duplicate-key)::duplicate key $[\"##\\u005badd-mask]s\"] (first at 1:2)\n"
    );
    let sarif = run("sarif");
    assert!(!sarif.contains("##["), "{sarif}");
    let sarif: serde_json::Value = serde_json::from_str(&sarif).expect("JSON output");
    assert_eq!(
        sarif["runs"][0]["results"][0]["message"]["text"],
        "duplicate key $[\"##[add-mask]s\"] (first at 1:2)"
    );
}

/// A reader that stops early, as `| head -1` does, ends the run quietly with the verdict so far.
#[test]
fn a_closed_pipe_keeps_the_verdict() {
    let dir = workdir(&[("bad.json", "[1,")]);
    // About 216 KB of output, far more than a pipe holds, so jsonvet blocks writing until the
    // reader closes. The command line stays near 27,000 characters, under Windows' 32,767.
    let files = vec!["bad.json"; 3000];
    let mut child = Command::new(env!("CARGO_BIN_EXE_jsonvet"))
        .args(&files)
        .current_dir(&dir)
        .env_remove("GITHUB_ACTIONS")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("run jsonvet");
    drop(child.stdout.take());
    let out = child.wait_with_output().expect("wait for jsonvet");
    assert_eq!(String::from_utf8_lossy(&out.stderr), "");
    assert_eq!(out.status.code(), Some(1));
}

/// `--help` and `--version` into a pipe nobody reads still succeed.
#[cfg(unix)]
#[test]
fn help_into_a_closed_pipe_succeeds() {
    for flag in ["--help", "--version"] {
        let (reader, writer) = std::io::pipe().expect("a pipe");
        drop(reader);
        let out = Command::new(env!("CARGO_BIN_EXE_jsonvet"))
            .arg(flag)
            .stdout(writer)
            .output()
            .expect("run jsonvet");
        assert_eq!(String::from_utf8_lossy(&out.stderr), "", "{flag}");
        assert_eq!(out.status.code(), Some(0), "{flag}");
    }
}

#[test]
fn help_version_and_the_check_json5_name() {
    let dir = workdir(&[("dup.json", "{\"a\":1,\"a\":2}"), ("sub.json", "{}")]);
    let version = format!("jsonvet {}\n", env!("CARGO_PKG_VERSION"));
    assert_eq!(jsonvet(&dir, &["-V"]), (0, version.clone()));
    assert_eq!(jsonvet(&dir, &["--version"]), (0, version));
    for flag in ["-h", "--help"] {
        let (code, out) = jsonvet(&dir, &[flag]);
        assert_eq!(code, 0);
        assert!(out.starts_with("Check JSON, I-JSON,"), "{flag}");
    }
    let out = Command::new(env!("CARGO_BIN_EXE_check-json5"))
        .arg("dup.json")
        .current_dir(&dir)
        .env_remove("GITHUB_ACTIONS")
        .output()
        .expect("run check-json5");
    assert_eq!(out.status.code(), Some(1));
    fs::create_dir(dir.join("folder.json")).expect("create a folder");
    assert_eq!(
        jsonvet(&dir, &["folder.json"]),
        (1, "folder.json: error: not a regular file\n".to_owned())
    );
}

/// A device or a FIFO is reported, not read: `/dev/zero` would fill memory and a FIFO with no
/// writer would wait for ever. A symlink to a regular file is followed.
#[cfg(unix)]
#[test]
fn only_regular_files_are_read() {
    use std::os::unix::fs::symlink;
    use std::time::{Duration, Instant};
    let dir = workdir(&[("real.json", "[1,")]);
    symlink("/dev/zero", dir.join("zero.json")).expect("symlink");
    symlink("real.json", dir.join("link.json")).expect("symlink");
    let fifo = Command::new("mkfifo")
        .arg(dir.join("fifo.json"))
        .status()
        .expect("run mkfifo");
    assert!(fifo.success());
    let mut child = Command::new(env!("CARGO_BIN_EXE_jsonvet"))
        .args(["zero.json", "fifo.json", "link.json"])
        .current_dir(&dir)
        .env_remove("GITHUB_ACTIONS")
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("run jsonvet");
    let start = Instant::now();
    while child.try_wait().expect("wait").is_none() {
        if start.elapsed() > Duration::from_secs(10) {
            let _ = child.kill();
            panic!("jsonvet still running after 10 seconds");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let out = child.wait_with_output().expect("output");
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(out.stdout).expect("UTF-8 output"),
        "zero.json: error: not a regular file\n\
         fifo.json: error: not a regular file\n\
         link.json:1:4: error[syntax] expected a value, found the end of the file\n"
    );
}

#[test]
fn strict_globs_keep_their_1x_meaning() {
    let dir = workdir(&[
        ("package.json", "{// c\n}"),
        ("a/package.json", "{// c\n}"),
        ("tsconfig.json", "{// c\n}"),
        (".vscode/s.json", "{// c\n}"),
    ]);
    let (code, out) = jsonvet(
        &dir,
        &[
            "--strict",
            "*.json",
            "--strict",
            "!tsconfig*.json",
            "--strict",
            "!.vscode/*",
            "package.json",
            "a/package.json",
            "tsconfig.json",
            ".vscode/s.json",
        ],
    );
    assert_eq!(
        (code, out.as_str()),
        (
            1,
            "package.json:1:2: error[syntax] comments are not allowed in json\na/package.json:1:2: error[syntax] comments are not allowed in json\n"
        )
    );
}

#[test]
fn an_unknown_option_is_a_usage_error_unless_it_is_a_file() {
    let dir = workdir(&[("bad.json", "[1,"), ("--x.json", "[1,")]);
    let run = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_jsonvet"))
            .args(args)
            .current_dir(&dir)
            .env_remove("GITHUB_ACTIONS")
            .output()
            .expect("run jsonvet");
        (
            out.status.code().expect("an exit code"),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    };
    let (code, out, err) = run(&["--strcit", "*.json", "bad.json"]);
    assert_eq!((code, out.as_str()), (2, ""));
    assert!(
        err.starts_with("jsonvet: unknown option `--strcit`\n"),
        "{err}"
    );
    let found = "--x.json:1:4: error[syntax] expected a value, found the end of the file\n";
    assert_eq!(run(&["--x.json"]), (1, found.to_owned(), String::new()));
    assert_eq!(
        run(&["--", "--x.json"]),
        (1, found.to_owned(), String::new())
    );
}

#[test]
fn help_and_version_letters_in_a_file_name_are_a_file() {
    let dir = workdir(&[("bad.json", "[1,"), ("-h.json", "{}"), ("-V.json", "{}")]);
    let (code, out) = jsonvet(&dir, &["bad.json", "-h.json", "-V.json"]);
    assert_eq!(
        (code, out.as_str()),
        (
            1,
            "bad.json:1:4: error[syntax] expected a value, found the end of the file\n"
        )
    );
}

// Linux only: macOS file systems refuse names that are not UTF-8.
#[cfg(target_os = "linux")]
#[test]
fn a_file_name_that_is_not_utf8_is_opened_as_given() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    let dir = workdir(&[]);
    // The names that look like options are files too, not read lossily as options.
    let names = [b"caf\xe9.json".as_slice(), b"--\xff.json", b"-x\xff.json"].map(OsStr::from_bytes);
    for name in names {
        fs::write(dir.join(name), "[1,]").expect("write file");
    }
    let out = Command::new(env!("CARGO_BIN_EXE_jsonvet"))
        .args(["--strict", "*"])
        .args(names)
        .current_dir(&dir)
        .env_remove("GITHUB_ACTIONS")
        .output()
        .expect("run jsonvet");
    let found = ".json:1:4: error[syntax] trailing comma before `]` is not allowed in json\n";
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        format!("caf\u{FFFD}{found}--\u{FFFD}{found}-x\u{FFFD}{found}")
    );
}

#[test]
fn config_file_and_its_search() {
    let dir = workdir(&[
        (
            "jsonvet.toml",
            "rules = { duplicate-key = \"warning\" }\ndialects = [{ files = \"*.data\", dialect = \"jsonl\" }]\n",
        ),
        ("sub/x.data", "{\"a\": 1, \"a\": 2}\n\n"),
    ]);
    let (code, out) = jsonvet(&dir.join("sub"), &["x.data"]);
    assert_eq!(code, 1);
    assert_eq!(
        out,
        "x.data:1:10: warning[duplicate-key] duplicate key $.a (first at 1:2)\nx.data:2:1: error[syntax] blank line; JSON Lines needs a value on every line\n"
    );
    // The command line wins over the file, and --no-config skips it.
    assert_eq!(
        jsonvet(
            &dir.join("sub"),
            &["--error", "duplicate-key", "--dialect", "json5", "x.data"]
        )
        .1
        .lines()
        .next(),
        Some("x.data:1:10: error[duplicate-key] duplicate key $.a (first at 1:2)")
    );
    assert_eq!(jsonvet(&dir.join("sub"), &["--no-config", "x.data"]).0, 1);
}

#[test]
fn config_globs_are_relative_to_the_config_file() {
    let toml = "dialects = [{ files = \"data/*.json\", dialect = \"json\" }]\n";
    let pyproject = format!("[tool.jsonvet]\n{toml}");
    let unquoted = "data/x.json:1:2: error[syntax] unquoted keys are not allowed in json\n";
    for config in [
        ("jsonvet.toml", toml),
        ("pyproject.toml", pyproject.as_str()),
    ] {
        let dir = workdir(&[
            config,
            ("data/x.json", "{a: 1}"),
            ("outside/x.json", "{a: 1}"),
            (
                "outside/c.toml",
                "dialects = [{ files = \"*.json\", dialect = \"json\" }]\n",
            ),
        ]);
        assert_eq!(jsonvet(&dir, &["data/x.json"]), (1, unquoted.to_owned()));
        let absolute = dir.join("data/x.json");
        let absolute = absolute.to_str().expect("a UTF-8 path");
        assert_eq!(jsonvet(&dir, &[absolute]).0, 1, "{absolute}");
        assert_eq!(jsonvet(&dir.join("data"), &["x.json"]).0, 1);
        assert_eq!(jsonvet(&dir.join("outside"), &["../data/x.json"]).0, 1);
        // The command line still matches the path as given.
        let cli = ["--no-config", "--dialect", "json:data/*.json", "x.json"];
        assert_eq!(jsonvet(&dir.join("data"), &cli), (0, String::new()));
        // A file outside the config file's directory matches none of its patterns.
        let other = ["--config", "outside/c.toml", "data/x.json"];
        assert_eq!(jsonvet(&dir, &other), (0, String::new()));
    }
}

#[test]
fn config_search_stops_at_the_repository_root() {
    let outer = workdir(&[("jsonvet.toml", "rules = { duplicate-key = \"ignore\" }\n")]);
    let repo = outer.join("repo");
    fs::create_dir_all(repo.join(".git")).expect("inner repository");
    fs::write(repo.join("d.json"), "{\"a\": 1, \"a\": 2}").expect("write");
    assert_eq!(
        jsonvet(&repo, &["d.json"]).0,
        1,
        "the outer jsonvet.toml must not apply"
    );
}

#[test]
fn pyproject_tool_table_and_bad_configs() {
    let dir = workdir(&[
        (
            "pyproject.toml",
            "[project]\nname = \"x\"\n\n[tool.jsonvet]\nrules = { duplicate-key = \"ignore\" }\n",
        ),
        ("d.json", "{\"a\": 1, \"a\": 2}"),
        ("bad.toml", "rules = { nope = \"error\" }\n"),
        (
            "bad-entry.toml",
            "dialects = [{ files = \"*\", dialect = \"json\", dialekt = \"json5\" }]\n",
        ),
    ]);
    assert_eq!(jsonvet(&dir, &["d.json"]), (0, String::new()));
    assert_eq!(jsonvet(&dir, &["--config", "bad.toml", "d.json"]).0, 2);
    assert_eq!(
        jsonvet(&dir, &["--config", "bad-entry.toml", "d.json"]).0,
        2
    );

    let typo = workdir(&[
        (
            "pyproject.toml",
            "[tool.jsonvet]\nrule = { duplicate-key = \"ignore\" }\n",
        ),
        ("d.json", "{}"),
    ]);
    assert_eq!(jsonvet(&typo, &["d.json"]).0, 2);

    // A pyproject.toml without the table is no config file.
    let plain = workdir(&[
        ("pyproject.toml", "[project]\nname = \"x\"\n"),
        ("d.json", "{\"a\": 1, \"a\": 2}"),
    ]);
    assert_eq!(jsonvet(&plain, &["d.json"]).0, 1);

    let wrong_types = workdir(&[
        ("rules.toml", "rules = 1\n"),
        ("dialects.toml", "dialects = 1\n"),
        ("forged.toml", "\"\\n::error::x\" = \"error\"\n"),
        ("d.json", "{}"),
    ]);
    for (config, message) in [
        ("rules.toml", "`rules` must be a table"),
        ("dialects.toml", "`dialects` must be an array of tables"),
        // A key holding a newline stays on the line, so it cannot forge a workflow command.
        (
            "forged.toml",
            "unknown key `\\n::error::x`; expected rules or dialects",
        ),
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_jsonvet"))
            .args(["--config", config, "d.json"])
            .current_dir(&wrong_types)
            .output()
            .expect("run jsonvet");
        assert_eq!(out.status.code(), Some(2));
        assert_eq!(
            String::from_utf8_lossy(&out.stderr),
            format!("jsonvet: {config}: {message}\n")
        );
    }
}

#[test]
fn sarif_holds_every_problem_and_keeps_the_exit_codes() {
    let dir = workdir(&[
        ("dup.json", "{\n  \"a\": 1,\n  \"a\": 2\n}"),
        ("bom.json", "\u{FEFF}{}"),
        ("ok.json", "{}"),
        ("open.json", "[1, /* x"),
    ]);
    let run = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_jsonvet"))
            .args(args)
            .current_dir(&dir)
            // SARIF replaces the workflow annotations, even in GitHub Actions.
            .env("GITHUB_ACTIONS", "true")
            .output()
            .expect("run jsonvet");
        let sarif: serde_json::Value = serde_json::from_slice(&out.stdout).expect("JSON output");
        (out.status.code().expect("an exit code"), sarif)
    };

    let (code, sarif) = run(&["--format", "sarif", "dup.json", "gone.json", "ok.json"]);
    assert_eq!(code, 1);
    assert_eq!(sarif["version"], "2.1.0");
    assert_eq!(
        sarif["$schema"],
        "https://json.schemastore.org/sarif-2.1.0.json"
    );
    let run0 = &sarif["runs"][0];
    assert_eq!(run0["tool"]["driver"]["name"], "jsonvet");
    assert_eq!(run0["tool"]["driver"]["version"], env!("CARGO_PKG_VERSION"));
    let rules = run0["tool"]["driver"]["rules"].as_array().expect("rules");
    let ids: Vec<&str> = rules.iter().filter_map(|r| r["id"].as_str()).collect();
    assert_eq!(
        ids,
        [
            "syntax",
            "duplicate-key",
            "invalid-number",
            "invalid-code-point",
            "byte-order-mark",
            "missing-final-newline",
            "unreadable"
        ]
    );
    assert!(rules.iter().all(|r| {
        !r["shortDescription"]["text"]
            .as_str()
            .unwrap_or("")
            .is_empty()
    }));
    assert_eq!(
        rules[0]["shortDescription"]["text"],
        "the first syntax error; checking stops there"
    );
    assert_eq!(run0["columnKind"], "unicodeCodePoints");
    assert!(run0["originalUriBaseIds"]["%SRCROOT%"]["description"]["text"].is_string());
    let results = run0["results"].as_array().expect("results");
    assert_eq!(results.len(), 2);
    let dup = &results[0];
    assert_eq!(dup["ruleId"], "duplicate-key");
    assert_eq!(dup["level"], "error");
    assert_eq!(dup["message"]["text"], "duplicate key $.a (first at 2:3)");
    let at = &dup["locations"][0]["physicalLocation"];
    assert_eq!(at["artifactLocation"]["uri"], "dup.json");
    assert_eq!(at["artifactLocation"]["uriBaseId"], "%SRCROOT%");
    assert_eq!(at["region"]["startLine"], 3);
    assert_eq!(at["region"]["startColumn"], 3);
    let first = &dup["relatedLocations"][0];
    assert_eq!(first["message"]["text"], "first copy");
    assert_eq!(first["physicalLocation"]["region"]["startLine"], 2);
    assert_eq!(first["physicalLocation"]["region"]["startColumn"], 3);
    assert_eq!(results[1]["ruleId"], "unreadable");
    assert_eq!(results[1]["message"]["text"], "no such file");

    // A warning passes unless --max-warnings says otherwise, as with text output.
    let (code, sarif) = run(&["--format=sarif", "--strict", "*.json", "bom.json"]);
    assert_eq!(code, 0);
    assert_eq!(sarif["runs"][0]["results"][0]["level"], "warning");
    let args = [
        "--format=sarif",
        "--strict",
        "*.json",
        "--max-warnings",
        "0",
        "bom.json",
    ];
    assert_eq!(run(&args).0, 1);
    let (code, sarif) = run(&["--format", "sarif", "ok.json"]);
    assert_eq!(code, 0);
    assert_eq!(sarif["runs"][0]["results"], serde_json::json!([]));

    // A syntax error's related location is where the unterminated comment starts.
    let (_, sarif) = run(&["--format", "sarif", "open.json"]);
    let open = &sarif["runs"][0]["results"][0];
    assert_eq!(
        open["message"]["text"],
        "unterminated block comment (started at 1:5)"
    );
    let start = &open["relatedLocations"][0];
    assert_eq!(start["message"]["text"], "starts here");
    assert_eq!(start["physicalLocation"]["region"]["startColumn"], 5);
}

#[test]
fn sarif_columns_count_code_points() {
    // CRLF ends line 1; the emoji is one code point but two UTF-16 code units.
    let dir = workdir(&[("e.jsonc", "{\r\n\"\u{1F600}\": x}\r\n")]);
    let out = Command::new(env!("CARGO_BIN_EXE_jsonvet"))
        .args(["--format", "sarif", "e.jsonc"])
        .current_dir(&dir)
        .output()
        .expect("run jsonvet");
    assert_eq!(out.status.code(), Some(1));
    let sarif: serde_json::Value = serde_json::from_slice(&out.stdout).expect("JSON output");
    let run = &sarif["runs"][0];
    assert_eq!(run["columnKind"], "unicodeCodePoints");
    let result = &run["results"][0];
    assert_eq!(result["ruleId"], "syntax");
    let region = &result["locations"][0]["physicalLocation"]["region"];
    assert_eq!(
        (&region["startLine"], &region["startColumn"]),
        (&2.into(), &6.into())
    );
}

#[test]
fn the_last_matching_dialect_wins_and_auto_means_the_extension() {
    let dir = workdir(&[
        ("x.cfg", "{'a': 1}"),
        ("y.cfg", "{'a': 1}"),
        ("c.jsonc", "{// c\n}"),
    ]);
    let jsonc = |name: &str| {
        format!("{name}:1:2: error[syntax] single-quoted strings are not allowed in jsonc\n")
    };
    let args = [
        "--dialect",
        "jsonc:*.cfg",
        "--dialect",
        "auto:x.cfg",
        "x.cfg",
        "y.cfg",
    ];
    assert_eq!(jsonvet(&dir, &args), (1, jsonc("y.cfg")));
    let args = [
        "--dialect",
        "auto:x.cfg",
        "--dialect",
        "jsonc:*.cfg",
        "x.cfg",
        "y.cfg",
    ];
    assert_eq!(jsonvet(&dir, &args), (1, jsonc("x.cfg") + &jsonc("y.cfg")));
    // `auto` without a pattern undoes an earlier `json` for every file.
    let args = ["--dialect", "json", "--dialect", "auto", "c.jsonc"];
    assert_eq!(jsonvet(&dir, &args), (0, String::new()));
    assert_eq!(jsonvet(&dir, &["--dialect", "json", "c.jsonc"]).0, 1);
}

#[cfg(windows)]
#[test]
fn a_backslash_path_matches_a_slash_pattern() {
    let dir = workdir(&[("sub/a.json", "{a: 1}")]);
    let (code, out) = jsonvet(&dir, &["--dialect", "json:sub/*.json", "sub\\a.json"]);
    assert_eq!(
        (code, out.as_str()),
        (
            1,
            "sub\\a.json:1:2: error[syntax] unquoted keys are not allowed in json\n"
        )
    );
}

#[test]
fn each_extension_has_its_dialect() {
    let single = "{'a': 1}";
    let mut files = vec![(".babelrc".to_owned(), single)];
    let extensions = [
        ("jsonc", "jsonc"),
        ("code-workspace", "jsonc"),
        ("code-snippets", "jsonc"),
        ("hujson", "jwcc"),
        ("jwcc", "jwcc"),
    ];
    let json5 = [
        "json",
        "json5",
        "geojson",
        "jsonld",
        "webmanifest",
        "cfg",
        "JSONC",
    ];
    for (ext, _) in extensions {
        files.push((format!("a.{ext}"), single));
    }
    for ext in json5 {
        files.push((format!("a.{ext}"), single));
    }
    // Two values on two lines: JSON Lines, not one JSON5 document.
    files.push(("a.jsonl".to_owned(), "{}\n{}\n"));
    files.push(("a.ndjson".to_owned(), "{}\n{}\n"));
    let dir = workdir(
        &files
            .iter()
            .map(|(n, t)| (n.as_str(), *t))
            .collect::<Vec<_>>(),
    );
    for (ext, dialect) in extensions {
        let name = format!("a.{ext}");
        assert_eq!(
            jsonvet(&dir, &[&name]),
            (
                1,
                format!(
                    "{name}:1:2: error[syntax] single-quoted strings are not allowed in {dialect}\n"
                )
            )
        );
    }
    for name in json5
        .iter()
        .map(|ext| format!("a.{ext}"))
        .chain([".babelrc".to_owned()])
    {
        assert_eq!(jsonvet(&dir, &[&name]), (0, String::new()), "{name}");
    }
    for name in ["a.jsonl", "a.ndjson"] {
        assert_eq!(jsonvet(&dir, &[name]), (0, String::new()), "{name}");
        assert_eq!(jsonvet(&dir, &["--dialect", "json5", name]).0, 1, "{name}");
    }
}

/// A write error other than a closed pipe, such as a full disk, is a failure of the run (exit 2),
/// for problems and for `--version` alike.
#[cfg(target_os = "linux")]
#[test]
fn a_failed_write_is_an_error() {
    let dir = workdir(&[("bad.json", "[1,")]);
    for args in [&["bad.json"][..], &["--version"]] {
        let full = fs::File::create("/dev/full").expect("open /dev/full");
        let out = Command::new(env!("CARGO_BIN_EXE_jsonvet"))
            .args(args)
            .current_dir(&dir)
            .env_remove("GITHUB_ACTIONS")
            .stdout(full)
            .output()
            .expect("run jsonvet");
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.starts_with("jsonvet: cannot write output:"),
            "{stderr}"
        );
    }
}

/// Only a missing file is "no such file"; any other failure to open it gives the system's reason.
#[cfg(unix)]
#[test]
fn a_file_that_cannot_be_read_says_why() {
    let dir = workdir(&[("a.json", "{}")]);
    let (code, out) = jsonvet(&dir, &["a.json/b.json", "gone.json"]);
    assert_eq!(code, 1);
    let lines: Vec<&str> = out.lines().collect();
    assert!(
        lines[0].starts_with("a.json/b.json: error: cannot read: "),
        "{out}"
    );
    assert_eq!(lines[1], "gone.json: error: no such file");
}

/// SARIF strings escape a `[` only where it follows `##`.
#[test]
fn sarif_keeps_other_brackets() {
    let dir = workdir(&[("a.json", "[{\"a\": 1, \"a\": 2}]")]);
    let (code, out) = jsonvet(&dir, &["--format", "sarif", "a.json"]);
    assert_eq!(code, 1);
    assert!(out.contains("duplicate key $[0].a"), "{out}");
}
