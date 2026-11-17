//! Design 072 §13: the typed-object-model guards that `cargo xtask layers` checks textually. Each
//! starts as a ratchet at its exact count in the lane that adds it; the count only falls, and the
//! end gate checks every guard at 0 as a plain ban.
//!
//! Counted over the product crates' `src/` (Core, Engine, UI kit, App, Programs), outside `//`
//! comments and outside a file's trailing `#[cfg(test)]` module.

use std::path::{Path, PathBuf};

use serde_json::Value;

/// The one file where the word rules may be written out (design 072 §9.1).
const WORDS_FILE: &str = "crates/ecad-model/src/words.rs";

/// The boolean words G2 looks for, as string literals.
const BOOL_LITERALS: &[&str] = &["\"yes\"", "\"no\"", "\"true\"", "\"false\"", "\"on\"", "\"off\"", "\"y\"", "\"n\""];

/// One guard: its name, what it counts, the count it may not exceed, and the fix.
pub struct Guard {
    pub name: &'static str,
    pub what: &'static str,
    pub allowed: usize,
    pub fix: &'static str,
    pub scan: Scan,
}

/// How a guard finds its places in a file (comments and the trailing test module stripped).
pub enum Scan {
    /// Whether one line, in file `rel`, is a place.
    Line(fn(rel: &str, line: &str) -> bool),
    /// The 0-based numbers of the lines that are places, from the whole file.
    File(fn(rel: &str, code: &str) -> Vec<usize>),
}

/// The guards and their counts. Lane 072 A1 recorded the starting counts; later lanes lower them,
/// and the end of lane F sets each to 0.
#[rustfmt::skip]
pub const GUARDS: &[Guard] = &[
    Guard { name: "G2", what: "boolean words compared or matched", allowed: 0, fix: "read it with ecad_model::words::flag", scan: Scan::Line(g2) },
    Guard { name: "G3", what: "`.all(…is_ascii_digit…)` digit checks", allowed: 0, fix: "use ecad_model::words::is_digits, uint or uint_canonical", scan: Scan::Line(g3) },
    Guard { name: "G8a", what: "`Kind::Choice(&[` word lists", allowed: 0, fix: "declare the words as a word_enum! and use a typed choice", scan: Scan::Line(g8_choice) },
    Guard { name: "G8b", what: "`.word() == \"` comparisons", allowed: 0, fix: "compare the typed value, not its word", scan: Scan::Line(g8_word) },
    Guard { name: "G10", what: "catch-all words or bodies (`Unknown(`, `Stored<`, `Raw`)", allowed: 4, fix: "an unknown word is a read error (design 072 §9.4); type it", scan: Scan::Line(g10) },
    Guard { name: "G13", what: "`String` or `&str` fields in stored structs", allowed: 25, fix: "store a Text<N>, LongText<N> or a typed value (design 072 §9.4)", scan: Scan::File(g13) },
    Guard { name: "G14", what: "hidden-character checks outside words.rs", allowed: 0, fix: "use ecad_model::text::Text<N> (or words::is_hidden in ecad-model's text types)", scan: Scan::Line(g14) },
];

fn g2(rel: &str, line: &str) -> bool {
    rel != WORDS_FILE
        && BOOL_LITERALS.iter().any(|w| {
            let compared = [format!("== {w}"), format!("!= {w}"), format!("{w} =="), format!("{w} !="), format!("eq_ignore_ascii_case({w})")];
            let matched = [format!("{w} =>"), format!("{w} |"), format!("| {w}")];
            compared.iter().chain(&matched).any(|p| line.contains(p.as_str())) || (line.contains("matches!(") && line.contains(w))
        })
}

fn g3(rel: &str, line: &str) -> bool {
    rel != WORDS_FILE && line.find(".all(").is_some_and(|i| line.get(i..).is_some_and(|rest| rest.contains("is_ascii_digit")))
}

fn g8_choice(_: &str, line: &str) -> bool {
    line.contains("Kind::Choice(&[")
}

fn g8_word(_: &str, line: &str) -> bool {
    line.contains(".word() == \"")
}

fn g10(_: &str, line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("Unknown(") || line.contains("Stored<") || line.contains("enum Raw ") || line.contains(", Raw>")
}

/// The files where the hidden-character rule may appear: the rule and the text types built on it.
const HIDDEN_RULE_FILES: &[&str] = &[WORDS_FILE, "crates/ecad-model/src/text.rs"];

/// Hand-written members of the hidden set (G14): a few of its code points, as hex or escapes.
const HIDDEN_MARKS: &[&str] = &["is_hidden", "0x200B", "0x202A", "0x2066"];
/// The same code points written as Rust escapes, compared in lower case.
const HIDDEN_ESCAPES: &[&str] = &[r"\u{200b}", r"\u{202a}", r"\u{202e}", r"\u{2066}"];

fn g14(rel: &str, line: &str) -> bool {
    !HIDDEN_RULE_FILES.contains(&rel)
        && (HIDDEN_MARKS.iter().any(|m| line.contains(m)) || HIDDEN_ESCAPES.iter().any(|m| line.to_ascii_lowercase().contains(m)))
}

/// The crate whose structs are stored (design 072 Part I: stored structs live in ecad-model).
const STORED_CRATE: &str = "crates/ecad-model/src/";

/// G13: in ecad-model, the fields of a struct deriving `Serialize` typed with `String` or `&str`
/// (also inside `Option`, `Vec` or a map), and tuple structs over them.
fn g13(rel: &str, code: &str) -> Vec<usize> {
    let mut out = Vec::new();
    if !rel.starts_with(STORED_CRATE) {
        return out;
    }
    let textual = |l: &str| l.contains("String") || l.contains("&str") || l.contains("&'static str");
    let (mut serialized, mut depth) = (false, 0usize);
    for (i, line) in code.lines().enumerate() {
        let t = line.trim();
        if depth > 0 {
            let field = t.split_once(':').is_some_and(|(name, _)| {
                let name = name.trim_start_matches("pub(crate) ").trim_start_matches("pub ");
                !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            });
            if depth == 1 && field && textual(t) {
                out.push(i);
            }
            depth = (depth + t.matches('{').count()).saturating_sub(t.matches('}').count());
            continue;
        }
        if t.starts_with("#[derive(") {
            serialized = t.contains("Serialize");
            continue;
        }
        if t.starts_with('#') || t.starts_with("///") || t.is_empty() {
            continue;
        }
        let is_struct = t.starts_with("struct ") || t.starts_with("pub struct ") || t.starts_with("pub(crate) struct ");
        if serialized && is_struct {
            if t.ends_with('{') {
                depth = 1;
            } else if t.contains('(') && textual(t) {
                out.push(i);
            }
        }
        serialized = false;
    }
    out
}

/// `text` up to its trailing `#[cfg(test)]` module, if it has one: the line `#[cfg(test)]`
/// followed by a `mod` line.
pub fn product_part(text: &str) -> &str {
    let mut offset = 0;
    let mut lines = text.split_inclusive('\n').peekable();
    while let Some(line) = lines.next() {
        if line.trim() == "#[cfg(test)]" && lines.peek().is_some_and(|next| next.trim_start().starts_with("mod ")) {
            return text.get(..offset).unwrap_or(text);
        }
        offset += line.len();
    }
    text
}

/// The places of `guard` in file `rel` with contents `text`, as `rel:line: text`.
pub fn places(guard: &Guard, rel: &str, text: &str) -> Vec<String> {
    let product = product_part(text);
    let code = super::strip_line_comments(product);
    let lines: Vec<usize> = match guard.scan {
        Scan::Line(hit) => code.lines().enumerate().filter(|(_, l)| hit(rel, l)).map(|(i, _)| i).collect(),
        Scan::File(scan) => scan(rel, &code),
    };
    lines.into_iter().map(|i| format!("{rel}:{}: {}", i + 1, product.lines().nth(i).unwrap_or_default().trim())).collect()
}

/// Every guard's count over the product crates in `meta`, against [`GUARDS`]: a count that rose
/// (with its places) or fell (lower it).
pub fn violations(meta: &Value) -> Result<Vec<String>, String> {
    let root = PathBuf::from(meta["workspace_root"].as_str().ok_or("cargo metadata: no workspace root")?);
    let mut files: Vec<(String, String)> = Vec::new();
    for (_, class, dir, _) in super::classed_packages(meta)? {
        let (true, Some(dir)) =
            (matches!(class, super::Class::Core | super::Class::Engine | super::Class::UiKit | super::Class::App | super::Class::Programs), dir)
        else {
            continue;
        };
        let mut paths = Vec::new();
        super::rust_files(&dir.join("src"), &mut paths);
        for f in paths {
            let text = std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
            files.push((relative(&root, &f), text));
        }
    }
    let mut out = Vec::new();
    for g in GUARDS {
        let found: Vec<String> = files.iter().flat_map(|(rel, text)| places(g, rel, text)).collect();
        let what = format!("{} ({})", g.what, g.name);
        if let Some(p) = super::ratchet_problem(g.name, found.len(), g.allowed, &what, g.fix) {
            out.push(p);
            if found.len() > g.allowed {
                out.extend(found.into_iter().map(|f| format!("    {f}")));
            }
        }
    }
    Ok(out)
}

fn relative(root: &Path, f: &Path) -> String {
    f.strip_prefix(root).unwrap_or(f).to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(name: &str, rel: &str, text: &str) -> usize {
        GUARDS.iter().find(|g| g.name == name).map_or(usize::MAX, |g| places(g, rel, text).len())
    }

    #[test]
    fn the_guards_find_their_patterns() {
        let table: &[(&str, &str, &str, usize)] = &[
            ("G2", "crates/c/src/a.rs", "if v == \"yes\" {}", 1),
            ("G2", "crates/c/src/a.rs", "Some(\"on\" | \"1\") => true,", 1),
            ("G2", "crates/c/src/a.rs", "let w = \"yes\";", 0),
            ("G2", "crates/c/src/a.rs", "Mode::Off => \"off\",", 0),
            ("G2", "crates/c/src/a.rs", "x => json!({\"on\": true}),", 0),
            ("G2", "crates/c/src/a.rs", "v.trim().eq_ignore_ascii_case(\"no\")", 1),
            ("G2", "crates/c/src/a.rs", "matches!(s, \"yes\" | \"y\")", 1),
            ("G2", WORDS_FILE, "if v == \"yes\" {}", 0),
            ("G2", "crates/c/src/a.rs", "// v == \"yes\"", 0),
            ("G3", "crates/c/src/a.rs", "t.bytes().all(|b| b.is_ascii_digit())", 1),
            ("G3", "crates/c/src/a.rs", "t.chars().take_while(char::is_ascii_digit)", 0),
            ("G8a", "crates/c/src/a.rs", "kind: Kind::Choice(&[\"a\", \"b\"]),", 1),
            ("G8b", "crates/c/src/a.rs", "if v.word() == \"left\" {}", 1),
            ("G10", "crates/c/src/a.rs", "    Unknown(String),", 1),
            ("G10", "crates/c/src/a.rs", "pub other: BTreeMap<String, Raw>,", 1),
            ("G10", "crates/c/src/a.rs", "Source::Unknown(_) => false,", 0),
            ("G2", "crates/c/src/a.rs", "fn f() {}\n#[cfg(test)]\nmod tests {\n    fn t() { assert!(v == \"yes\"); }\n}\n", 0),
        ];
        for (name, rel, text, want) in table {
            assert_eq!(count(name, rel, text), *want, "{name} on {text:?}");
        }
    }
}
