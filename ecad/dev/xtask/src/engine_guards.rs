//! ecad-engine is only the adapter to CADCraft, so domain rules do not creep
//! back into it. This guard counts the engine's **rule functions**: functions whose inputs are all
//! Core or standard types (no `Session`, no `Project`, no CADCraft type, no type the engine crate
//! defines, no `self`), at least one of them a type a Core crate declares (the document, an id, a
//! point: a domain input, so helpers over only numbers, text and paths are not counted), and that
//! return something (a decision, a plan, words, JSON), again not an engine type. Such a function needs nothing the engine has, so it belongs in a Core crate
//! (usually ecad-electrical).
//!
//! No file has one, apart from the areas on [`EXEMPT`], which keep theirs by design and say why
//! (a folder or a file, no count). Functions on [`ADAPTERS`] (thin wrappers over a font,
//! the clock, a file write or a CADCraft reader, which the textual test cannot see) are not counted.
//!
//! Textual, like the other guards: over `crates/ecad-engine/src/`, outside `//` comments and
//! outside a file's trailing `#[cfg(test)]` module. A type counts as Core when a Core crate
//! declares it `pub` ([`CORE_CRATES`]) or it is a standard one ([`STD_TYPES`]); one the engine
//! crate declares, or a file imports from `cadcraft_*`, is an engine type even when a Core crate
//! has the same name (the engine's font wrappers `FrameText` and `FrameInput`, for example).
//! Types a Core macro declares (ids, units, text types: [`DECLARING_MACROS`]) count as Core.
//! Unknown names count as engine, so the guard never flags what it cannot place. Methods with a
//! `self` receiver are not counted.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// The engine crate's source folder, from the workspace root.
const ENGINE_SRC: &str = "crates/ecad-engine/src";

/// The Core crates whose `pub` types count as Core.
const CORE_CRATES: &[&str] = &["ecad-model", "ecad-store", "ecad-file", "ecad-electrical", "ecad-drawing-style"];

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

/// The areas of the engine that keep rule functions by design, and why: each needs a live session,
/// or is movement, where a mistake is most expensive. An entry is a folder (ending in `/`) or a file under the workspace root; no
/// count, and every other file has none.
#[rustfmt::skip]
const EXEMPT: &[(&str, &str)] = &[
    ("crates/ecad-engine/src/agent/", "the agent gate and tools: the undo step, issue order, transaction summary and list paging an agent's session answers with"),
    ("crates/ecad-engine/src/dev/", "the dev tools (snapshot, live debugger, projection check): read-only views of a live session, never shipped behaviour"),
    ("crates/ecad-engine/src/translate/", "movement: the drawn shape of a moved primitive, kept beside the movement code, where a mistake is most expensive"),
];

/// Functions the guard's textual test counts that are adapters, not domain rules: each is a thin
/// wrapper over something only the engine has, which the guard cannot see in a body. They are
/// left out of the count by file and name; one that no longer exists is a problem, so the list
/// stays true.
#[rustfmt::skip]
const ADAPTERS: &[(&str, &str, &str)] = &[
    ("crates/ecad-engine/src/cables/draw.rs", "marker_box", "measures text with CADCraft's drafting font"),
    ("crates/ecad-engine/src/styles/draw.rs", "text_width", "measures text with CADCraft's fonts (EngineMetrics)"),
    ("crates/ecad-engine/src/references/draw/devices.rs", "table", "lays the contact table out with CADCraft's fonts (EngineMetrics)"),
    ("crates/ecad-engine/src/history/mod.rs", "group_text", "reads the local time zone"),
    ("crates/ecad-engine/src/project/mod.rs", "views_for", "builds the engine's own views"),
    ("crates/ecad-engine/src/title_blocks/dxf.rs", "block", "reads the DXF with CADCraft's reader"),
    ("crates/ecad-engine/src/translate/mod.rs", "preview", "applies ops to the live ledger and rolls them back"),
    ("crates/ecad-engine/src/convert.rs", "block_name", "names the CADCraft block a definition projects to"),
    ("crates/ecad-engine/src/cables/views.rs", "drawings_on_page", "lays cables out with CADCraft's fonts (font_width, LOOK)"),
    ("crates/ecad-engine/src/devices/draw.rs", "texts_on_page", "measures derived texts with CADCraft's fonts (LOOK)"),
    ("crates/ecad-engine/src/devices/draw.rs", "hover_lines", "measures a device's lens with CADCraft's fonts (LOOK)"),
    ("crates/ecad-engine/src/devices/draw.rs", "all_texts_on_page", "measures derived texts with CADCraft's fonts (LOOK)"),
    ("crates/ecad-engine/src/references/draw/cables.rs", "reference_at", "measures reference flags with CADCraft's fonts (LOOK)"),
    ("crates/ecad-engine/src/references/draw/devices.rs", "contacts_json", "measures the contact table with CADCraft's fonts (LOOK)"),
    ("crates/ecad-engine/src/references/interact/cables.rs", "check_continue", "plans a reference end with CADCraft's fonts (LOOK)"),
    ("crates/ecad-engine/src/wires/draw.rs", "labels_on_page_with", "measures wire labels with CADCraft's fonts (LOOK)"),
    ("crates/ecad-engine/src/wires/draw.rs", "labels_on_page", "measures wire labels with CADCraft's fonts (LOOK)"),
    ("crates/ecad-engine/src/wires/draw.rs", "quiet_ends_of", "reads reference markers and cable layouts with CADCraft's fonts (LOOK)"),
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

/// The Core macros that declare a `pub` type, named by the first identifier of their call
/// (`typed_id!(/// A device. DeviceId, "cmp")`).
const DECLARING_MACROS: &[&str] = &["typed_id!(", "unit!(", "text_type!("];

/// The names `text` declares `pub` (structs, enums, unions, type aliases, traits, and the types
/// [`DECLARING_MACROS`] declare).
fn pub_types(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for m in DECLARING_MACROS {
        for (at, _) in text.match_indices(m) {
            if text[..at].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_') {
                continue;
            }
            // Skip the doc comments, then take the name.
            let mut rest = text[at + m.len()..].trim_start();
            while let Some(r) = rest.strip_prefix("///") {
                rest = r.split_once('\n').map_or("", |x| x.1).trim_start();
            }
            let n: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
            if n.chars().next().is_some_and(char::is_uppercase) {
                out.push(n);
            }
        }
    }
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
pub(crate) fn product_code(text: &str) -> String {
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
    let mut adapter_gone = Vec::new();
    let mut files_seen = BTreeSet::new();
    for f in files {
        let rel = f.strip_prefix(&root).unwrap_or(&f).to_string_lossy().replace('\\', "/");
        files_seen.insert(rel.clone());
        let text = std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
        let mut found = places(&rel, &product_code(&text), &names);
        found.retain(|f| {
            let name = f.rsplit("fn ").next().unwrap_or_default();
            !ADAPTERS.iter().any(|(p, n, _)| *p == rel && *n == name)
        });
        for (p, n, _) in ADAPTERS.iter().filter(|(p, _, _)| *p == rel) {
            if !text_has_fn(&text, n) {
                adapter_gone.push(format!("{p}: adapter `{n}` is on engine_guards::ADAPTERS but no longer there; remove it"));
            }
        }
        if !found.is_empty() {
            by_file.insert(rel, found);
        }
    }
    let what = "engine rule functions (inputs only Core types, returns a decision)";
    let fix = "move the function to a Core crate (usually ecad-electrical) and call it from the engine";
    let mut out = Vec::new();
    for (rel, found) in &by_file {
        if exempt(rel) {
            continue;
        }
        if let Some(p) = super::ratchet_problem(rel, found.len(), 0, what, fix) {
            out.push(p);
            out.extend(found.iter().map(|f| format!("    {f}")));
        }
    }
    for (area, _) in EXEMPT {
        if !files_seen.iter().any(|f| f == area || (area.ends_with('/') && f.starts_with(area))) {
            out.push(format!("{area}: on engine_guards::EXEMPT but there is no such file or folder; remove it"));
        }
    }
    for (p, n, _) in ADAPTERS {
        if !files_seen.contains(*p) {
            out.push(format!("{p}: adapter `{n}` is on engine_guards::ADAPTERS but the file is gone; remove it"));
        }
    }
    out.extend(adapter_gone);
    Ok(out)
}

/// Whether file `rel` is in an area on [`EXEMPT`].
fn exempt(rel: &str) -> bool {
    EXEMPT.iter().any(|(a, _)| rel == *a || (a.ends_with('/') && rel.starts_with(a)))
}

/// Whether `text` declares a function named `name`.
fn text_has_fn(text: &str, name: &str) -> bool {
    text.match_indices("fn ").any(|(i, _)| text[i + 3..].strip_prefix(name).is_some_and(|r| r.starts_with(['(', '<'])))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_adapter_is_found_by_its_whole_name() {
        assert!(text_has_fn("pub fn marker_box(at: Pt) -> Pt {", "marker_box"));
        assert!(text_has_fn("fn table<'a>(doc: &Document) {", "table"));
        assert!(!text_has_fn("fn marker_boxes(at: Pt) {", "marker_box"));
        assert!(!text_has_fn("let marker_box = 1;", "marker_box"));
    }

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
    fn macro_declared_types_are_found() {
        let text =
            "typed_id!(/// A device.\n    DeviceId, \"cmp\");\nunit!(\n    /// Volts.\n    /// More.\n    Volts,\n    \"V\"\n);\nmy_unit!(Nope);";
        assert_eq!(pub_types(text), vec!["DeviceId".to_string(), "Volts".to_string()]);
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
