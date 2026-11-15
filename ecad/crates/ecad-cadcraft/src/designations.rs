//! The project's list of functions and locations (brief 052, design 052 §5.9, FP7, FP8).
//!
//! # Semantics
//! - The project holds one list per [`Aspect`]. Each entry has a stable **key** (a fresh ID in hex,
//!   allocated when the entry is made and recorded in the op), a **code** (`+CP1`, `=F1`), unique
//!   within its aspect, and an optional **description**. They are project properties
//!   `designation.<f|l>.<key>.code` and `.description`: no new object kind, no new op (FP7).
//! - Devices, boxes, pages and the project store **the key** in `function`, `location` and
//!   `location.<type>` (FP8). Everything a person sees shows the code ([`code_of`]), so renaming an
//!   entry is one `SetProperty`.
//! - A typed code is stored through [`Pending::store`]: a known code gives its key; an unknown one
//!   adds an entry in the same transaction. A stored value with no entry (a v0.2.0 code, or a key
//!   whose entry a merge removed) shows `?` (design 052 §7).
//! - The **use index** ([`Uses`]) maps each key to the objects storing it, kept from the ledger's
//!   ops (never a rescan on an ordinary commit) and rebuilt on open and branch switch.
//!
//! Excludes: proposals and contexts (`crate::locations`), the commands
//! (`crate::designation_cmds`, `crate::location_cmds`). This module reads and plans ops only.

use std::collections::{BTreeMap, BTreeSet};

use ecad_model::book::{FUNCTION_KEY, LOCATION_KEY};
use ecad_model::{ComponentId, Document, IdAllocator, Ledger, ObjectRef, Op};

/// The prefix of every list property.
pub const PREFIX: &str = "designation.";

/// Function (`=`) or location (`+`), the two IEC 81346 aspects a device carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Aspect {
    Function,
    Location,
}

impl Aspect {
    /// Both aspects, function first (IEC 81346 order: function, location, product).
    pub const ALL: [Aspect; 2] = [Aspect::Function, Aspect::Location];

    /// The letter in the list's property keys (`f`, `l`).
    pub fn letter(self) -> char {
        match self {
            Aspect::Function => 'f',
            Aspect::Location => 'l',
        }
    }

    /// The sign a code starts with (`=`, `+`).
    pub fn sign(self) -> char {
        match self {
            Aspect::Function => '=',
            Aspect::Location => '+',
        }
    }

    /// The property that stores it (`function`, `location`).
    pub fn property(self) -> &'static str {
        match self {
            Aspect::Function => FUNCTION_KEY,
            Aspect::Location => LOCATION_KEY,
        }
    }

    /// The word a person sees (`function`, `location`).
    pub fn word(self) -> &'static str {
        self.property()
    }

    /// The plural (`functions`, `locations`).
    pub fn plural(self) -> &'static str {
        match self {
            Aspect::Function => "functions",
            Aspect::Location => "locations",
        }
    }

    /// The aspect a word names (`function`, `location`, `f`, `l`, `=`, `+`).
    pub fn of_word(w: &str) -> Option<Aspect> {
        match w.trim().to_ascii_lowercase().as_str() {
            "function" | "f" | "=" => Some(Aspect::Function),
            "location" | "l" | "+" => Some(Aspect::Location),
            _ => None,
        }
    }

    /// The aspect a stored property key belongs to: `function`; `location` and `location.<type>`.
    pub fn of_property(key: &str) -> Option<Aspect> {
        if key == FUNCTION_KEY {
            Some(Aspect::Function)
        } else if key == LOCATION_KEY || key.strip_prefix("location.").is_some_and(|t| !t.is_empty()) {
            Some(Aspect::Location)
        } else {
            None
        }
    }
}

/// One entry of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub aspect: Aspect,
    pub key: String,
    pub code: String,
    pub description: Option<String>,
}

/// The property holding entry `key`'s code.
pub fn code_property(a: Aspect, key: &str) -> String {
    format!("{PREFIX}{}.{key}.code", a.letter())
}

/// The property holding entry `key`'s description.
pub fn description_property(a: Aspect, key: &str) -> String {
    format!("{PREFIX}{}.{key}.description", a.letter())
}

/// Whether `key` is a list property (`designation.…`).
pub fn is_list_property(key: &str) -> bool {
    key.starts_with(PREFIX)
}

/// The aspect and entry key of a list property, and whether it is the code (else the
/// description).
pub fn parse_list_property(prop: &str) -> Option<(Aspect, &str, bool)> {
    let rest = prop.strip_prefix(PREFIX)?;
    let (letter, rest) = rest.split_once('.')?;
    let a = match letter {
        "f" => Aspect::Function,
        "l" => Aspect::Location,
        _ => return None,
    };
    if let Some(key) = rest.strip_suffix(".code") {
        return (!key.is_empty()).then_some((a, key, true));
    }
    rest.strip_suffix(".description").filter(|k| !k.is_empty()).map(|key| (a, key, false))
}

/// Every entry of aspect `a`, in natural order of codes (`+CP2` before `+CP10`). O(entries).
pub fn entries(doc: &Document, a: Aspect) -> Vec<Entry> {
    let start = format!("{PREFIX}{}.", a.letter());
    let mut out: Vec<Entry> = doc
        .properties
        .range(start.clone()..)
        .take_while(|(k, _)| k.starts_with(&start))
        .filter_map(|(k, v)| {
            let (_, key, is_code) = parse_list_property(k)?;
            is_code.then(|| Entry {
                aspect: a,
                key: key.to_string(),
                code: v.clone(),
                description: doc.properties.get(&description_property(a, key)).filter(|d| !d.is_empty()).cloned(),
            })
        })
        .collect();
    out.sort_by(|x, y| crate::locations::natural_cmp(&x.code, &y.code).then(x.key.cmp(&y.key)));
    out
}

/// The code of entry `key`, or `None` when the list has no such entry.
pub fn code_of(doc: &Document, a: Aspect, key: &str) -> Option<String> {
    doc.properties.get(&code_property(a, key)).cloned()
}

/// The entry `key_or_code` names: a code (starting with the aspect's sign) or a key.
pub fn find(doc: &Document, a: Aspect, key_or_code: &str) -> Option<Entry> {
    let t = key_or_code.trim();
    if t.starts_with(a.sign()) {
        return entries(doc, a).into_iter().find(|e| e.code.eq_ignore_ascii_case(t));
    }
    let code = code_of(doc, a, t)?;
    Some(Entry {
        aspect: a,
        key: t.to_string(),
        code,
        description: doc.properties.get(&description_property(a, t)).filter(|d| !d.is_empty()).cloned(),
    })
}

/// The key of the entry with `code` (case-insensitive), if any.
pub fn key_of_code(doc: &Document, a: Aspect, code: &str) -> Option<String> {
    entries(doc, a).into_iter().find(|e| e.code.eq_ignore_ascii_case(code.trim())).map(|e| e.key)
}

/// A code as typed, made canonical: trimmed, with the aspect's sign added (024's
/// `location_value`).
///
/// # Errors
/// Empty, too long or a control character.
pub fn normalise(doc: &Document, a: Aspect, text: &str) -> Result<String, String> {
    crate::devices::property_value(doc, a.property(), Some(text))?.ok_or_else(|| format!("A {} needs a code, such as {}.", a.word(), example(a)))
}

/// An example code (`+CP1`, `=F1`).
pub fn example(a: Aspect) -> &'static str {
    match a {
        Aspect::Function => "=F1",
        Aspect::Location => "+CP1",
    }
}

/// The code a stored value shows: its entry's code, or `None` when it names no entry.
pub fn shown(doc: &Document, a: Aspect, stored: Option<&str>) -> Option<String> {
    stored.and_then(|k| code_of(doc, a, k))
}

/// The text a stored value shows: its code, `?` for a key with no entry, empty when unset.
pub fn shown_text(doc: &Document, a: Aspect, stored: Option<&str>) -> String {
    match stored {
        None => String::new(),
        Some(k) => code_of(doc, a, k).unwrap_or_else(|| "?".into()),
    }
}

/// The text of property `prop` stored as `value` on any object, for generic property rows: the
/// code for an aspect property, else the value itself.
pub fn display(doc: &Document, prop: &str, value: &str) -> String {
    match Aspect::of_property(prop) {
        Some(a) => code_of(doc, a, value).unwrap_or_else(|| "?".into()),
        None => value.to_string(),
    }
}

/// Codes stored within one transaction, so two writes of one new code share one new entry.
#[derive(Debug, Default)]
pub struct Pending {
    added: BTreeMap<(Aspect, String), String>,
    /// The codes added, in order, for the echo.
    pub new_codes: Vec<(Aspect, String)>,
}

impl Pending {
    /// The key to store for `text` typed as aspect `a`: a known code's key, else a new entry's
    /// (its `SetProperty` appended to `ops`).
    ///
    /// # Errors
    /// As [`normalise`].
    pub fn store(&mut self, doc: &Document, ids: &mut IdAllocator, a: Aspect, text: &str, ops: &mut Vec<Op>) -> Result<String, String> {
        let code = normalise(doc, a, text)?;
        if let Some(k) = key_of_code(doc, a, &code) {
            return Ok(k);
        }
        let upper = code.to_uppercase();
        if let Some(k) = self.added.get(&(a, upper.clone())) {
            return Ok(k.clone());
        }
        let id: ComponentId = ids.fresh();
        let key = id.hex();
        ops.push(Op::SetProperty { object: ObjectRef::Project, key: code_property(a, &key), before: None, after: Some(code.clone()) });
        self.added.insert((a, upper), key.clone());
        self.new_codes.push((a, code));
        Ok(key)
    }

    /// The echo for the entries added: `New location +CP4 added to the project's list.`
    pub fn words(&self) -> Option<String> {
        match self.new_codes.as_slice() {
            [] => None,
            [(a, c)] => Some(format!("New {} {c} added to the project's list.", a.word())),
            many => Some(format!(
                "{} new entries added to the project's list: {}.",
                many.len(),
                many.iter().map(|(_, c)| c.as_str()).collect::<Vec<_>>().join(", ")
            )),
        }
    }
}

// ---------------------------------------------------------------- the use index

/// Who stores one entry key.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Users {
    /// (object, property) pairs: `location`, `function` or `location.<type>`.
    pub refs: BTreeSet<(ObjectRef, String)>,
}

/// How many of each kind of object use an entry.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub devices: usize,
    pub boxes: usize,
    pub pages: usize,
    /// Devices storing it in an override (`location.<type>`).
    pub overrides: usize,
    /// The project default.
    pub project: bool,
}

impl Counts {
    /// Whether nothing uses the entry.
    pub fn unused(&self) -> bool {
        self.devices == 0 && self.boxes == 0 && self.pages == 0 && self.overrides == 0 && !self.project
    }

    /// `12 devices and 1 box`, for refusals and notes.
    pub fn words(&self) -> String {
        let mut parts = Vec::new();
        let n = |k: usize, one: &str, many: &str| format!("{k} {}", if k == 1 { one } else { many });
        if self.devices > 0 {
            parts.push(n(self.devices, "device", "devices"));
        }
        if self.overrides > 0 {
            parts.push(n(self.overrides, "device part", "device parts"));
        }
        if self.boxes > 0 {
            parts.push(n(self.boxes, "box", "boxes"));
        }
        if self.pages > 0 {
            parts.push(n(self.pages, "page", "pages"));
        }
        if self.project {
            parts.push("the project".into());
        }
        match parts.len() {
            0 => "nothing".into(),
            1 => parts.remove(0),
            _ => {
                let last = parts.pop().unwrap_or_default();
                format!("{} and {last}", parts.join(", "))
            }
        }
    }
}

/// The use index: entry key → the objects storing it (design 052 §5.9). Kept from the ledger's
/// ops; rebuilt from the document when it has not been built or the ledger was replaced.
#[derive(Debug, Clone, Default)]
pub struct Uses {
    /// The log length read so far; `None` until built.
    seen: Option<usize>,
    by_key: BTreeMap<(Aspect, String), Users>,
}

impl Uses {
    /// The index of `doc`, built in one pass over components, pages and the project.
    pub fn of(doc: &Document) -> Uses {
        let mut u = Uses::default();
        for (c, x) in &doc.components {
            u.add_props(ObjectRef::Component(*c), &x.properties);
        }
        for (p, x) in &doc.pages {
            u.add_props(ObjectRef::Page(*p), &x.properties);
        }
        for (k, v) in &doc.properties {
            if let Some(a) = Aspect::of_property(k) {
                u.note(a, v, ObjectRef::Project, k, true);
            }
        }
        u
    }

    /// Forget the index (a branch switch or reload): the next [`Uses::catch_up`] rebuilds it.
    pub fn reset(&mut self) {
        *self = Uses::default();
    }

    /// Bring the index up to `ledger`'s head: the ops of the transactions not read yet, or a
    /// rebuild when it was never built or the log is shorter than what was read.
    pub fn catch_up(&mut self, ledger: &Ledger) {
        let log = ledger.log();
        match self.seen {
            Some(n) if n <= log.len() => {
                for tx in log.get(n..).unwrap_or_default() {
                    self.apply(&tx.ops);
                }
            }
            _ => *self = Uses::of(ledger.document()),
        }
        self.seen = Some(log.len());
    }

    /// Update from `ops` (as committed).
    pub fn apply(&mut self, ops: &[Op]) {
        for op in ops {
            match op {
                Op::SetProperty { object, key, before, after } => {
                    let Some(a) = Aspect::of_property(key) else { continue };
                    if let Some(b) = before {
                        self.note(a, b, *object, key, false);
                    }
                    if let Some(v) = after {
                        self.note(a, v, *object, key, true);
                    }
                }
                Op::CreateComponent { id, component } => self.add_props(ObjectRef::Component(*id), &component.properties),
                Op::DeleteComponent { id, component } => self.remove_props(ObjectRef::Component(*id), &component.properties),
                Op::AddPage { id, page, .. } => self.add_props(ObjectRef::Page(*id), &page.properties),
                Op::RemovePage { id, page, .. } => self.remove_props(ObjectRef::Page(*id), &page.properties),
                _ => {}
            }
        }
    }

    fn add_props(&mut self, o: ObjectRef, props: &ecad_model::Properties) {
        for (k, v) in props {
            if let Some(a) = Aspect::of_property(k) {
                self.note(a, v, o, k, true);
            }
        }
    }

    fn remove_props(&mut self, o: ObjectRef, props: &ecad_model::Properties) {
        for (k, v) in props {
            if let Some(a) = Aspect::of_property(k) {
                self.note(a, v, o, k, false);
            }
        }
    }

    fn note(&mut self, a: Aspect, key: &str, o: ObjectRef, prop: &str, add: bool) {
        let slot = (a, key.to_string());
        if add {
            self.by_key.entry(slot).or_default().refs.insert((o, prop.to_string()));
        } else if let Some(u) = self.by_key.get_mut(&slot) {
            u.refs.remove(&(o, prop.to_string()));
            if u.refs.is_empty() {
                self.by_key.remove(&slot);
            }
        }
    }

    /// The objects storing entry `key` of aspect `a`.
    pub fn users(&self, a: Aspect, key: &str) -> Option<&Users> {
        self.by_key.get(&(a, key.to_string()))
    }

    /// Every stored key of aspect `a` (with or without an entry).
    pub fn keys(&self, a: Aspect) -> impl Iterator<Item = &str> {
        self.by_key.keys().filter(move |(x, _)| *x == a).map(|(_, k)| k.as_str())
    }

    /// How many devices, boxes, pages and overrides use entry `key`.
    pub fn counts(&self, doc: &Document, a: Aspect, key: &str) -> Counts {
        let mut c = Counts::default();
        let Some(u) = self.users(a, key) else { return c };
        let mut devices = BTreeSet::new();
        let mut overrides = BTreeSet::new();
        for (o, prop) in &u.refs {
            match o {
                ObjectRef::Component(x) if crate::locations::is_box(doc, *x) => c.boxes += 1,
                ObjectRef::Component(x) if prop == a.property() => {
                    devices.insert(*x);
                }
                ObjectRef::Component(x) => {
                    overrides.insert(*x);
                }
                ObjectRef::Page(_) => c.pages += 1,
                ObjectRef::Project => c.project = true,
                _ => {}
            }
        }
        c.devices = devices.len();
        c.overrides = overrides.iter().filter(|x| !devices.contains(x)).count();
        c
    }
}

/// A stored `function` or `location` (or override) of any object, with what it shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stored {
    pub key: String,
    /// The entry's code, or `None` when the list has no such entry.
    pub code: Option<String>,
}

/// The stored value of `prop` on `props`, resolved through the list.
pub fn stored(doc: &Document, a: Aspect, props: &ecad_model::Properties, prop: &str) -> Option<Stored> {
    let key = props.get(prop)?;
    Some(Stored { key: key.clone(), code: code_of(doc, a, key) })
}
