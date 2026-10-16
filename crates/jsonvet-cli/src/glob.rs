//! File patterns, matched as check-json5 1.4 matched them: a pattern without `/` matches the
//! file name at any depth, one with `/` matches the whole path, and `*` also matches `/`.

/// Whether `pattern` matches `path`.
pub(crate) fn matches(pattern: &str, path: &str) -> bool {
    let path = normalize(path);
    let target = if pattern.contains('/') {
        path.as_str()
    } else {
        path.rsplit('/').next().unwrap_or_default()
    };
    let pattern: Vec<char> = pattern.chars().collect();
    let target: Vec<char> = target.chars().collect();
    wildcard(&pattern, &target)
}

/// `path` relative to the directory `base`, both absolute, or `None` when `path` is not
/// inside `base`.
pub(crate) fn relative(path: &str, base: &str) -> Option<String> {
    let (path, base) = (normalize(path), normalize(base));
    let rest = path.strip_prefix(base.trim_end_matches('/'))?;
    rest.strip_prefix('/').map(str::to_owned)
}

/// Collapse `.`, `..` and repeated separators, and use `/` throughout, as Python's
/// `os.path.normpath` does, so `./sub/a.json` matches `sub/*.json`.
fn normalize(path: &str) -> String {
    let path = path.replace('\\', "/");
    let absolute = path.starts_with('/');
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." if parts.last().is_some_and(|p| *p != "..") => {
                parts.pop();
            }
            ".." if absolute => {}
            _ => parts.push(part),
        }
    }
    let joined = parts.join("/");
    match (absolute, joined.is_empty()) {
        (true, _) => format!("/{joined}"),
        (false, true) => ".".to_owned(),
        (false, false) => joined,
    }
}

/// `fnmatchcase`: `*` any run, `?` one character, `[abc]`, `[a-z]` and `[!abc]` classes. An
/// unclosed `[` is a literal. Backtracks to the last `*` only, so it runs in linear time per
/// `*` instead of exponential time.
fn wildcard(pattern: &[char], text: &[char]) -> bool {
    let (mut p, mut t) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        if p < pattern.len() && pattern[p] == '*' {
            star = Some((p, t));
            p += 1;
            continue;
        }
        if let Some((true, next)) = one(&pattern[p..], text[t]) {
            p += next;
            t += 1;
            continue;
        }
        match star {
            Some((star_p, star_t)) => {
                p = star_p + 1;
                t = star_t + 1;
                star = Some((star_p, star_t + 1));
            }
            None => return false,
        }
    }
    pattern[p..].iter().all(|&c| c == '*')
}

/// Match one pattern element against one character: whether it matched, and how many pattern
/// characters it used. `None` when the element is `*`, which the caller handles.
fn one(pattern: &[char], c: char) -> Option<(bool, usize)> {
    match pattern.first()? {
        '*' => None,
        '?' => Some((true, 1)),
        '[' => match class_end(pattern) {
            Some(end) => Some((in_class(&pattern[1..end], c), end + 1)),
            None => Some((c == '[', 1)),
        },
        &literal => Some((c == literal, 1)),
    }
}

/// The index of the `]` closing the class that starts at `pattern[0]`, if there is one.
fn class_end(pattern: &[char]) -> Option<usize> {
    let mut i = 1;
    if pattern.get(i) == Some(&'!') {
        i += 1;
    }
    // A `]` right after `[` or `[!` belongs to the class.
    if pattern.get(i) == Some(&']') {
        i += 1;
    }
    (i..pattern.len()).find(|&j| pattern[j] == ']')
}

fn in_class(class: &[char], c: char) -> bool {
    let (negated, class) = match class.split_first() {
        Some(('!', rest)) => (true, rest),
        _ => (false, class),
    };
    let mut found = false;
    let mut i = 0;
    while i < class.len() {
        if i + 2 < class.len() && class[i + 1] == '-' {
            found |= class[i] <= c && c <= class[i + 2];
            i += 3;
        } else {
            found |= class[i] == c;
            i += 1;
        }
    }
    found != negated
}

#[cfg(test)]
mod tests {
    use super::{matches, normalize, relative, wildcard};

    #[test]
    fn patterns_match_as_check_json5_matched_them() {
        assert!(matches("*.json", "a/b/x.json"));
        assert!(matches("package.json", "a/package.json"));
        assert!(matches("src/*.json", "src/x.json"));
        assert!(matches("src/*.json", "src/deep/x.json"), "`*` crosses `/`");
        assert!(!matches("src/*.json", "lib/src/x.json"));
        assert!(matches("sub/*.json", "./sub/a.json"));
        assert!(!matches("*.json", "A.JSON"), "case-sensitive");
        assert!(matches("tsconfig*.json", "tsconfig.build.json"));
        assert!(matches("[ab].json", "b.json"));
        assert!(!matches("[!ab].json", "b.json"));
        assert!(matches("[a-c]?.json", "bz.json"));
        assert!(matches("[.json", "[.json"), "an unclosed `[` is a literal");
        assert!(matches(".vscode/*", ".vscode/settings.json"));
    }

    #[test]
    fn wildcards_agree_with_fnmatchcase() {
        // Expected values from Python's `fnmatch.fnmatchcase(text, pattern)`.
        for (pattern, text, expected) in [
            ("a", "ab", false),
            ("*b", "aab", true),
            ("*.json", "a.b.json", true),
            ("a*b*c", "axbxbyc", true),
            ("a*b*c", "axbxby", false),
            ("[!]].json", "].json", false),
            ("[!]].json", "x.json", true),
            ("[]a].json", "].json", true),
            ("[]a].json", "b.json", false),
            ("[a-].json", "-.json", true),
            ("[a-].json", "b.json", false),
            ("[-a].json", "-.json", true),
            ("[a-cx].json", "x.json", true),
            ("[a-cx].json", "d.json", false),
            ("[c-a].json", "b.json", false),
            ("[!a-c].json", "d.json", true),
            ("?", "", false),
        ] {
            let chars = |s: &str| s.chars().collect::<Vec<_>>();
            assert_eq!(
                wildcard(&chars(pattern), &chars(text)),
                expected,
                "{pattern} on {text}"
            );
        }
    }

    #[test]
    fn paths_relative_to_a_directory() {
        assert_eq!(
            relative("/r/data/x.json", "/r").as_deref(),
            Some("data/x.json")
        );
        assert_eq!(
            relative("/r/sub/../x.json", "/r/").as_deref(),
            Some("x.json")
        );
        assert_eq!(relative("/a.json", "/").as_deref(), Some("a.json"));
        assert_eq!(
            relative("/rx/a.json", "/r"),
            None,
            "a sibling with a longer name"
        );
        assert_eq!(relative("/other/a.json", "/r"), None);
        assert_eq!(
            relative("C:\\r\\a.json", "C:\\r").as_deref(),
            Some("a.json")
        );
    }

    #[test]
    fn paths_normalize_like_os_path_normpath() {
        assert_eq!(normalize("./a//b/./c"), "a/b/c");
        assert_eq!(normalize("a/../b"), "b");
        assert_eq!(normalize("../a"), "../a");
        assert_eq!(normalize("/x/../../y"), "/y");
        assert_eq!(normalize("."), ".");
    }
}
