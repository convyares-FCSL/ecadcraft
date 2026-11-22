//! The project's list of functions and locations (brief 052, design 052 §5.9; design 072 §5.3).
//!
//! # Semantics
//! - The project holds one list per [`Aspect`] in [`ecad_model::Project`]. Each entry has a stable
//!   id (allocated when the entry is made and recorded in the op), a **code** (`+CP1`, `=F1`),
//!   unique within its aspect, and optional descriptions. The lists change through
//!   `ModifyProject`.
//! - Pages, the project and devices store **the id** (`Page.location`/`function`,
//!   `Project.location`/`function`, `Device.location`/`function` with their `fixed` flags, design
//!   072 §6.3); a device's per-type override `location.<type>` keeps its 32-hex form in the bag. Everything a person sees
//!   shows the code ([`code_of`]), so renaming an entry is one edit of the list.
//! - A typed code is stored through [`Pending::store`]: a known code gives its key; an unknown one
//!   adds an entry in the same transaction. A stored value with no entry (a key whose entry a merge
//!   removed) shows `?` (design 052 §7).
//! - The **use index** ([`Uses`]) maps each key to the objects storing it, kept from the ledger's
//!   ops (never a rescan on an ordinary commit) and rebuilt on open and branch switch.
//!
//! Excludes: proposals and contexts (`crate::locations`), the commands
//! (`ecad_engine::commands::designation`, `ecad_engine::commands::location`). This module reads and plans ops only.

use std::collections::{BTreeMap, BTreeSet};

use crate::CmdError;
use ecad_model::document::book::{FUNCTION_KEY, LOCATION_KEY};
use ecad_model::{
    Aid, AidId, Device, Document, FunctionId, IdAllocator, Ledger, ListCode, ListDescription, ListEntry, LocationBox, LocationId, ObjectRef, Op,
    Page, Project,
};

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
    ///
    /// `location.<t>` counts only for an override type (`socket`, `plug`, or a symbol type other
    /// than `head`, design 052 FP3): other `location.*` keys, such as a box's `location.style`
    /// (brief 050a), are not locations (tester 052, finding 2).
    pub fn of_property(key: &str) -> Option<Aspect> {
        if key == FUNCTION_KEY {
            Some(Aspect::Function)
        } else if key == LOCATION_KEY || key.strip_prefix("location.").is_some_and(crate::locations::override_type_ok) {
            Some(Aspect::Location)
        } else {
            None
        }
    }
}

/// One entry of the list, as a person reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub aspect: Aspect,
    /// The entry's id in its 32-hex form (what a device stores).
    pub key: String,
    pub code: String,
    /// Description 1, if any.
    pub description: Option<String>,
}

impl Entry {
    fn of<I>(aspect: Aspect, id: u128, e: &ListEntry<I>) -> Entry {
        Entry { aspect, key: format!("{id:032x}"), code: e.code.to_string(), description: e.description().map(ToString::to_string) }
    }
}

/// Every entry of aspect `a` in list order.
fn listed(project: &Project, a: Aspect) -> Vec<Entry> {
    match a {
        Aspect::Function => project.functions.iter().map(|e| Entry::of(a, e.id.0, e)).collect(),
        Aspect::Location => project.locations.iter().map(|e| Entry::of(a, e.id.0, e)).collect(),
    }
}

/// The raw id a stored key names, if it is a key.
pub(crate) fn raw(key: &str) -> Option<u128> {
    let t = key.trim();
    (t.len() == 32).then(|| u128::from_str_radix(t, 16).ok()).flatten()
}

/// The entry of `project` with raw id `id`.
fn entry_by_id(project: &Project, a: Aspect, id: u128) -> Option<Entry> {
    match a {
        Aspect::Function => project.function(FunctionId(id)).map(|e| Entry::of(a, id, e)),
        Aspect::Location => project.location(LocationId(id)).map(|e| Entry::of(a, id, e)),
    }
}

/// The code of entry `id` in `project`.
fn code_in(project: &Project, a: Aspect, id: u128) -> Option<String> {
    match a {
        Aspect::Function => project.function(FunctionId(id)).map(|e| e.code.to_string()),
        Aspect::Location => project.location(LocationId(id)).map(|e| e.code.to_string()),
    }
}

/// The code of entry `key` in `project` (a `ModifyProject`'s side), if it has one.
pub fn code_in_project(project: &Project, a: Aspect, key: &str) -> Option<String> {
    code_in(project, a, raw(key)?)
}

/// Description 1 of entry `key`, if it has one.
pub fn description_of(doc: &Document, a: Aspect, key: &str) -> Option<String> {
    entry_by_id(&doc.project, a, raw(key)?)?.description
}

/// The keys of the entries of aspect `a` whose code differs between `before` and `after` (added,
/// removed or renamed): what the views that show codes redraw for a `ModifyProject`.
pub fn changed_codes(before: &Project, after: &Project, a: Aspect) -> Vec<String> {
    let ids = |p: &Project| -> BTreeSet<u128> {
        match a {
            Aspect::Function => p.functions.iter().map(|e| e.id.0).collect(),
            Aspect::Location => p.locations.iter().map(|e| e.id.0).collect(),
        }
    };
    ids(before).union(&ids(after)).filter(|id| code_in(before, a, **id) != code_in(after, a, **id)).map(|id| format!("{id:032x}")).collect()
}

/// Whether a `ModifyProject` from `before` to `after` changed what aspect `a` shows anywhere: a
/// code, or the project's default.
pub fn shown_changed(before: &Project, after: &Project, a: Aspect) -> bool {
    default_key(before, a) != default_key(after, a) || !changed_codes(before, after, a).is_empty()
}

/// The project's default of aspect `a`, as a key.
pub fn default_key(project: &Project, a: Aspect) -> Option<String> {
    match a {
        Aspect::Function => project.function.map(FunctionId::hex),
        Aspect::Location => project.location.map(LocationId::hex),
    }
}

/// Page `page`'s value of aspect `a` as a key: its own, else the project's.
pub fn inherited_key(doc: &Document, page: ecad_model::PageId, a: Aspect) -> Option<String> {
    doc.pages.get(&page).and_then(|p| page_key(p, a)).or_else(|| default_key(&doc.project, a))
}

/// Page `page`'s own value of aspect `a`, as a key.
pub fn page_key(page: &Page, a: Aspect) -> Option<String> {
    match a {
        Aspect::Function => page.function.map(FunctionId::hex),
        Aspect::Location => page.location.map(LocationId::hex),
    }
}

/// `page` with its value of aspect `a` set to `key` (`None`, or text that is not a key, clears it).
pub fn with_page_key(page: &Page, a: Aspect, key: Option<&str>) -> Page {
    let mut p = page.clone();
    let id = key.and_then(raw);
    match a {
        Aspect::Function => p.function = id.map(FunctionId),
        Aspect::Location => p.location = id.map(LocationId),
    }
    p
}

/// Device `d`'s own value of aspect `a`, as a key.
pub fn device_key(d: &Device, a: Aspect) -> Option<String> {
    match a {
        Aspect::Function => d.function.map(FunctionId::hex),
        Aspect::Location => d.location.map(LocationId::hex),
    }
}

/// Whether device `d`'s value of aspect `a` was set by hand (design 072 §6.3).
pub fn device_fixed(d: &Device, a: Aspect) -> bool {
    match a {
        Aspect::Function => d.function_fixed,
        Aspect::Location => d.location_fixed,
    }
}

/// `d` with its value of aspect `a` set to `key` (`None`, or text that is not a key, clears it)
/// and its `fixed` flag set to `fixed` (cleared with the value).
pub fn with_device_key(d: &Device, a: Aspect, key: Option<&str>, fixed: bool) -> Device {
    let mut x = d.clone();
    let id = key.and_then(raw);
    let fixed = fixed && id.is_some();
    match a {
        Aspect::Function => {
            x.function = id.map(FunctionId);
            x.function_fixed = fixed;
        }
        Aspect::Location => {
            x.location = id.map(LocationId);
            x.location_fixed = fixed;
        }
    }
    x
}

/// Location box `b`'s value of aspect `a`, as a key.
pub fn box_key(b: &LocationBox, a: Aspect) -> Option<String> {
    match a {
        Aspect::Function => b.function.map(FunctionId::hex),
        Aspect::Location => b.location.map(LocationId::hex),
    }
}

/// `b` with its value of aspect `a` set to `key` (`None`, or text that is not a key, clears it).
pub fn with_box_key(b: &LocationBox, a: Aspect, key: Option<&str>) -> LocationBox {
    let mut x = b.clone();
    let id = key.and_then(raw);
    match a {
        Aspect::Function => x.function = id.map(FunctionId),
        Aspect::Location => x.location = id.map(LocationId),
    }
    x
}

/// Every key device `d` stores, with the property it is under: its own location and function,
/// and its location overrides (`location.<type>`, still in its bag).
pub fn device_keys(d: &Device) -> Vec<(String, String)> {
    let own = Aspect::ALL.into_iter().filter_map(|a| device_key(d, a).map(|k| (a.property().to_string(), k)));
    let overrides = d.properties.iter().filter(|(k, _)| Aspect::of_property(k).is_some()).map(|(k, v)| (k.clone(), v.clone()));
    own.chain(overrides).collect()
}

/// `project` with its default of aspect `a` set to `key` (`None` clears it).
pub fn with_default_key(project: &Project, a: Aspect, key: Option<&str>) -> Project {
    let mut p = project.clone();
    let id = key.and_then(raw);
    match a {
        Aspect::Function => p.function = id.map(FunctionId),
        Aspect::Location => p.location = id.map(LocationId),
    }
    p
}

/// A code as typed, checked against the list's text bounds.
///
/// # Errors
/// Longer than [`ecad_model::document::project::MAX_CODE`] or with a hidden character.
pub fn list_code(code: &str) -> Result<ListCode, CmdError> {
    ListCode::new(code).map_err(|e| CmdError::refused(format!("\"{code}\" is not a code: {e}.")))
}

/// A description as typed, checked against the list's text bounds.
///
/// # Errors
/// Too long, several lines or a hidden character.
pub fn list_description(text: &str) -> Result<ListDescription, CmdError> {
    ListDescription::new(text).map_err(|e| CmdError::refused(format!("That description is refused: {e}.")))
}

/// `project` with entry `key` of aspect `a` given `code` and `description` (description 1;
/// `None` removes every description). Nothing changes when it has no such entry.
pub fn with_entry(project: &Project, a: Aspect, key: &str, code: ListCode, description: Option<ListDescription>) -> Project {
    let mut p = project.clone();
    let Some(id) = raw(key) else { return p };
    let set = |c: &mut ListCode, d: &mut Vec<ListDescription>| {
        *c = code;
        match description {
            Some(x) if d.is_empty() => d.push(x),
            Some(x) => {
                if let Some(first) = d.first_mut() {
                    *first = x;
                }
            }
            None => d.clear(),
        }
    };
    match a {
        Aspect::Function => {
            if let Some(e) = p.functions.iter_mut().find(|e| e.id.0 == id) {
                set(&mut e.code, &mut e.descriptions);
            }
        }
        Aspect::Location => {
            if let Some(e) = p.locations.iter_mut().find(|e| e.id.0 == id) {
                set(&mut e.code, &mut e.descriptions);
            }
        }
    }
    p
}

/// `project` with a new entry of aspect `a`.
pub fn with_new_entry(project: &Project, a: Aspect, id: u128, code: ListCode, description: Option<ListDescription>) -> Project {
    let mut p = project.clone();
    let descriptions: Vec<ListDescription> = description.into_iter().collect();
    match a {
        Aspect::Function => p.functions.push(ListEntry { id: FunctionId(id), code, tag: None, descriptions }),
        Aspect::Location => p.locations.push(ListEntry { id: LocationId(id), code, tag: None, descriptions }),
    }
    p
}

/// `project` without entry `key` of aspect `a`.
pub fn without_entry(project: &Project, a: Aspect, key: &str) -> Project {
    let mut p = project.clone();
    let id = raw(key);
    match a {
        Aspect::Function => p.functions.retain(|e| Some(e.id.0) != id),
        Aspect::Location => p.locations.retain(|e| Some(e.id.0) != id),
    }
    p
}

/// Every entry of aspect `a`, in natural order of codes (`+CP2` before `+CP10`). O(entries).
pub fn entries(doc: &Document, a: Aspect) -> Vec<Entry> {
    let mut out = listed(&doc.project, a);
    out.sort_by(|x, y| crate::locations::natural_cmp(&x.code, &y.code).then(x.key.cmp(&y.key)));
    out
}

/// The code of entry `key`, or `None` when the list has no such entry.
pub fn code_of(doc: &Document, a: Aspect, key: &str) -> Option<String> {
    code_in(&doc.project, a, raw(key)?)
}

/// The entry `key_or_code` names: a code (starting with the aspect's sign) or a key.
///
/// A code may be typed without its sign (`CP1`), as `setlocation` and `designationadd` take it
/// (tester 052, finding 5): text that is not a key is read as a code.
pub fn find(doc: &Document, a: Aspect, key_or_code: &str) -> Option<Entry> {
    let t = key_or_code.trim();
    let by_code = |c: &str| entries(doc, a).into_iter().find(|e| e.code.eq_ignore_ascii_case(c));
    if t.starts_with(a.sign()) {
        return by_code(t);
    }
    match raw(t).and_then(|id| entry_by_id(&doc.project, a, id)) {
        Some(e) => Some(e),
        None => by_code(&format!("{}{t}", a.sign())),
    }
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
pub fn normalise(doc: &Document, a: Aspect, text: &str) -> Result<String, CmdError> {
    crate::devices::property_value(doc, a.property(), Some(text))?
        .ok_or_else(|| CmdError::refused(format!("A {} needs a code, such as {}.", a.word(), example(a))))
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
    /// The project as the entries added so far leave it.
    working: Option<Project>,
    /// The codes added, in order, for the echo.
    pub new_codes: Vec<(Aspect, String)>,
}

impl Pending {
    /// The key to store for `text` typed as aspect `a`: a known code's key, else a new entry's
    /// (its `ModifyProject` appended to `ops`).
    ///
    /// # Errors
    /// As [`normalise`] and [`list_code`].
    pub fn store(&mut self, doc: &Document, ids: &mut IdAllocator, a: Aspect, text: &str, ops: &mut Vec<Op>) -> Result<String, CmdError> {
        let code = normalise(doc, a, text)?;
        if let Some(k) = key_of_code(doc, a, &code) {
            return Ok(k);
        }
        let upper = code.to_uppercase();
        if let Some(k) = self.added.get(&(a, upper.clone())) {
            return Ok(k.clone());
        }
        let typed = list_code(&code)?;
        let id: u128 = ids.fresh();
        let key = format!("{id:032x}");
        let before = self.working.take().unwrap_or_else(|| doc.project.clone());
        let after = with_new_entry(&before, a, id, typed, None);
        ops.push(Op::ModifyProject { before, after: after.clone() });
        self.working = Some(after);
        self.added.insert((a, upper), key.clone());
        self.new_codes.push((a, code));
        Ok(key)
    }

    /// Note a `ModifyProject` the caller recorded in the same transaction, so a later entry
    /// starts from it.
    pub fn set_project(&mut self, project: Project) {
        self.working = Some(project);
    }

    /// The project as the entries added so far leave it (`doc`'s when none were added): what a
    /// later `ModifyProject` in the same transaction starts from.
    pub fn project<'a>(&'a self, doc: &'a Document) -> &'a Project {
        self.working.as_ref().unwrap_or(&doc.project)
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
    /// The location boxes carrying it.
    pub boxes: BTreeSet<AidId>,
}

impl Users {
    fn is_empty(&self) -> bool {
        self.refs.is_empty() && self.boxes.is_empty()
    }
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
        for (c, x) in &doc.devices {
            u.device(*c, x, true);
        }
        for (p, x) in &doc.pages {
            u.page(*p, x, true);
        }
        for (a, x) in &doc.aids {
            u.aid(*a, x, true);
        }
        u.project(&doc.project, true);
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
                Op::CreateDevice { id, device } => self.device(*id, device, true),
                Op::DeleteDevice { id, device } => self.device(*id, device, false),
                Op::ModifyDevice { id, before, after } => {
                    self.device(*id, before, false);
                    self.device(*id, after, true);
                }
                Op::CreateAid { id, aid } => self.aid(*id, aid, true),
                Op::DeleteAid { id, aid } => self.aid(*id, aid, false),
                Op::ModifyAid { id, before, after } => {
                    self.aid(*id, before, false);
                    self.aid(*id, after, true);
                }
                Op::AddPage { id, page, .. } => self.page(*id, page, true),
                Op::RemovePage { id, page, .. } => self.page(*id, page, false),
                Op::ModifyPage { id, before, after } => {
                    self.page(*id, before, false);
                    self.page(*id, after, true);
                }
                Op::ModifyProject { before, after } => {
                    self.project(before, false);
                    self.project(after, true);
                }
                _ => {}
            }
        }
    }

    /// Note (or forget) a page's own location and function.
    fn page(&mut self, id: ecad_model::PageId, page: &Page, add: bool) {
        for a in Aspect::ALL {
            if let Some(k) = page_key(page, a) {
                self.note(a, &k, ObjectRef::Page(id), a.property(), add);
            }
        }
    }

    /// Note (or forget) the project's defaults.
    fn project(&mut self, project: &Project, add: bool) {
        for a in Aspect::ALL {
            if let Some(k) = default_key(project, a) {
                self.note(a, &k, ObjectRef::Project, a.property(), add);
            }
        }
    }

    /// Note (or forget) every key a device stores ([`device_keys`]).
    fn device(&mut self, id: ecad_model::DeviceId, d: &Device, add: bool) {
        for (prop, k) in device_keys(d) {
            if let Some(a) = Aspect::of_property(&prop) {
                self.note(a, &k, ObjectRef::Device(id), &prop, add);
            }
        }
    }

    /// Note (or forget) the keys a location box carries.
    fn aid(&mut self, id: AidId, aid: &Aid, add: bool) {
        let Some(b) = aid.as_box() else { return };
        for a in Aspect::ALL {
            let Some(k) = box_key(b, a) else { continue };
            let slot = (a, k);
            if add {
                self.by_key.entry(slot).or_default().boxes.insert(id);
            } else if let Some(u) = self.by_key.get_mut(&slot) {
                u.boxes.remove(&id);
                if u.is_empty() {
                    self.by_key.remove(&slot);
                }
            }
        }
    }

    fn note(&mut self, a: Aspect, key: &str, o: ObjectRef, prop: &str, add: bool) {
        let slot = (a, key.to_string());
        if add {
            self.by_key.entry(slot).or_default().refs.insert((o, prop.to_string()));
        } else if let Some(u) = self.by_key.get_mut(&slot) {
            u.refs.remove(&(o, prop.to_string()));
            if u.is_empty() {
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
    pub fn counts(&self, a: Aspect, key: &str) -> Counts {
        let mut c = Counts::default();
        let Some(u) = self.users(a, key) else { return c };
        let mut devices = BTreeSet::new();
        let mut overrides = BTreeSet::new();
        for (o, prop) in &u.refs {
            match o {
                ObjectRef::Device(x) if prop == a.property() => {
                    devices.insert(*x);
                }
                ObjectRef::Device(x) => {
                    overrides.insert(*x);
                }
                ObjectRef::Page(_) => c.pages += 1,
                ObjectRef::Project => c.project = true,
                _ => {}
            }
        }
        c.boxes = u.boxes.len();
        c.devices = devices.len();
        c.overrides = overrides.iter().filter(|x| !devices.contains(x)).count();
        c
    }
}
