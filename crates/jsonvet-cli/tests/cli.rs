//! The jsonvet binary end to end: output, exit codes, the 1.x options and config discovery.

#![allow(
    clippy::expect_used,
    reason = "a broken test setup should fail the test loudly"
)]

use std::fmt::Write as _;
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
            "unreadable",
            "schema"
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

/// Run jsonvet in `dir` with GitHub annotations off; return its exit code, stdout and stderr.
fn jsonvet_stderr(dir: &Path, args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_jsonvet"))
        .args(args)
        .current_dir(dir)
        .env_remove("GITHUB_ACTIONS")
        .output()
        .expect("run jsonvet");
    (
        out.status.code().expect("an exit code"),
        String::from_utf8(out.stdout).expect("UTF-8 output"),
        String::from_utf8(out.stderr).expect("UTF-8 errors"),
    )
}

/// A schema in JSON5, so the schema file itself may use any dialect's syntax.
const SCHEMA: &str = r#"{
  // Every file below breaks it the same three ways.
  type: "object",
  properties: {
    name: { type: "string" },
    port: { type: "integer" },
    tags: { type: "array", items: { type: "string" } },
  },
  required: ["name"],
  additionalProperties: false,
}
"#;

#[test]
fn schema_errors_point_at_the_value_in_every_dialect() {
    let dir = workdir(&[
        ("schema.json", SCHEMA),
        (
            "strict.json",
            "{\"name\": \"é\", \"port\": \"80\",\n \"tags\": [\"a\", 1], \"x\": 0}",
        ),
        (
            "i.json",
            "{\"name\": \"é\", \"port\": \"80\",\n \"tags\": [\"a\", 1], \"x\": 0}",
        ),
        (
            "c.jsonc",
            "{\"name\": /* é */ \"n\", \"port\": \"80\", // c\n \"tags\": [\"a\", 1], \"x\": 0}",
        ),
        (
            "w.hujson",
            "{\"name\": \"n\", \"port\": \"80\",\n \"tags\": [\"a\", 1,], \"x\": 0,}",
        ),
        (
            "f.json5",
            "{name: 'n', 'po\\u0072t': '80',\n tags: ['a', +0x1], x: 0,}",
        ),
        (
            "l.jsonl",
            "{\"name\": \"n\", \"port\": 80}\n{\"port\": \"80\", \"tags\": [\"a\", 1], \"x\": 0}\n",
        ),
    ]);
    let (code, out, _) = jsonvet_stderr(
        &dir,
        &[
            "--schema",
            "schema.json",
            "--dialect",
            "json:strict.json",
            "--dialect",
            "i-json:i.json",
            "strict.json",
            "i.json",
            "c.jsonc",
            "w.hujson",
            "f.json5",
            "l.jsonl",
        ],
    );
    assert_eq!(code, 1);
    let expected = "\
strict.json:1:23: error[schema] \"80\" is not of type \"integer\" (at /port)
strict.json:2:16: error[schema] 1 is not of type \"string\" (at /tags/1)
strict.json:2:20: error[schema] Additional properties are not allowed ('x' was unexpected) (at /)
i.json:1:23: error[schema] \"80\" is not of type \"integer\" (at /port)
i.json:2:16: error[schema] 1 is not of type \"string\" (at /tags/1)
i.json:2:20: error[schema] Additional properties are not allowed ('x' was unexpected) (at /)
c.jsonc:1:31: error[schema] \"80\" is not of type \"integer\" (at /port)
c.jsonc:2:16: error[schema] 1 is not of type \"string\" (at /tags/1)
c.jsonc:2:20: error[schema] Additional properties are not allowed ('x' was unexpected) (at /)
w.hujson:1:23: error[schema] \"80\" is not of type \"integer\" (at /port)
w.hujson:2:16: error[schema] 1 is not of type \"string\" (at /tags/1)
w.hujson:2:21: error[schema] Additional properties are not allowed ('x' was unexpected) (at /)
f.json5:1:26: error[schema] \"80\" is not of type \"integer\" (at /port)
f.json5:2:14: error[schema] 1 is not of type \"string\" (at /tags/1)
f.json5:2:21: error[schema] Additional properties are not allowed ('x' was unexpected) (at /)
l.jsonl:2:1: error[schema] \"name\" is a required property (at /)
l.jsonl:2:10: error[schema] \"80\" is not of type \"integer\" (at /port)
l.jsonl:2:30: error[schema] 1 is not of type \"string\" (at /tags/1)
l.jsonl:2:34: error[schema] Additional properties are not allowed ('x' was unexpected) (at /)
";
    assert_eq!(out, expected);

    // The same errors as GitHub annotations and as SARIF results.
    let annotated = Command::new(env!("CARGO_BIN_EXE_jsonvet"))
        .args(["--schema", "schema.json", "f.json5"])
        .current_dir(&dir)
        .env("GITHUB_ACTIONS", "true")
        .output()
        .expect("run jsonvet");
    assert!(String::from_utf8_lossy(&annotated.stdout).contains(
        "::error file=f.json5,line=1,col=26,title=jsonvet (schema)::\"80\" is not of type"
    ));
    let (_, out, _) = jsonvet_stderr(
        &dir,
        &["--format", "sarif", "--schema", "schema.json", "f.json5"],
    );
    let sarif: serde_json::Value = serde_json::from_str(&out).expect("JSON output");
    let result = &sarif["runs"][0]["results"][0];
    assert_eq!(result["ruleId"], "schema");
    let region = &result["locations"][0]["physicalLocation"]["region"];
    assert_eq!(
        (&region["startLine"], &region["startColumn"]),
        (&1.into(), &26.into())
    );
}

#[test]
fn a_file_with_a_syntax_error_gets_no_schema_errors() {
    let dir = workdir(&[
        ("schema.json", SCHEMA),
        ("bad.json", "{\"port\": \"80\", \"x\": [1,"),
        ("dup.json", "{\"name\": \"n\", \"name\": \"m\"}"),
    ]);
    let (code, out, _) = jsonvet_stderr(&dir, &["--schema", "schema.json", "bad.json", "dup.json"]);
    assert_eq!(code, 1);
    assert_eq!(
        out,
        "bad.json:1:24: error[syntax] expected a value, found the end of the file\n\
         dup.json:1:15: error[duplicate-key] duplicate key $.name (first at 1:2)\n"
    );
}

#[test]
fn a_schema_that_cannot_be_used_is_a_usage_error() {
    let dir = workdir(&[
        ("ok.json", "{}"),
        ("type.json", "{\n  \"type\": 5\n}"),
        ("broken.json", "{\"type\": "),
        (
            "remote.json",
            "{\"properties\": {\"a\": {\"$ref\": \"https://example.com/a.json\"}}}",
        ),
        ("relative.json", "{\"$ref\": \"other.json\"}"),
        (
            "local.json",
            "{\"$defs\": {\"s\": {\"type\": \"string\"}}, \"$ref\": \"#/$defs/s\"}",
        ),
    ]);
    let cases = [
        (
            "type.json",
            "jsonvet: the schema type.json:2:11: 5 is not valid under any of the schemas listed in the 'anyOf' keyword\n",
        ),
        (
            "broken.json",
            "jsonvet: the schema broken.json:1:10: expected a value, found the end of the file\n",
        ),
        (
            "remote.json",
            "jsonvet: the schema remote.json has a `$ref` to https://example.com/a.json, outside the file; jsonvet fetches no schemas\n",
        ),
        (
            "relative.json",
            "jsonvet: the schema relative.json has a `$ref` to other.json, outside the file; jsonvet fetches no schemas\n",
        ),
        ("gone.json", "jsonvet: cannot read the schema gone.json: "),
    ];
    for (schema, message) in cases {
        let (code, out, err) = jsonvet_stderr(&dir, &["--schema", schema, "ok.json"]);
        assert_eq!((code, out.as_str()), (2, ""), "{schema}");
        assert!(err.starts_with(message), "{schema}: {err}");
    }
    // A `$ref` inside the schema file resolves.
    let (code, out, _) = jsonvet_stderr(&dir, &["--schema", "local.json", "ok.json"]);
    assert_eq!(
        (code, out.as_str()),
        (
            1,
            "ok.json:1:1: error[schema] {} is not of type \"string\" (at /)\n"
        )
    );
}

#[test]
fn schema_errors_in_a_large_file() {
    // 100,000 records, every 10,000th with a string port: about 3 MB.
    let mut text = String::from("[\n");
    for i in 0..100_000 {
        let port = if i % 10_000 == 0 {
            "\"80\"".to_owned()
        } else {
            i.to_string()
        };
        let _ = writeln!(text, "  {{\"name\": \"n{i}\", \"port\": {port}}},");
    }
    text.push_str("  {\"name\": \"last\"}\n]\n");
    let schema = "{\"items\": {\"properties\": {\"port\": {\"type\": \"integer\"}}}}";
    let dir = workdir(&[
        ("schema.json", schema),
        ("object.json", "{\"type\": \"object\"}"),
        ("big.json", &text),
    ]);
    let (code, out, _) = jsonvet_stderr(&dir, &["--schema", "schema.json", "big.json"]);
    assert_eq!(code, 1);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 10);
    assert_eq!(
        lines[0],
        "big.json:2:26: error[schema] \"80\" is not of type \"integer\" (at /0/port)"
    );
    assert_eq!(
        lines[9],
        "big.json:90002:30: error[schema] \"80\" is not of type \"integer\" (at /90000/port)"
    );
    // The message names a large value instead of quoting all of it.
    assert_eq!(
        jsonvet_stderr(&dir, &["--schema", "object.json", "big.json"]).1,
        "big.json:1:1: error[schema] value is not of type \"object\" (at /)\n"
    );
}

#[test]
fn a_deep_file_with_long_keys_is_placed_quickly() {
    // 9,000 levels with 100-byte keys: a JSON Pointer per value would hold about 4 GB.
    let key = "k".repeat(100);
    let mut text = String::new();
    for i in 0..9_000 {
        let _ = write!(text, "{{\"{key}{i}\":");
    }
    text.push('0');
    text.push_str(&"}".repeat(9_000));
    let dir = workdir(&[
        ("schema.json", "{\"type\": \"string\"}"),
        ("deep.json", &text),
    ]);
    let start = std::time::Instant::now();
    let (code, out, _) = jsonvet_stderr(&dir, &["--schema", "schema.json", "deep.json"]);
    assert!(start.elapsed().as_secs() < 5, "took {:?}", start.elapsed());
    assert_eq!(code, 1);
    assert_eq!(
        out,
        "deep.json:1:1: error[schema] value is not of type \"string\" (at /)\n"
    );
}

#[test]
fn a_deep_schema_is_a_usage_error() {
    let schema = format!("{}{{}}{}", "{\"not\":".repeat(5_000), "}".repeat(5_000));
    let dir = workdir(&[("schema.json", &schema), ("ok.json", "{}")]);
    let (code, out, err) = jsonvet_stderr(&dir, &["--schema", "schema.json", "ok.json"]);
    assert_eq!((code, out.as_str()), (2, ""));
    assert_eq!(
        err,
        "jsonvet: the schema schema.json:1:897: nested deeper than 128 levels\n"
    );
}

#[test]
fn a_backtracking_pattern_is_cut_short_and_reported() {
    let strings: Vec<String> = (0..500)
        .map(|i| format!("\"{}!{i}\"", "a".repeat(30)))
        .collect();
    let dir = workdir(&[
        ("schema.json", r#"{"items": {"pattern": "^(a|a)*\\1b"}}"#),
        ("r.json", &format!("[{}]", strings.join(", "))),
    ]);
    let start = std::time::Instant::now();
    let (code, out, _) = jsonvet_stderr(&dir, &["--schema", "schema.json", "r.json"]);
    assert!(start.elapsed().as_secs() < 5, "took {:?}", start.elapsed());
    assert_eq!(code, 1);
    // At most 100 errors per document, then a count of the rest.
    assert_eq!(out.lines().count(), 101);
    assert!(
        out.ends_with(
            "r.json:1:1: error[schema] ... and 400 more schema errors in this document\n"
        )
    );
    assert!(out.starts_with(
        "r.json:1:2: error[schema] Error executing regex: Max limit for backtracking count exceeded (at /0)\n"
    ));
}

#[test]
fn a_schema_value_json_schema_cannot_hold_is_a_usage_error() {
    let dir = workdir(&[
        ("nan.json5", "{enum: [NaN, 1]}"),
        (
            "keys.json",
            r#"{"properties": {"\ud800": true, "\ud801": false}}"#,
        ),
        ("null.json", "null"),
    ]);
    let cases = [
        (
            "nan.json5",
            "jsonvet: the schema nan.json5:1:9: NaN, Infinity and numbers beyond a double's range have no JSON Schema value\n",
        ),
        (
            "keys.json",
            "jsonvet: the schema keys.json:1:33: this key and an earlier one in the object are the same once lone surrogates become U+FFFD\n",
        ),
    ];
    for (schema, message) in cases {
        let (code, out, err) = jsonvet_stderr(&dir, &["--schema", schema, "null.json"]);
        assert_eq!((code, out.as_str(), err.as_str()), (2, "", message));
    }
}

#[test]
fn keys_that_differ_only_in_lone_surrogates_skip_the_document() {
    let dir = workdir(&[
        (
            "schema.json",
            r#"{"additionalProperties": {"type": "string"}}"#,
        ),
        ("s.json", r#"{"\ud800": 1, "\ud801": "s"}"#),
    ]);
    let (code, out, _) = jsonvet_stderr(&dir, &["--schema", "schema.json", "s.json"]);
    assert_eq!(code, 1);
    assert_eq!(
        out,
        "s.json:1:15: error[schema] this key and an earlier one in the object are the same once \
         lone surrogates become U+FFFD, so this document is not checked against the schema\n"
    );
}

#[test]
fn a_message_listing_many_keys_is_cut_short() {
    let members: Vec<String> = (0..1_000).map(|i| format!("\"k{i}\": {i}")).collect();
    let dir = workdir(&[
        (
            "schema.json",
            r#"{"properties": {"a": true}, "additionalProperties": false}"#,
        ),
        ("many.json", &format!("{{{}}}", members.join(", "))),
    ]);
    let (code, out, _) = jsonvet_stderr(&dir, &["--schema", "schema.json", "many.json"]);
    assert_eq!(code, 1);
    assert_eq!(out.lines().count(), 1);
    assert!(out.len() < 300, "{out}");
    assert!(
        out.starts_with(
            "many.json:1:2: error[schema] Additional properties are not allowed ('k0', 'k1',"
        ),
        "{out}"
    );
    assert!(out.ends_with("... (at /)\n"), "{out}");
}

#[test]
fn control_characters_in_keys_are_escaped_in_the_output() {
    let dir = workdir(&[
        (
            "schema.json",
            r#"{"additionalProperties": {"type": "string"}}"#,
        ),
        ("k.json", r#"{"x\u001b]0;t\u0007\r\n::error::y": 1}"#),
    ]);
    let (code, out, _) = jsonvet_stderr(&dir, &["--schema", "schema.json", "k.json"]);
    assert_eq!(code, 1);
    assert_eq!(
        out,
        "k.json:1:37: error[schema] 1 is not of type \"string\" (at /x\\u001b]0;t\\u0007\\r\\n::error::y)\n"
    );
}

#[test]
fn the_schema_rule_cannot_be_reconfigured() {
    let dir = workdir(&[
        ("schema.json", "{}"),
        ("ok.json", "{}"),
        ("jsonvet.toml", "[rules]\nschema = \"warn\"\n"),
    ]);
    for flag in ["--error", "--warn", "--ignore"] {
        let (code, out, err) = jsonvet_stderr(
            &dir,
            &[flag, "schema", "--schema", "schema.json", "ok.json"],
        );
        assert_eq!((code, out.as_str()), (2, ""), "{flag}");
        assert!(
            err.starts_with("jsonvet: unknown rule `schema`;"),
            "{flag}: {err}"
        );
    }
    let (code, _, err) = jsonvet_stderr(&dir, &["--schema", "schema.json", "ok.json"]);
    assert_eq!(code, 2);
    assert!(
        err.contains("jsonvet.toml: unknown rule `schema`;"),
        "{err}"
    );
}
