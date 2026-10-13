//! Conformance driver for the Rust parsers. Usage: conformance-driver <parser> < paths
//! Prints "ready", then "<path>\t<accept|reject|crash>" per path. See conformance/README.md.

use std::io::{BufRead, Write};
use std::panic;

type Value = serde_json::Value;

/// Parses bytes; `false` means the parser reported an error.
fn parser(name: &str) -> Option<fn(&[u8]) -> bool> {
    Some(match name {
        "rs-serde-json" => |b| serde_json::from_slice::<Value>(b).is_ok(),
        // The usual way to read a JSON Lines stream in Rust. It splits on whitespace, not lines.
        "rs-serde-json-stream" => |b| {
            serde_json::Deserializer::from_slice(b)
                .into_iter::<Value>()
                .all(|v| v.is_ok())
        },
        // The next three take text, so a file that is not UTF-8 counts as a reject.
        "rs-json5" => |b| str::from_utf8(b).is_ok_and(|t| json5::from_str::<Value>(t).is_ok()),
        "rs-serde-json5" => {
            |b| str::from_utf8(b).is_ok_and(|t| serde_json5::from_str::<Value>(t).is_ok())
        }
        // Ok(None), for a file with no value, is still no error, so it counts as an accept.
        "rs-jsonc-parser" => |b| {
            str::from_utf8(b).is_ok_and(|t| {
                jsonc_parser::parse_to_value(t, &jsonc_parser::ParseOptions::default()).is_ok()
            })
        },
        _ => return None,
    })
}

fn main() {
    let name = std::env::args().nth(1).unwrap_or_default();
    let Some(parse) = parser(&name) else {
        eprintln!("unknown parser: {name}");
        std::process::exit(2);
    };
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "ready");
    let _ = out.flush();
    for path in std::io::stdin().lock().lines().map_while(Result::ok) {
        if path.is_empty() {
            continue;
        }
        let verdict = match std::fs::read(&path) {
            Err(_) => "crash",
            Ok(bytes) => match panic::catch_unwind(|| parse(&bytes)) {
                Ok(true) => "accept",
                Ok(false) => "reject",
                Err(_) => "crash",
            },
        };
        let _ = writeln!(out, "{path}\t{verdict}");
        let _ = out.flush();
    }
}
