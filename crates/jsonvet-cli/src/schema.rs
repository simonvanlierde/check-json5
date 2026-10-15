//! `--schema`: each file's values checked against one JSON Schema, every error placed at the
//! value it is about.

use std::collections::HashMap;
use std::ops::Range;
use std::path::Path;

use jsonschema::error::ValidationErrorKind;
use jsonschema::{PatternOptions, ReferencingError, ValidationError, Validator};
use jsonvet::{Event, EventKind, Severity};
use serde_json::{Map, Number, Value};

use crate::settings::Settings;

/// One document's values, and the range of the events they were folded from.
struct Document {
    value: Value,
    events: Range<usize>,
    /// Why the values are not the text's, and where: the first such number or key.
    unchecked: Option<(usize, &'static str)>,
}

const NOT_A_NUMBER: &str =
    "NaN, Infinity and numbers beyond a double's range have no JSON Schema value";
// shortcut: two copies of one key holding U+FFFD or a lone surrogate also land here, though they
// were already the same; tell them apart by the key's source text if that is ever reported.
const SAME_KEYS: &str =
    "this key and an earlier one in the object are the same once lone surrogates become U+FFFD";

/// How many schema errors one document reports, in the order jsonschema finds them; one more
/// line counts the rest.
const MAX_ERRORS: usize = 100;

/// Past this many bytes, a message says "value" instead of quoting the value.
const MAX_MESSAGE: usize = 200;

/// How deep a schema may nest; real schemas stay well under it.
const MAX_SCHEMA_DEPTH: usize = 128;

/// How many times one regex match may backtrack before it fails. About one step per character
/// for a lookaround over the whole string, so a string over a few thousand characters can hit it.
const BACKTRACK_LIMIT: usize = 5_000;

/// A compiled schema.
pub(crate) struct Schema(Validator);

impl Schema {
    /// Read and compile the schema at `path`, in its dialect under `settings`. Any problem
    /// with it is a usage error.
    pub(crate) fn load(path: &Path, settings: &Settings) -> Result<Schema, String> {
        let name = path.to_string_lossy();
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read the schema {name}: {e}"))?;
        // jsonschema compiles a schema recursively: a deep one can take gigabytes or the stack.
        let config = settings.config_for(&name).max_depth(MAX_SCHEMA_DEPTH);
        let (report, events) = jsonvet::parse(&text, &config);
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
        let events = events.unwrap_or_default();
        let mut documents = documents(&events);
        if documents.len() != 1 {
            return Err(format!("the schema {name} must hold one value"));
        }
        let document = documents.swap_remove(0);
        if let Some((offset, reason)) = document.unchecked {
            let at = report.location(offset);
            return Err(format!(
                "the schema {name}:{}:{}: {reason}",
                at.line, at.column
            ));
        }
        // Offline: a `$ref` to anything outside the schema file fails here.
        let options = jsonschema::options()
            .offline()
            // fancy-regex backtracks for back-references and lookaround, and its default limit
            // of a million lets one 30-byte string take 30 ms.
            .with_pattern_options(PatternOptions::fancy_regex().backtrack_limit(BACKTRACK_LIMIT));
        let error = match options.build(&document.value) {
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
        let at = report.location(locate(&events[document.events], std::slice::from_ref(&error))[0]);
        Err(format!(
            "the schema {name}:{}:{}: {error}",
            at.line, at.column
        ))
    }

    /// The schema errors in one file's values, as (offset, message), in document order within each
    /// document, which ends with a count of any errors past [`MAX_ERRORS`].
    pub(crate) fn errors(&self, events: &[Event<'_>]) -> Vec<(usize, String)> {
        let mut errors = Vec::new();
        for document in documents(events) {
            if let Some((offset, reason)) = document.unchecked {
                errors.push((
                    offset,
                    format!("{reason}, so this document is not checked against the schema"),
                ));
                continue;
            }
            let mut all = self.0.iter_errors(&document.value);
            let found: Vec<ValidationError<'_>> = all.by_ref().take(MAX_ERRORS).collect();
            if found.is_empty() {
                continue;
            }
            let events = &events[document.events];
            let mut placed: Vec<(usize, String)> = locate(events, &found)
                .into_iter()
                .zip(&found)
                .map(|(offset, error)| {
                    let pointer = error.instance_path().as_str();
                    let at = if pointer.is_empty() { "/" } else { pointer };
                    (offset, format!("{} (at {at})", message(error)))
                })
                .collect();
            // Stable, so errors at one value keep the schema's order.
            placed.sort_by_key(|(offset, _)| *offset);
            errors.append(&mut placed);
            let more = all.count();
            if more > 0 {
                errors.push((
                    events[0].offset,
                    format!("... and {more} more schema errors in this document"),
                ));
            }
        }
        errors
    }
}

/// An error's message, quoting the value it is about only when that is short, and cut short
/// itself when it lists many keys or values.
fn message(error: &ValidationError<'_>) -> String {
    let mut budget = MAX_MESSAGE;
    let mut message = String::new();
    if fits(error.instance(), &mut budget) {
        message = error.to_string();
    }
    if message.is_empty() || message.len() > MAX_MESSAGE {
        message = error.masked().to_string();
    }
    if message.len() > MAX_MESSAGE {
        let mut end = MAX_MESSAGE;
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        message.truncate(end);
        message.push_str("...");
    }
    message
}

/// Whether `value` as JSON takes about `budget` bytes or fewer, without writing it out. Each level
/// costs at least one byte, so this recurses no deeper than the budget.
fn fits(value: &Value, budget: &mut usize) -> bool {
    let cost = match value {
        Value::String(s) => s.len() + 2,
        Value::Array(_) | Value::Object(_) => 2,
        _ => 5,
    };
    *budget = budget.saturating_sub(cost);
    *budget > 0
        && match value {
            Value::Array(items) => items.iter().all(|item| fits(item, budget)),
            Value::Object(members) => members.iter().all(|(key, value)| {
                *budget = budget.saturating_sub(key.len() + 4);
                *budget > 0 && fits(value, budget)
            }),
            _ => true,
        }
}

/// The values some errors are about, as a tree of JSON Pointer segments, with the offset of
/// each value and, for an object member, of its key.
#[derive(Default)]
struct Wanted(Vec<Node>);

#[derive(Default)]
struct Node {
    children: HashMap<String, usize>,
    at: (usize, Option<usize>),
}

impl Wanted {
    /// The node for `key` under `node`, added if new.
    fn child(&mut self, node: usize, key: &str) -> usize {
        if let Some(&child) = self.0[node].children.get(key) {
            return child;
        }
        self.0.push(Node::default());
        let child = self.0.len() - 1;
        self.0[node].children.insert(key.to_owned(), child);
        child
    }
}

/// In an open container: an array's next index, or an object's current key and its offset.
type Slot<'e> = Result<usize, (&'e str, usize)>;

/// Where each error is, found in one walk over its document's events: its value; for unexpected
/// properties, the first such key; for a property name, that key.
fn locate(events: &[Event<'_>], errors: &[ValidationError<'_>]) -> Vec<usize> {
    let mut wanted = Wanted(vec![Node::default()]);
    // For each error, its value's node and the nodes of any keys it is about.
    let nodes: Vec<(usize, Vec<usize>)> = errors
        .iter()
        .map(|error| {
            let mut node = 0;
            for segment in error.instance_path().as_str().split('/').skip(1) {
                node = wanted.child(node, &segment.replace("~1", "/").replace("~0", "~"));
            }
            let keys = match error.kind() {
                ValidationErrorKind::AdditionalProperties { unexpected }
                | ValidationErrorKind::UnevaluatedProperties { unexpected } => unexpected
                    .iter()
                    .map(|key| wanted.child(node, key))
                    .collect(),
                ValidationErrorKind::PropertyNames { error } => error
                    .instance()
                    .as_str()
                    .map(|key| vec![wanted.child(node, key)])
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            (node, keys)
        })
        .collect();

    // Each open container: its node if any error is inside it, and its next index or key.
    let mut stack: Vec<(Option<usize>, Slot<'_>)> = Vec::new();
    for event in events {
        match (&event.kind, stack.last_mut()) {
            (EventKind::Key(key), Some((_, slot))) => {
                *slot = Err((key, event.offset));
                continue;
            }
            (EventKind::End, _) => {
                stack.pop();
                continue;
            }
            _ => {}
        }
        let node = match stack.last_mut() {
            None => Some((0, None)),
            Some((parent, Ok(index))) => {
                *index += 1;
                let key = *index - 1;
                parent
                    .and_then(|p| wanted.0[p].children.get(&key.to_string()))
                    .map(|&n| (n, None))
            }
            Some((parent, Err((key, offset)))) => parent
                .and_then(|p| wanted.0[p].children.get(*key))
                .map(|&n| (n, Some(*offset))),
        };
        if let Some((node, key)) = node {
            // A later duplicate key overwrites, as it does in the folded value.
            wanted.0[node].at = (event.offset, key);
        }
        let node = node
            .map(|(n, _)| n)
            .filter(|&n| !wanted.0[n].children.is_empty());
        match event.kind {
            EventKind::ObjectStart => stack.push((node, Err(("", 0)))),
            EventKind::ArrayStart => stack.push((node, Ok(0))),
            _ => {}
        }
    }

    nodes
        .into_iter()
        .map(|(node, keys)| {
            keys.iter()
                .filter_map(|&key| wanted.0[key].at.1)
                .min()
                .unwrap_or(wanted.0[node].at.0)
        })
        .collect()
}

/// An open container while the events are folded into values, with an object's current key.
enum Open {
    Array(Vec<Value>),
    Object(Map<String, Value>, (String, usize)),
}

/// Fold events into documents: one, or one per line in JSON Lines.
fn documents(events: &[Event<'_>]) -> Vec<Document> {
    let mut documents = Vec::new();
    let mut first_event = 0;
    let mut stack: Vec<Open> = Vec::new();
    let mut unchecked = None;
    for (i, event) in events.iter().enumerate() {
        let value = match &event.kind {
            EventKind::Key(key) => {
                if let Some(Open::Object(_, current)) = stack.last_mut() {
                    *current = (key.to_string(), event.offset);
                }
                continue;
            }
            EventKind::ObjectStart => {
                stack.push(Open::Object(Map::new(), (String::new(), 0)));
                continue;
            }
            EventKind::ArrayStart => {
                stack.push(Open::Array(Vec::new()));
                continue;
            }
            EventKind::End => match stack.pop() {
                Some(Open::Array(items)) => Value::Array(items),
                Some(Open::Object(members, _)) => Value::Object(members),
                None => continue,
            },
            EventKind::Null => Value::Null,
            EventKind::Bool(b) => Value::Bool(*b),
            EventKind::String(s) => Value::String(s.to_string()),
            EventKind::Number(text) => number(text).unwrap_or_else(|| {
                unchecked.get_or_insert((event.offset, NOT_A_NUMBER));
                Value::Null
            }),
            _ => continue,
        };
        match stack.last_mut() {
            None => documents.push(Document {
                value,
                events: std::mem::replace(&mut first_event, i + 1)..i + 1,
                unchecked: unchecked.take(),
            }),
            Some(Open::Array(items)) => items.push(value),
            Some(Open::Object(members, (key, offset))) => {
                let key = std::mem::take(key);
                // Real duplicates are the duplicate-key rule's; lone surrogates are not.
                if key.contains('\u{FFFD}') && members.contains_key(&key) {
                    unchecked.get_or_insert((*offset, SAME_KEYS));
                }
                members.insert(key, value);
            }
        }
    }
    documents
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
    use super::{documents, locate, number};
    use jsonvet::{Config, Dialect};
    use proptest::prelude::*;
    use serde_json::{Value, json};

    #[test]
    fn events_fold_into_the_values_serde_json_reads() {
        let text = r#"{"a": [1, -2.5e3, true, null, {"b": [false, "c"]}], "b\u00e9/~": "x\n\ud83d\ude00"}"#;
        let (_, events) = jsonvet::parse(text, &Config::new(Dialect::Json));
        let documents = documents(&events.unwrap_or_default());
        let expected: serde_json::Value = serde_json::from_str(text).unwrap_or_default();
        assert_eq!(documents.len(), 1);
        assert_eq!(documents[0].value, expected);
    }

    #[test]
    fn a_later_duplicate_key_wins_in_the_value_and_its_position() {
        let text = r#"{"a": [1, {"b": 2}], "b\u00e9/~": "x", "a": "last"}"#;
        let (_, events) = jsonvet::parse(text, &Config::new(Dialect::Json));
        let events = events.unwrap_or_default();
        let documents = documents(&events);
        assert_eq!(documents[0].value, json!({"a": "last", "b\u{e9}/~": "x"}));
        let schema =
            json!({"properties": {"a": {"type": "integer"}, "b\u{e9}/~": {"type": "integer"}}});
        let validator = jsonschema::validator_for(&schema).expect("a schema");
        let errors: Vec<_> = validator.iter_errors(&documents[0].value).collect();
        let pointers: Vec<_> = errors.iter().map(|e| e.instance_path().as_str()).collect();
        assert_eq!(pointers, ["/a", "/b\u{e9}~1~0"]);
        assert_eq!(locate(&events, &errors), [44, 34]);
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

    /// Any JSON value, with numbers serde_json reads exactly without its `float_roundtrip`
    /// feature, and strings that need escapes.
    fn any_json() -> impl Strategy<Value = Value> {
        let leaf = prop_oneof![
            Just(Value::Null),
            any::<bool>().prop_map(Value::Bool),
            any::<i64>().prop_map(Value::from),
            any::<u64>().prop_map(Value::from),
            any::<i32>().prop_map(|n| Value::from(f64::from(n) / 64.0)),
            "\\PC{0,6}|[\\x00-\\x1f\"\\\\/]{0,3}".prop_map(Value::String),
        ];
        leaf.prop_recursive(4, 32, 4, |inner| {
            prop_oneof![
                prop::collection::vec(inner.clone(), 0..4).prop_map(Value::Array),
                prop::collection::vec(("\\PC{0,3}", inner), 0..4)
                    .prop_map(|members| Value::Object(members.into_iter().collect())),
            ]
        })
    }

    proptest! {
        #[test]
        fn strict_json_folds_into_what_serde_json_reads(value in any_json(), pretty in any::<bool>()) {
            let text = if pretty {
                serde_json::to_string_pretty(&value)
            } else {
                serde_json::to_string(&value)
            }
            .unwrap_or_default();
            let (_, events) = jsonvet::parse(&text, &Config::new(Dialect::Json));
            prop_assert!(events.is_some(), "{}", text);
            let documents = documents(&events.unwrap_or_default());
            let expected: Value = serde_json::from_str(&text).unwrap_or_default();
            prop_assert_eq!(documents.len(), 1);
            prop_assert_eq!(&documents[0].value, &expected);
        }
    }
}
