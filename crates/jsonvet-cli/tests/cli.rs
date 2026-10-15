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
    assert!(!run("false").contains("::"));
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
         ::error file=\u{1b}]0;x\u{7}.json,line=1,col=4,title=jsonvet (syntax)::expected a value, found the end of the file\n\
         gone\\u001b[2J.json: error: no such file\n\
         ::error file=gone\u{1b}[2J.json,title=jsonvet::no such file\n"
    );
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
    // A directory is unreadable for a reason other than not existing.
    fs::create_dir(dir.join("folder.json")).expect("create a folder");
    let (code, out) = jsonvet(&dir, &["folder.json"]);
    assert_eq!(code, 1);
    assert!(out.contains(": error: cannot read: "), "{out}");
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
    let name = OsStr::from_bytes(b"caf\xe9.json");
    fs::write(dir.join(name), "[1,]").expect("write file");
    let out = Command::new(env!("CARGO_BIN_EXE_jsonvet"))
        .args(["--strict", "*"])
        .arg(name)
        .current_dir(&dir)
        .env_remove("GITHUB_ACTIONS")
        .output()
        .expect("run jsonvet");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "caf\u{FFFD}.json:1:4: error[syntax] trailing comma before `]` is not allowed in json\n"
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
        ("d.json", "{}"),
    ]);
    for (config, message) in [
        ("rules.toml", "`rules` must be a table"),
        ("dialects.toml", "`dialects` must be an array of tables"),
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
    // `Rule` is non-exhaustive, so a new rule without a description would compile.
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
}
