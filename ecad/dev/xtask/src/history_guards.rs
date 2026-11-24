//! Comments in `src/` say what the code does and why, in domain words. Where a piece of code came from (a brief,
//! design, lane, review finding or pull request number, a section mark, who decided it and when) lives in git and `docs/`:
//! such a citation goes stale as soon as the next change lands, and the reader cannot open most of them.
//!
//! This guard reads the comment text (`//`, `///`, `//!`, and a `//` comment after code) of every `.rs` file under
//! the `src/` of each workspace member, and the `#` comments of the built-in data under its `defaults/` (`.toml`),
//! and reports each line [`citation`] recognises. A hard zero: rewrite the
//! comment to say what is true now; there is no allowlist.
//!
//! Textual and hand-written like the other guards (xtask has no regex crate). Domain text that looks similar is
//! not a citation: a jumper bar lane (`lane 0`), a jumper bar tag (`JB-10`), a PLC address (`%Q2.2`), a number
//! with a thousands space (`10 000`) or a decimal (`0.005`), a timestamp (`2026-11-12T14:30:05Z`), a date in a
//! backticked example (a revision's date).

use std::path::PathBuf;

use serde_json::Value;

/// The prefixes of the old briefs' requirement ids, always written in capitals and followed by a dash and a digit.
const REQUIREMENT_IDS: &[&str] = &["DOC", "PROJ", "SAVE", "FILE", "TX", "UNDO", "LIB", "EEP"];

/// The words that follow `owner` (or `owner's`) when a comment cites a decision.
const OWNER_WORDS: &[&str] =
    &["request", "answer", "rule", "ruling", "decision", "trial", "look", "choice", "approval", "approved", "asked", "said", "check"];

/// Whether `c` continues a word.
fn word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// `rest` starts with `p`, ignoring ASCII case.
fn starts_ci(rest: &str, p: &str) -> bool {
    rest.get(..p.len()).is_some_and(|h| h.eq_ignore_ascii_case(p))
}

/// The byte at `k` of `s` is an ASCII digit.
fn digit_at(s: &str, k: usize) -> bool {
    s.as_bytes().get(k).is_some_and(u8::is_ascii_digit)
}

/// The character at byte `k` of `s` does not continue a word (or `s` ends there).
fn ends_at(s: &str, k: usize) -> bool {
    s.get(k..).and_then(|t| t.chars().next()).is_none_or(|c| !word_char(c))
}

/// `rest` starts with one of `words` (ignoring ASCII case) followed by a space: the length up to after the space.
fn word_then_space(rest: &str, words: &[&str]) -> Option<usize> {
    words.iter().find(|w| starts_ci(rest, w) && rest.as_bytes().get(w.len()) == Some(&b' ')).map(|w| w.len() + 1)
}

/// The kind of history citation `text` (a comment's text) holds, if any.
pub fn citation(text: &str) -> Option<&'static str> {
    if text.contains('§') {
        return Some("a section mark");
    }
    let mut prev: Option<char> = None;
    let mut prev2: Option<char> = None;
    let mut in_code = false;
    for (i, c) in text.char_indices() {
        in_code ^= c == '`';
        let rest = &text[i..];
        if c == '(' && (starts_ci(&rest[1..], "owner") && ends_at(rest, 6)) {
            return Some("an owner ruling");
        }
        if c == '(' && rest.as_bytes().get(1) == Some(&b'#') {
            let n = rest[2..].bytes().take_while(u8::is_ascii_digit).count();
            if n > 0 && rest.as_bytes().get(2 + n) == Some(&b')') {
                return Some("an issue or pull request number");
            }
        }
        let at_word = prev.is_none_or(|p| !word_char(p)) && word_char(c);
        if at_word && let Some(kind) = at_word_start(rest, prev, prev2, in_code) {
            return Some(kind);
        }
        prev2 = prev;
        prev = Some(c);
    }
    None
}

/// The citation starting at a word start `rest` (`prev` and `prev2` are the characters before it, `in_code` whether
/// it is inside a backticked code span), if any.
fn at_word_start(rest: &str, prev: Option<char>, prev2: Option<char>, in_code: bool) -> Option<&'static str> {
    if let Some(k) = word_then_space(rest, &["brief", "briefs", "design", "designs"])
        && digit_at(rest, k)
    {
        return Some("a brief or design number");
    }
    if let Some(k) = word_then_space(rest, &["lane", "lanes"]) {
        let upper = rest.as_bytes().get(k).is_some_and(u8::is_ascii_uppercase);
        let three = (k..k + 3).all(|j| digit_at(rest, j)) && !digit_at(rest, k + 3);
        if upper || three {
            return Some("a lane");
        }
    }
    if rest.starts_with("FP") && digit_at(rest, 2) {
        return Some("a proposal finding (FP)");
    }
    if REQUIREMENT_IDS.iter().any(|id| rest.starts_with(id) && rest[id.len()..].starts_with('-') && digit_at(rest, id.len() + 1)) {
        return Some("a requirement id");
    }
    if ["overseer", "reviewer", "tester"].iter().any(|w| starts_ci(rest, w)) {
        return Some("a role of the agent workflow");
    }
    if starts_ci(rest, "ruling") && ends_at(rest, 6) {
        return Some("a ruling");
    }
    if starts_ci(rest, "owner") {
        let after = ["'s ", "s ", " "].iter().find(|s| rest[5..].starts_with(**s)).map(|s| 5 + s.len());
        if let Some(k) = after
            && OWNER_WORDS.iter().any(|w| starts_ci(&rest[k..], w) && ends_at(rest, k + w.len()))
        {
            return Some("an owner ruling");
        }
    }
    if !in_code && rest.starts_with("20") && date_at(rest) {
        return Some("a date");
    }
    if starts_ci(rest, "ui rev") {
        return Some("a UI revision");
    }
    if rest.starts_with('0') && digit_at(rest, 1) && digit_at(rest, 2) {
        let k = 3 + usize::from(rest.as_bytes().get(3).is_some_and(u8::is_ascii_lowercase));
        let grouped = prev == Some(' ') && prev2.is_some_and(|c| c.is_ascii_digit());
        let decimal = matches!(prev, Some('.' | ','));
        if ends_at(rest, k) && !grouped && !decimal {
            return Some("a brief number");
        }
    }
    if let Some(k) = word_then_space(rest, &["finding", "findings"]) {
        let k = k + usize::from(rest.as_bytes().get(k) == Some(&b'#'));
        if digit_at(rest, k) {
            return Some("a review finding");
        }
    }
    if (rest.starts_with("PR #") && digit_at(rest, 4))
        || starts_ci(rest, "review of #")
        || starts_ci(rest, "review of pr #")
        || starts_ci(rest, "end-gate review")
        || (starts_ci(rest, "review m") && digit_at(rest, 8))
    {
        return Some("a review or pull request");
    }
    if let Some(k) = word_then_space(rest, &["cut", "slice"])
        && digit_at(rest, k)
    {
        return Some("a cut or slice of a plan");
    }
    None
}

/// `rest` starts with a date `20YY-MM-DD` that is not the start of a timestamp.
fn date_at(rest: &str) -> bool {
    let b = rest.as_bytes();
    let shape = [0, 1, 2, 3, 5, 6, 8, 9].iter().all(|&j| digit_at(rest, j)) && b.get(4) == Some(&b'-') && b.get(7) == Some(&b'-');
    shape && !digit_at(rest, 10) && b.get(10) != Some(&b'T')
}

/// The comment text of `line`, whose comments start with `mark` (`//` in Rust, `#` in TOML): the whole line when it
/// is a comment, else what follows `mark` outside a string.
fn comment_of<'a>(line: &'a str, mark: &str) -> Option<&'a str> {
    let t = line.trim_start();
    if t.starts_with(mark) {
        return Some(t);
    }
    let mut in_str = false;
    let mut escaped = false;
    for (i, c) in line.char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' if in_str => escaped = true,
            '"' => in_str = !in_str,
            _ if !in_str && line[i..].starts_with(mark) => return Some(&line[i..]),
            _ => {}
        }
    }
    None
}

/// The citing lines of `text`, whose comments start with `mark`: (line number, kind).
fn places(text: &str, mark: &str) -> Vec<(usize, &'static str)> {
    text.lines().enumerate().filter_map(|(i, l)| comment_of(l, mark).and_then(citation).map(|k| (i + 1, k))).collect()
}

/// The `.toml` files directly in `dir`.
fn toml_files(dir: &std::path::Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "toml")).collect()
}

/// The guard's problems, if any.
pub fn violations(meta: &Value) -> Result<Vec<String>, String> {
    let root = PathBuf::from(meta["workspace_root"].as_str().ok_or("cargo metadata: no workspace root")?);
    let mut out = Vec::new();
    for (_, _, dir, _) in super::classed_packages(meta)? {
        let Some(dir) = dir else { continue };
        let mut paths = Vec::new();
        super::rust_files(&dir.join("src"), &mut paths);
        let mut paths: Vec<(PathBuf, &str)> = paths.into_iter().map(|p| (p, "//")).collect();
        paths.extend(toml_files(&dir.join("defaults")).into_iter().map(|p| (p, "#")));
        paths.sort();
        for (f, mark) in paths {
            let text = std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
            let rel = f.strip_prefix(&root).unwrap_or(&f).to_string_lossy().replace('\\', "/");
            out.extend(
                places(&text, mark)
                    .into_iter()
                    .map(|(l, kind)| format!("{rel}:{l}: a comment cites history ({kind}); say what the code does now, in domain words")),
            );
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn citations_are_found_and_domain_text_is_not() {
        let hits: &[(&str, &str)] = &[
            ("/// Brief 038: the cable list.", "a brief or design number"),
            ("// (design 065 §4)", "a section mark"),
            ("//! as lane 068 writes them", "a lane"),
            ("// the end of lane R9", "a lane"),
            ("// FP2: part data", "a proposal finding (FP)"),
            ("/// Refused on a broken page (PROJ-02).", "a requirement id"),
            ("// Reviewer 029: kept as it was", "a role of the agent workflow"),
            ("// (overseer ruling, M1 review)", "a role of the agent workflow"),
            ("// the owner's decision on sizes", "an owner ruling"),
            ("/// sizes (owner, 2026-11-12)", "an owner ruling"),
            ("// a ruling on sizes", "a ruling"),
            ("// checked by hand 2026-11-21", "a date"),
            ("// UI rev 3 colours", "a UI revision"),
            ("// as 030 keeps them", "a brief number"),
            ("// the review drawer (048b)", "a brief number"),
            ("// review 2026-11, finding 8", "a review finding"),
            ("// (review of PR #57, 4)", "a review or pull request"),
            ("// Review of #108: a fuse", "a review or pull request"),
            ("// hand-rolled (#113)", "an issue or pull request number"),
            ("// end-gate review", "a review or pull request"),
            ("// definition key, review M1 2.1", "a review or pull request"),
            ("// cut 12 moved it", "a cut or slice of a plan"),
            ("let x = 1; // brief 012", "a brief or design number"),
        ];
        for (line, kind) in hits {
            assert_eq!(comment_of(line, "//").and_then(citation), Some(*kind), "{line}");
        }
        let clean = [
            "/// The jumper bar on lane 0 of the strip.",
            "/// A tag such as `JB-10` or `-Q01`.",
            "// PLC address %Q2.2 or Q4.",
            "// 10 000 references, 1 000 pages",
            "/// assert!((x - 3.57).abs() < 0.005);",
            "/// The route: 2 to 4,096 [x, y] points.",
            "/// Now, UTC, `2026-11-12T14:30:05Z`.",
            "/// The revision bar: `Rev A, 2026-11-08: read-only`.",
            "// ConnectionPointOwner and the owner device",
            "// the review drawer lists drafts",
            "// a design rule for the frame",
            "// file 2 of 3",
            "let s = \"brief 012 // design 065\";",
        ];
        for line in clean {
            assert_eq!(comment_of(line, "//").and_then(citation), None, "{line}");
        }
    }

    #[test]
    fn places_names_the_lines() {
        assert_eq!(places("fn a() {}\n// Brief 038: x\nlet s = \"§\";\n", "//"), vec![(2, "a brief or design number")]);
        let toml = "# Brief 047: the BOM.\n\"a.b\" = 2.5   # owner, 2026-11-15\nname = \"#1 §\"\n";
        assert_eq!(places(toml, "#"), vec![(1, "a brief or design number"), (2, "a date")]);
    }
}
