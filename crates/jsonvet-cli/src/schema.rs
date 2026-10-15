//! `--schema`: each file's values checked against one JSON Schema, every error placed at the
//! value it is about.

use std::collections::HashMap;
use std::ops::Range;
use std::path::Path;

use jsonschema::error::ValidationErrorKind;
use jsonschema::{ReferencingError, ValidationError, Validator};
use jsonvet::{Event, EventKind, Severity};
use serde_json::{Map, Number, Value};

use crate::settings::Settings;

/// One document's values. With positions asked for, also the offset of each value and, for an
/// object member, its key, by JSON Pointer.
// shortcut: one pointer string per value, held for the whole document, and only built for a
// document with errors; walk the events for each error's pointer instead if memory matters.
struct Document {
    value: Value,
    /// The document's events, as a range of the slice it was folded from.
    events: Range<usize>,
    positions: HashMap<String, (usize, Option<usize>)>,
    /// The first number JSON Schema has no value for: `NaN`, `Infinity` or one past a double.
    not_a_number: Option<usize>,
}

/// Past this many bytes, a message says "value" instead of quoting the value.
const MAX_MESSAGE: usize = 200;

/// A compiled schema.
pub(crate) struct Schema(Validator);

impl Schema {
    /// Read and compile the schema at `path`, in its dialect under `settings`. Any problem
    /// with it is a usage error.
    pub(crate) fn load(path: &Path, settings: &Settings) -> Result<Schema, String> {
        let name = path.to_string_lossy();
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read the schema {name}: {e}"))?;
        let (report, events) = jsonvet::parse(&text, &settings.config_for(&name));
        if let Some(problem) = report
            .problems()
            .iter()
            .find(|p| p.severity() == Severity::Error)
        {
            let at = report.location(problem.offset());
            return Err(format!(
                "the schema {name}:{}:{}: {problem}",
                at.line, at.column
            ));
        }
        let mut documents = documents(&events.unwrap_or_default(), true);
        if documents.len() != 1 {
            return Err(format!("the schema {name} must hold one value"));
        }
        let document = documents.swap_remove(0);
        // Offline: a `$ref` to anything outside the schema file fails here.
        let error = match jsonschema::options().offline().build(&document.value) {
            Ok(validator) => return Ok(Schema(validator)),
            Err(error) => error,
        };
        // A failed `$ref` has no instance path to place it by.
        if let ValidationErrorKind::Referencing(ReferencingError::Unretrievable { uri, .. }) =
            error.kind()
        {
            return Err(format!(
                "the schema {name} has a `$ref` to {uri}, outside the file; jsonvet fetches no \
                 schemas"
            ));
        }
        let at = report.location(offset(&document, &error));
        Err(format!(
            "the schema {name}:{}:{}: {error}",
            at.line, at.column
        ))
    }

    /// The schema errors in one file's values, as (offset, message), in document order.
    pub(crate) fn errors(&self, events: &[Event<'_>]) -> Vec<(usize, String)> {
        let mut errors = Vec::new();
        for document in documents(events, false) {
            if let Some(offset) = document.not_a_number {
                errors.push((
                    offset,
                    "NaN, Infinity and numbers beyond a double's range have no JSON Schema \
                     value, so this document is not checked against the schema"
                        .to_owned(),
                ));
                continue;
            }
            let mut found = self.0.iter_errors(&document.value).peekable();
            if found.peek().is_none() {
                continue;
            }
            // Positions cost more than the check itself, so only a document with errors
            // is folded again to find them.
            let placed = documents(&events[document.events], true);
            let Some(placed) = placed.first() else {
                continue;
            };
            for error in found {
                let pointer = error.instance_path().as_str();
                let at = if pointer.is_empty() { "/" } else { pointer };
                // A message quotes the value it is about, which can be the whole file.
                let mut message = error.to_string();
                if message.len() > MAX_MESSAGE {
                    message = error.masked().to_string();
                }
                errors.push((offset(placed, &error), format!("{message} (at {at})")));
            }
        }
        // Stable, so errors at one value keep the schema's order.
        errors.sort_by_key(|(offset, _)| *offset);
        errors
    }
}

/// Where an error is: its value, or for unexpected properties, the first such key.
fn offset(document: &Document, error: &ValidationError<'_>) -> usize {
    let pointer = error.instance_path().as_str();
    let (mut offset, _) = document
        .positions
        .get(pointer)
        .copied()
        .unwrap_or((0, None));
    if let ValidationErrorKind::AdditionalProperties { unexpected }
    | ValidationErrorKind::UnevaluatedProperties { unexpected } = error.kind()
    {
        let keys = unexpected.iter().filter_map(|key| {
            let child = format!("{pointer}/{}", escaped(key));
            document.positions.get(&child).and_then(|(_, key)| *key)
        });
        if let Some(first) = keys.min() {
            offset = first;
        }
    }
    offset
}

/// A key as a JSON Pointer segment.
fn escaped(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

/// An open container while the events are folded into values.
struct Open {
    value: Value,
    pointer: String,
    /// The current member's key and its offset, in an object.
    key: Option<(String, usize)>,
}

/// Fold events into documents: one, or one per line in JSON Lines.
fn documents(events: &[Event<'_>], with_positions: bool) -> Vec<Document> {
    let mut documents = Vec::new();
    let mut first_event = 0;
    let mut stack: Vec<Open> = Vec::new();
    let mut positions = HashMap::new();
    let mut not_a_number = None;
    for (i, event) in events.iter().enumerate() {
        let value = match &event.kind {
            EventKind::Key(key) => {
                if let Some(open) = stack.last_mut() {
                    open.key = Some((key.to_string(), event.offset));
                }
                continue;
            }
            EventKind::End => match stack.pop() {
                Some(open) => open.value,
                None => continue,
            },
            kind => {
                let mut pointer = String::new();
                if with_positions {
                    let key_offset;
                    (pointer, key_offset) = child(&stack);
                    positions.insert(pointer.clone(), (event.offset, key_offset));
                }
                match kind {
                    EventKind::ObjectStart | EventKind::ArrayStart => {
                        let value = if *kind == EventKind::ObjectStart {
                            Value::Object(Map::new())
                        } else {
                            Value::Array(Vec::new())
                        };
                        stack.push(Open {
                            value,
                            pointer,
                            key: None,
                        });
                        continue;
                    }
                    EventKind::Null => Value::Null,
                    EventKind::Bool(b) => Value::Bool(*b),
                    EventKind::String(s) => Value::String(s.to_string()),
                    EventKind::Number(text) => number(text).unwrap_or_else(|| {
                        not_a_number.get_or_insert(event.offset);
                        Value::Null
                    }),
                    _ => continue,
                }
            }
        };
        match stack.last_mut() {
            None => documents.push(Document {
                value,
                events: std::mem::replace(&mut first_event, i + 1)..i + 1,
                positions: std::mem::take(&mut positions),
                not_a_number: not_a_number.take(),
            }),
            Some(Open {
                value: Value::Array(items),
                ..
            }) => items.push(value),
            Some(Open {
                value: Value::Object(members),
                key,
                ..
            }) => {
                if let Some((key, _)) = key.take() {
                    members.insert(key, value);
                }
            }
            Some(_) => {}
        }
    }
    documents
}

/// The JSON Pointer of the next value in the innermost open container, and its key's offset.
fn child(stack: &[Open]) -> (String, Option<usize>) {
    match stack.last() {
        None => (String::new(), None),
        Some(Open {
            value: Value::Array(items),
            pointer,
            ..
        }) => (format!("{pointer}/{}", items.len()), None),
        Some(Open {
            pointer,
            key: Some((key, offset)),
            ..
        }) => (format!("{pointer}/{}", escaped(key)), Some(*offset)),
        Some(open) => (open.pointer.clone(), None),
    }
}

/// A number as written in any dialect, or `None` for `NaN`, `Infinity` and numbers past a
/// double's range. An integer too large for 64 bits becomes a double, as `serde_json` reads one.
fn number(text: &str) -> Option<Value> {
    let (negative, unsigned) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let hex = unsigned
        .strip_prefix("0x")
        .or_else(|| unsigned.strip_prefix("0X"));
    let integer = hex.is_some() || !unsigned.contains(['.', 'e', 'E']);
    let exact = match hex {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None if integer => unsigned.parse::<u64>().ok(),
        None => None,
    };
    let exact = exact.and_then(|n| {
        if negative {
            i64::try_from(n).ok().map(|n| Number::from(-n))
        } else {
            Some(Number::from(n))
        }
    });
    if let Some(n) = exact {
        return Some(Value::Number(n));
    }
    let magnitude = match hex {
        Some(hex) => hex.chars().fold(0.0, |n, d| {
            n * 16.0 + f64::from(d.to_digit(16).unwrap_or_default())
        }),
        // `f64::from_str` takes `.5` and `5.` too, and reads the words `NaN` and `Infinity`.
        None => unsigned.parse().ok()?,
    };
    Number::from_f64(if negative { -magnitude } else { magnitude }).map(Value::Number)
}

#[cfg(test)]
mod tests {
    use super::{documents, number};
    use jsonvet::{Config, Dialect};
    use serde_json::json;

    #[test]
    fn events_fold_into_the_values_serde_json_reads() {
        let text =
            r#"{"a": [1, -2.5e3, true, null, {}], "b\u00e9/~": "x\n\ud83d\ude00", "a": "last"}"#;
        let (_, events) = jsonvet::parse(text, &Config::new(Dialect::Json));
        let documents = documents(&events.unwrap_or_default(), true);
        let expected: serde_json::Value = serde_json::from_str(text).unwrap_or_default();
        assert_eq!(documents.len(), 1);
        assert_eq!(documents[0].value, expected);
        assert_eq!(documents[0].positions["/b\u{e9}~1~0"], (48, Some(35)));
        assert_eq!(documents[0].positions["/a"], (72, Some(67)));
    }

    #[test]
    fn numbers_in_every_dialect_become_json_numbers() {
        let cases = [
            ("0", json!(0)),
            ("-7", json!(-7)),
            ("+7", json!(7)),
            (
                "18446744073709551615",
                json!(18_446_744_073_709_551_615_u64),
            ),
            ("1.5e3", json!(1500.0)),
            (".5", json!(0.5)),
            ("5.", json!(5.0)),
            ("-0x1F", json!(-31)),
            ("0XfF", json!(255)),
            ("0x10000000000000000", json!(18_446_744_073_709_551_616.0)),
        ];
        for (text, expected) in cases {
            assert_eq!(number(text), Some(expected), "{text}");
        }
        assert_eq!(number("NaN"), None);
        assert_eq!(number("-Infinity"), None);
        assert_eq!(number("1e999"), None);
    }
}
