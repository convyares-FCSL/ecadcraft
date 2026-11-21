//! Brief 074 ("Rules"): ecad-cadcraft is only the adapter to CADCraft, so domain rules do not creep
//! back into it. This guard counts the engine's **rule functions**: functions whose inputs are all
//! Core or standard types (no `Session`, no `Project`, no CADCraft type, no type the engine crate
//! defines, no `self`), at least one of them a type a Core crate declares (the document, an id, a
//! point: a domain input, so helpers over only numbers, text and paths are not counted), and that
//! return something (a decision, a plan, words, JSON), again not an engine type. Such a function needs nothing the engine has, so it belongs in a Core crate
//! (usually ecad-electrical).
//!
//! The count is kept per file: a file on [`ALLOWED`] may have exactly that many (a ratchet: lower
//! the count when one moves out, remove the entry at 0), every other file none. Each entry says
//! why its functions are still in the engine.
//!
//! Textual, like the other guards: over `crates/ecad-cadcraft/src/`, outside `//` comments and
//! outside a file's trailing `#[cfg(test)]` module. A type counts as Core when a Core crate
//! declares it `pub` ([`CORE_CRATES`]) or it is a standard one ([`STD_TYPES`]); one the engine
//! crate declares, or a file imports from `cadcraft_*`, is an engine type even when a Core crate
//! has the same name (the engine's font wrappers `FrameText` and `FrameInput`, for example).
//! Unknown names count as engine, so the guard never flags what it cannot place. Methods with a
//! `self` receiver are not counted.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// The engine crate's source folder, from the workspace root.
const ENGINE_SRC: &str = "crates/ecad-cadcraft/src";

/// The Core crates whose `pub` types count as Core (design 065, `docs/design/crates.md` §1).
const CORE_CRATES: &[&str] = &["ecad-model", "ecad-drawing-style", "ecad-store", "ecad-file", "ecad-inspect", "ecad-electrical"];

/// Standard and serde types that count as Core. Single capital letters (generic parameters) count
/// too.
const STD_TYPES: &[&str] = &[
    "String",
    "Option",
    "Vec",
    "Result",
    "Box",
    "Rc",
    "Arc",
    "Cow",
    "BTreeMap",
    "BTreeSet",
    "HashMap",
    "HashSet",
    "VecDeque",
    "Path",
    "PathBuf",
    "Ordering",
    "Range",
    "RangeInclusive",
    "Fn",
    "FnMut",
    "FnOnce",
    "Iterator",
    "IntoIterator",
    "Value",
    "Map",
];

/// Engine types every file sees (the project handle and its pages).
const ENGINE_TYPES: &[&str] = &["Session", "Project", "Pages", "Self"];

/// The files that still have rule functions, how many, and why they stay for now. The count only
/// falls.
#[rustfmt::skip]
const ALLOWED: &[(&str, usize, &str)] = &[
    // Waits for R9 (the views lane): the cable fan, label layout and their vector helpers move with
    // the views. views.rs counts 0 today (its rules take engine types) but moves with R9 all the same.
    ("crates/ecad-cadcraft/src/look.rs", 17, "waits for R9: cable fan geometry, label layout and their point helpers"),
    ("crates/ecad-cadcraft/src/views.rs", 0, "waits for R9: the traditional/fan choice moves with the views"),
    // The agent gate stays in the engine (brief 074 "What stays").
    ("crates/ecad-cadcraft/src/agent/gate.rs", 3, "the agent gate: undo step, issue order and the transaction summary it answers with"),
    ("crates/ecad-cadcraft/src/agent/mod.rs", 1, "the agent gate: an author's words in its answers"),
    ("crates/ecad-cadcraft/src/agent/tools.rs", 1, "the MCP tools' list paging"),
    // The dev tools stay (brief 074 "What stays").
    ("crates/ecad-cadcraft/src/debug.rs", 6, "the live debugger: replay to a sequence number and its read-only labels"),
    ("crates/ecad-cadcraft/src/verify.rs", 3, "the projection check's point comparisons (a dev check)"),
    ("crates/ecad-cadcraft/src/translate_oracle.rs", 1, "the translate oracle's preview (a dev check)"),
    // Drawing and interaction: geometry for entities and gestures, measured with CADCraft's own
    // conventions (ACI colours, snapping, placement), so it stays with `draw/` and `interact/`.
    ("crates/ecad-cadcraft/src/draw/plc/sheets.rs", 1, "the I/O sheet's cell texts, drawn next to it"),
    ("crates/ecad-cadcraft/src/draw/text_style.rs", 1, "text width with this machine's fonts"),
    ("crates/ecad-cadcraft/src/draw/title_block/dxf.rs", 2, "the DXF reader (CADCraft's) and the area it fits into"),
    ("crates/ecad-cadcraft/src/draw/title_block/layout.rs", 3, "title block drawing: ACI colours, extents and placing primitives"),
    ("crates/ecad-cadcraft/src/draw/xref.rs", 1, "the contact table drawn under a device"),
    ("crates/ecad-cadcraft/src/strip_page.rs", 2, "the strip page's drawn row heights"),
    ("crates/ecad-cadcraft/src/translate.rs", 2, "movement: the preview ledger and the drawn shape of a moved primitive"),
    ("crates/ecad-cadcraft/src/interact/connectors.rs", 1, "a gesture's side word"),
    ("crates/ecad-cadcraft/src/interact/forms.rs", 1, "a form's placed primitive preview"),
    ("crates/ecad-cadcraft/src/interact/location_form.rs", 1, "the location form's snapping distance"),
    ("crates/ecad-cadcraft/src/interact/mod.rs", 4, "snapping onto primitives and the hover's words"),
    ("crates/ecad-cadcraft/src/interact/text_tools.rs", 1, "the text tool's kind"),
    // Project glue: which views to keep, what an edit touched, the print book's note.
    ("crates/ecad-cadcraft/src/project/mod.rs", 1, "builds the project's views when it opens"),
    ("crates/ecad-cadcraft/src/project/page_tree.rs", 1, "whether a commit touched the page tree (the open project's cache)"),
    ("crates/ecad-cadcraft/src/printbook.rs", 1, "the print book's pending note, written into the PDF"),
    ("crates/ecad-cadcraft/src/merging.rs", 3, "the merge session's objects, edit rule and conflict words; moves with the merge work"),
    ("crates/ecad-cadcraft/src/history.rs", 2, "the history panel's group words and device changes; next to move after 074"),
    // Command glue: a report's file, and JSON a command still reads by hand.
    ("crates/ecad-cadcraft/src/cmd/output.rs", 1, "a report's page, or the CSV file it writes"),
    ("crates/ecad-cadcraft/src/cmd/title_block.rs", 1, "titleblockimport's frame read from JSON by hand until lane 066d types it"),
];

/// The parameters of a list, split at its top-level commas.
fn split_top(list: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut depth, mut start) = (0i32, 0usize);
    for (i, c) in list.char_indices() {
        match c {
            '<' | '(' | '[' => depth += 1,
            '>' if !list[..i].ends_with('-') => depth -= 1,
            ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&list[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&list[start..]);
    out.into_iter().map(str::trim).filter(|p| !p.is_empty()).collect()
}

/// The type names in `ty`: its identifiers that start with a capital letter, plus `cadcraft` when
/// it names a CADCraft path.
fn type_names(ty: &str) -> Vec<String> {
    let mut out = Vec::new();
    if ty.contains("cadcraft") {
        out.push("cadcraft".to_string());
    }
    let mut cur = String::new();
    for c in ty.chars().chain(std::iter::once(' ')) {
        if c.is_alphanumeric() || c == '_' {
            cur.push(c);
        } else {
            if cur.chars().next().is_some_and(char::is_uppercase) {
                out.push(std::mem::take(&mut cur));
            }
            cur.clear();
        }
    }
    out
}

/// What the guard knows about type names.
struct Names {
    core: BTreeSet<String>,
    engine: BTreeSet<String>,
}

impl Names {
    /// Whether `ty` names a type a Core crate declares (a domain input, not only standard types).
    fn names_domain(&self, ty: &str, local: &BTreeSet<String>) -> bool {
        type_names(ty).iter().any(|n| self.core.contains(n) && !STD_TYPES.contains(&n.as_str()) && !local.contains(n) && !self.engine.contains(n))
    }

    /// Whether every name in `ty` is Core (or standard), for a file that also imports `local`
    /// from CADCraft.
    fn is_core(&self, ty: &str, local: &BTreeSet<String>) -> bool {
        type_names(ty).iter().all(|n| {
            let generic = n.len() == 1;
            !self.engine.contains(n) && !local.contains(n) && (generic || STD_TYPES.contains(&n.as_str()) || self.core.contains(n))
        })
    }
}

/// The names `text` declares `pub` (structs, enums, unions, type aliases, traits).
fn pub_types(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let l = line.trim_start();
        let Some(rest) = l.strip_prefix("pub ").or_else(|| l.strip_prefix("pub(crate) ")) else { continue };
        for kw in ["struct ", "enum ", "union ", "type ", "trait "] {
            if let Some(name) = rest.strip_prefix(kw) {
                let n: String = name.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                if !n.is_empty() {
                    out.push(n);
                }
            }
        }
    }
    out
}

/// The names `code` imports from a `cadcraft_*` crate (`use cadcraft_engine::doc::{Entity, …}`).
fn cadcraft_imports(code: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = code;
    while let Some(at) = rest.find("use cadcraft") {
        let tail = &rest[at..];
        let end = tail.find(';').unwrap_or(tail.len());
        out.extend(type_names(&tail[..end]).into_iter().filter(|n| n != "cadcraft"));
        rest = &tail[end..];
    }
    out
}

/// `text` up to its trailing `#[cfg(test)]` module, without `//` comments.
fn product_code(text: &str) -> String {
    let cut = text.lines().position(|l| l.trim_start().starts_with("#[cfg(test)]")).unwrap_or(usize::MAX);
    let head: Vec<&str> = text.lines().take(cut).collect();
    super::strip_line_comments(&head.join("\n"))
}

/// The rule functions in one file's product code, as `rel:line: fn name`.
fn places(rel: &str, code: &str, names: &Names) -> Vec<String> {
    let local = cadcraft_imports(code);
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(found) = code.get(from..).and_then(|c| c.find("fn ")) {
        let at = from + found;
        from = at + 3;
        // A whole word `fn`, followed by a name and its generics or parameters.
        if code[..at].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '"') {
            continue;
        }
        let rest = &code[from..];
        let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        if name.is_empty() {
            continue;
        }
        let mut i = name.len();
        let bytes = rest.as_bytes();
        if bytes.get(i) == Some(&b'<') {
            let mut depth = 0i32;
            while let Some(&b) = bytes.get(i) {
                match b {
                    b'<' => depth += 1,
                    b'>' => depth -= 1,
                    _ => {}
                }
                i += 1;
                if depth == 0 {
                    break;
                }
            }
        }
        if bytes.get(i) != Some(&b'(') {
            continue;
        }
        let open = i;
        let mut depth = 0i32;
        let mut close = None;
        for (j, &b) in bytes.iter().enumerate().skip(open) {
            match b {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(j);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(close) = close else { continue };
        let params = split_top(&rest[open + 1..close]);
        let after = &rest[close + 1..];
        let sig_end = after.find(['{', ';']).unwrap_or(after.len());
        let sig = &after[..sig_end];
        let ret = sig.split(" where ").next().unwrap_or_default().trim();
        let Some(ret) = ret.strip_prefix("->").map(str::trim) else { continue };
        if ret == "()" || ret.is_empty() || params.is_empty() {
            continue;
        }
        let receiver = params.first().is_some_and(|p| {
            let p = p.trim_start_matches('&').trim();
            let p = if p.starts_with('\'') { p.split_once(' ').map_or("", |x| x.1) } else { p };
            let p = p.strip_prefix("mut ").unwrap_or(p).trim();
            p == "self" || p.starts_with("self:") || p.starts_with("self ")
        });
        if receiver {
            continue;
        }
        let types: Vec<&str> = params.iter().filter_map(|p| p.split_once(':').map(|(_, ty)| ty)).collect();
        let inputs_core = types.len() == params.len() && types.iter().all(|ty| names.is_core(ty, &local));
        let domain = types.iter().any(|ty| names.names_domain(ty, &local));
        if inputs_core && domain && names.is_core(ret, &local) {
            let line = code[..at].matches('\n').count() + 1;
            out.push(format!("{rel}:{line}: fn {name}"));
        }
    }
    out
}

/// The `pub` types of the Core crates, and of the engine crate, under `root`.
fn names(root: &Path) -> Result<Names, String> {
    let read = |dir: &Path| -> Result<Vec<String>, String> {
        let mut files = Vec::new();
        super::rust_files(dir, &mut files);
        let mut out = Vec::new();
        for f in files {
            let text = std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
            out.extend(pub_types(&text));
        }
        Ok(out)
    };
    let mut core = BTreeSet::new();
    for c in CORE_CRATES {
        core.extend(read(&root.join("crates").join(c).join("src"))?);
    }
    let mut engine: BTreeSet<String> = read(&root.join(ENGINE_SRC))?.into_iter().collect();
    engine.extend(ENGINE_TYPES.iter().map(|s| s.to_string()));
    Ok(Names { core, engine })
}

/// The guard's problems, if any.
pub fn violations(meta: &Value) -> Result<Vec<String>, String> {
    let root = PathBuf::from(meta["workspace_root"].as_str().ok_or("cargo metadata: no workspace root")?);
    let names = names(&root)?;
    let mut files = Vec::new();
    super::rust_files(&root.join(ENGINE_SRC), &mut files);
    let mut by_file: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
    for f in files {
        let rel = f.strip_prefix(&root).unwrap_or(&f).to_string_lossy().replace('\\', "/");
        let text = std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
        let found = places(&rel, &product_code(&text), &names);
        if !found.is_empty() {
            by_file.insert(rel, found);
        }
    }
    let what = "engine rule functions (brief 074: inputs only Core types, returns a decision)";
    let fix = "move the function to a Core crate (usually ecad-electrical) and call it from the engine";
    let mut out = Vec::new();
    for (rel, found) in &by_file {
        let allowed = ALLOWED.iter().find(|(p, _, _)| p == rel).map_or(0, |(_, n, _)| *n);
        if let Some(p) = super::ratchet_problem(rel, found.len(), allowed, what, fix) {
            out.push(p);
            if found.len() > allowed {
                out.extend(found.iter().map(|f| format!("    {f}")));
            }
        }
    }
    for (rel, allowed, _) in ALLOWED {
        if !by_file.contains_key(*rel)
            && let Some(p) = super::ratchet_problem(rel, 0, *allowed, what, fix)
        {
            out.push(p);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> Names {
        let core = ["Document", "PageId", "StyleRef", "FrameText", "SheetSpec", "CmdError", "IdAllocator", "Op"];
        let engine = ["Session", "Project", "Pages", "Self", "FrameText", "EngineMetrics"];
        Names { core: core.iter().map(|s| s.to_string()).collect(), engine: engine.iter().map(|s| s.to_string()).collect() }
    }

    #[test]
    fn rule_functions_are_found() {
        let rows: &[(&str, usize)] = &[
            ("pub fn pages_using(doc: &Document, id: &StyleRef) -> Vec<PageId> {", 1),
            ("fn plan<T>(doc: &Document, ids: &mut IdAllocator, n: T) -> Result<Vec<Op>, CmdError> {", 1),
            ("pub(crate) fn words(\n    doc: &Document,\n    page: PageId,\n) -> String {", 1),
            ("fn run(s: &mut Session) -> Result<Value, CmdError> {", 0),
            ("fn apply(p: &mut Project, doc: &Document) -> bool {", 0),
            ("fn layout(doc: &Document, text: &FrameText) -> String {", 0),
            ("fn draw(e: &cadcraft_engine::doc::Entity) -> bool {", 0),
            ("fn commit(doc: &Document) {", 0),
            ("fn constant() -> usize {", 0),
            ("fn width(text: &str, size: f64) -> f64 {", 0),
            ("fn of(&self, doc: &Document) -> PageId {", 0),
            ("fn unknown(doc: &Document, x: &Mystery) -> bool {", 0),
            ("fn returns_engine(doc: &Document) -> EngineMetrics {", 0),
            ("let f = \"fn x(doc: &Document) -> bool {\";", 0),
            ("fn with_fn(doc: &Document, f: impl Fn(PageId) -> bool) -> Vec<PageId> {", 1),
        ];
        for (text, want) in rows {
            assert_eq!(places("f.rs", text, &names()).len(), *want, "{text}");
        }
    }

    #[test]
    fn cadcraft_imports_are_engine_types() {
        let code = "use cadcraft_engine::doc::{Entity, Handle};\nfn f(doc: &Document, h: Handle) -> bool {";
        assert_eq!(places("f.rs", code, &names()).len(), 0);
        assert_eq!(cadcraft_imports(code), ["Entity", "Handle"].iter().map(|s| s.to_string()).collect());
    }

    #[test]
    fn the_test_module_and_comments_are_skipped() {
        let text = "// fn a(doc: &Document) -> bool {\nfn b(doc: &Document) -> bool {}\n#[cfg(test)]\nmod tests { fn c(doc: &Document) -> bool {} }";
        assert_eq!(places("f.rs", &product_code(text), &names()), vec!["f.rs:2: fn b".to_string()]);
    }
}
