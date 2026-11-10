//! Three-way merge of two branches' states (brief 020b, design 020 §5–§6).
//!
//! # Semantics
//! - The inputs are three states (the common **ancestor**, the **target** the merge is appended
//!   to, the **source** merged from) and the **scope**: every object either side's steps named
//!   since the fork. Only objects in the scope are compared, so the cost follows what changed,
//!   not the project's size (an object outside it is the same in all three states).
//! - Per object and per property (design §5.1): changed on one side only → that side; changed
//!   alike on both → kept; changed differently → a **value conflict**; removed on one side and
//!   changed on the other → a **removed conflict**. Added on one side → added.
//! - Rules the result must keep are conflicts too, each with a choice that satisfies it, so the
//!   result always passes the ledger's check (design §5.3): an object kept that refers to one
//!   removed (**needed**: bring it back, or leave it unconnected / remove what uses it), two
//!   devices with one tag and two pages with one number (renumber the source's or the
//!   target's), two sibling folders with one name (rename one), page or folder order changed
//!   differently on both sides.
//! - Nothing is preselected (owner answer 7): an unanswered conflict leaves the target's side in
//!   the provisional result and is listed in [`Plan::unanswered`]; [`Plan::ops`] is empty until
//!   every conflict has a choice. Never a dead end: every conflict offers at least two choices.
//! - The result is ordinary ops from the target's state ([`crate::diff::ops_between_in`]) over
//!   the scope and what the rules added to it.
//! - Version names are the target's (they are branch-local history); project properties are
//!   merged only where `keep_project_key` allows (revision fields are never merged into the
//!   release line, owner answer 10).
//!
//! Excluded: "make them one device" for a duplicate tag and "combine" for sibling folders (the
//! renumber and rename choices cover them); an inline editor for anything but property values.

use std::collections::{BTreeMap, BTreeSet};

use crate::diff;
use crate::doc::{
    Cable, CableMember, Component, Conductor, Document, End, Folder, Form, Item, ItemKind, Page, Properties, Representation, Subject, Terminal,
};
use crate::id::{CableId, ComponentId, ConductorId, DefinitionId, FolderId, ItemId, LayerId, PageId, RepresentationId, TerminalId};
use crate::invariant::Touched;
use crate::op::{ObjectRef, Op};
use crate::tag::{self, HighWater};

/// An object a merge compares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Obj {
    Page(PageId),
    Folder(FolderId),
    Layer(LayerId),
    Definition(DefinitionId),
    Component(ComponentId),
    Terminal(TerminalId),
    Representation(RepresentationId),
    Item(ItemId),
    Conductor(ConductorId),
    Cable(CableId),
    Project,
}

impl Obj {
    /// The object's ID as 32 hex digits (`project` for the project).
    pub fn hex(self) -> String {
        match self {
            Obj::Page(i) => i.hex(),
            Obj::Folder(i) => i.hex(),
            Obj::Layer(i) => i.hex(),
            Obj::Definition(i) => i.hex(),
            Obj::Component(i) => i.hex(),
            Obj::Terminal(i) => i.hex(),
            Obj::Representation(i) => i.hex(),
            Obj::Item(i) => i.hex(),
            Obj::Conductor(i) => i.hex(),
            Obj::Cable(i) => i.hex(),
            Obj::Project => "project".into(),
        }
    }

    /// The kind in words (`device`, `wire`, …).
    pub fn kind(self) -> &'static str {
        match self {
            Obj::Page(_) => "page",
            Obj::Folder(_) => "folder",
            Obj::Layer(_) => "layer",
            Obj::Definition(_) => "symbol definition",
            Obj::Component(_) => "device",
            Obj::Terminal(_) => "connection point",
            Obj::Representation(_) => "symbol",
            Obj::Item(_) => "drawing item",
            Obj::Conductor(_) => "wire",
            Obj::Cable(_) => "cable",
            Obj::Project => "project",
        }
    }
}

/// What either side changed since the fork: the objects its steps named, and whether they
/// changed the project's own properties or settings.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scope {
    pub touched: Touched,
    pub project: bool,
}

impl Scope {
    /// Add what `ops` name.
    pub fn add_ops(&mut self, ops: &[Op]) {
        for op in ops {
            self.touched.add(op);
            if matches!(op, Op::SetProperty { object: ObjectRef::Project, .. } | Op::SetProjectSettings { .. }) {
                self.project = true;
            }
        }
    }

    /// How many objects it names.
    pub fn len(&self) -> usize {
        let t = &self.touched;
        t.pages.len()
            + t.page_properties.len()
            + t.layers.len()
            + t.items.len()
            + t.components.len()
            + t.terminals.len()
            + t.representations.len()
            + t.conductors.len()
            + t.cables.len()
            + t.definitions.len()
            + t.folders.len()
            + usize::from(self.project)
    }

    /// Whether it names nothing.
    pub fn is_empty(&self) -> bool {
        self.len() == 0 && !self.touched.page_order && !self.touched.folder_order
    }
}

/// A choice for a conflict. In commands the sides are named by branch (`main`, `branch`); the
/// caller maps them to [`Choice::Target`] and [`Choice::Source`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    /// Keep the target's side.
    Target,
    /// Keep the source's side.
    Source,
    /// A typed value (property values only).
    Value(String),
    /// Bring back what the other side removed (and what it needs).
    BringBack,
    /// Leave it unconnected / remove what uses it.
    Drop,
    /// Renumber (rename) the source's object.
    RenumberSource,
    /// Renumber (rename) the target's object.
    RenumberTarget,
}

impl Choice {
    /// Whether `self` is the same choice as `other` (a typed value matches any typed value).
    fn same_kind(&self, other: &Choice) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

/// What a conflict is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictKind {
    /// The same property or field changed differently on both sides.
    Value,
    /// Removed on one side, changed on the other.
    Removed,
    /// Something kept refers to something removed.
    Needed,
    /// Two devices now share a tag.
    Tag,
    /// Two pages now share a number.
    PageNumber,
    /// Two sibling folders now share a name.
    FolderName,
    /// The page or folder order changed differently on both sides.
    Order,
}

/// One conflict and its choices (design §5.2–§5.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// Stable: `<object hex>/<property>` (`…/prop.rating`, `…/end.b`, `…/removed`,
    /// `…/needed`), or `rule/…` for the rules.
    pub id: String,
    pub kind: ConflictKind,
    pub object: Obj,
    /// What it is about (`-K01 · Rating`).
    pub what: String,
    /// The target's side in words.
    pub target: String,
    /// The source's side in words.
    pub source: String,
    /// The choices offered, in order ([`Choice::Value`] means an Edit field is offered).
    pub choices: Vec<Choice>,
    /// The choice made, if any.
    pub chosen: Option<Choice>,
}

impl Conflict {
    fn allows(&self, c: &Choice) -> bool {
        self.choices.iter().any(|x| x.same_kind(c))
    }
}

/// One change the source made since the ancestor (the preview's "Changes from the branch").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// `+` added, `~` changed, `−` removed.
    pub sign: char,
    pub object: Obj,
    pub text: String,
    /// The page it lies on, when it has one.
    pub page: Option<PageId>,
}

/// A merge, worked out (nothing is written).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    /// The source's changes since the ancestor, by page then text.
    pub changes: Vec<Change>,
    /// The pages those changes lie on, in book order of the source.
    pub pages: Vec<PageId>,
    pub conflicts: Vec<Conflict>,
    /// Conflicts without a (valid) choice.
    pub unanswered: Vec<String>,
    /// The merge's ops from the target's state; empty while a conflict is unanswered, while
    /// [`Plan::problem`] is set, or when there is nothing to change.
    pub ops: Vec<Op>,
    /// Answers that contradict each other, or a result that would still break a rule: what to
    /// change before Merge (never a refusal after the preview said yes).
    pub problem: Option<String>,
}

/// What a merge reads.
pub struct Inputs<'a> {
    pub ancestor: &'a Document,
    pub target: &'a Document,
    pub source: &'a Document,
    pub scope: &'a Scope,
    /// The sides' names, for the conflict texts (`main`, `"Option B"`).
    pub target_name: &'a str,
    pub source_name: &'a str,
    /// Project property keys a merge may write.
    pub keep_project_key: &'a dyn Fn(&str) -> bool,
    /// The larger of both sides' tag high-water marks (renumbering takes the next free number).
    pub high: &'a HighWater,
}

/// How a kept object uses another one (what "leave it unconnected / remove what uses it" does).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Use {
    End(End),
    ConnectionPoint,
    Member,
    PageFolder,
    FolderParent,
    RepItem,
    ComponentDefinition,
    /// The object cannot exist without it: removed with it.
    Whole,
}

struct Ctx<'a, 'b> {
    inp: &'a Inputs<'b>,
    choices: &'a BTreeMap<String, Choice>,
    conflicts: Vec<Conflict>,
    seen: BTreeSet<String>,
    problems: Vec<String>,
}

impl Ctx<'_, '_> {
    /// Record `c` (once per ID) and return its valid choice, if any.
    fn ask(&mut self, mut c: Conflict) -> Option<Choice> {
        let chosen = self.choices.get(&c.id).filter(|ch| c.allows(ch)).cloned();
        if self.seen.insert(c.id.clone()) {
            c.chosen = chosen.clone();
            self.conflicts.push(c);
        }
        chosen
    }

    fn t(&self) -> &str {
        self.inp.target_name
    }

    fn s(&self) -> &str {
        self.inp.source_name
    }
}

/// The three-way pick: `Some(value)` when it resolves, `None` for a conflict.
fn pick<T: PartialEq + Clone>(a: Option<&T>, t: Option<&T>, s: Option<&T>) -> Option<Option<T>> {
    if t == s || a == s {
        Some(t.cloned())
    } else if a == t {
        Some(s.cloned())
    } else {
        None
    }
}

/// A property key in words (`rating` → `Rating`, `wire_number` → `Wire number`).
fn key_words(k: &str) -> String {
    let t = k.replace('_', " ");
    let mut c = t.chars();
    c.next().map_or_else(String::new, |f| f.to_uppercase().collect::<String>() + c.as_str())
}

fn shown(v: Option<&String>) -> String {
    v.map_or_else(|| "(none)".to_string(), Clone::clone)
}

/// Merge property maps: per key (design §5.1).
fn merge_props(
    ctx: &mut Ctx<'_, '_>, obj: Obj, what: &str, a: &Properties, t: &Properties, s: &Properties, keep: &dyn Fn(&str) -> bool,
) -> Properties {
    let mut out = t.clone();
    let keys: BTreeSet<&String> = a.keys().chain(t.keys()).chain(s.keys()).collect();
    for k in keys {
        if !keep(k) {
            continue;
        }
        let v = match pick(a.get(k), t.get(k), s.get(k)) {
            Some(v) => v,
            None => {
                let c = Conflict {
                    id: format!("{}/prop.{k}", obj.hex()),
                    kind: ConflictKind::Value,
                    object: obj,
                    what: format!("{what} · {}", key_words(k)),
                    target: shown(t.get(k)),
                    source: shown(s.get(k)),
                    choices: vec![Choice::Target, Choice::Source, Choice::Value(String::new())],
                    chosen: None,
                };
                match ctx.ask(c) {
                    Some(Choice::Source) => s.get(k).cloned(),
                    Some(Choice::Value(x)) => Some(x).filter(|x| !x.is_empty()),
                    _ => t.get(k).cloned(),
                }
            }
        };
        match v {
            Some(v) => out.insert(k.clone(), v),
            None => out.remove(k),
        };
    }
    out
}

/// Merge one field as a whole value.
#[allow(clippy::too_many_arguments)]
fn field<T: PartialEq + Clone>(ctx: &mut Ctx<'_, '_>, obj: Obj, name: &str, what: String, a: &T, t: &T, s: &T, show: &dyn Fn(&T) -> String) -> T {
    match pick(Some(a), Some(t), Some(s)) {
        Some(Some(v)) => v,
        _ => {
            let c = Conflict {
                id: format!("{}/{name}", obj.hex()),
                kind: ConflictKind::Value,
                object: obj,
                what,
                target: show(t),
                source: show(s),
                choices: vec![Choice::Target, Choice::Source],
                chosen: None,
            };
            match ctx.ask(c) {
                Some(Choice::Source) => s.clone(),
                _ => t.clone(),
            }
        }
    }
}

/// Merge one object: whole when it resolves, else by `fields` when both sides kept it, else a
/// removed conflict.
fn object<T: PartialEq + Clone>(
    ctx: &mut Ctx<'_, '_>, obj: Obj, what: &str, a: Option<&T>, t: Option<&T>, s: Option<&T>, fields: impl FnOnce(&mut Ctx<'_, '_>, &T, &T, &T) -> T,
) -> Option<T> {
    if let Some(v) = pick(a, t, s) {
        return v;
    }
    match (t, s) {
        (Some(tv), Some(sv)) => Some(fields(ctx, a.unwrap_or(tv), tv, sv)),
        (None, Some(sv)) => {
            let c = Conflict {
                id: format!("{}/removed", obj.hex()),
                kind: ConflictKind::Removed,
                object: obj,
                what: format!("{what} · removed on {}; changed on {}", ctx.t(), ctx.s()),
                target: "removed".into(),
                source: "changed".into(),
                choices: vec![Choice::Target, Choice::Source],
                chosen: None,
            };
            match ctx.ask(c) {
                Some(Choice::Source) => Some(sv.clone()),
                _ => None,
            }
        }
        (Some(tv), None) => {
            let c = Conflict {
                id: format!("{}/removed", obj.hex()),
                kind: ConflictKind::Removed,
                object: obj,
                what: format!("{what} · changed on {}; removed on {}", ctx.t(), ctx.s()),
                target: "changed".into(),
                source: "removed".into(),
                choices: vec![Choice::Target, Choice::Source],
                chosen: None,
            };
            match ctx.ask(c) {
                Some(Choice::Source) => None,
                _ => Some(tv.clone()),
            }
        }
        (None, None) => None,
    }
}

/// The name of `obj` for texts, from whichever state holds it.
pub fn name_of(docs: &[&Document], obj: Obj) -> String {
    let tag_of = |c: ComponentId| {
        docs.iter().find_map(|d| d.components.get(&c)).and_then(|c| c.properties.get(tag::TAG_KEY).cloned()).unwrap_or_else(|| c.short())
    };
    let page_of = |p: PageId| {
        docs.iter()
            .find(|d| d.pages.contains_key(&p))
            .map_or_else(|| p.short(), |d| format!("Page {}", crate::book::stored_number(d, p).map_or_else(|| p.short(), str::to_string)))
    };
    match obj {
        Obj::Component(c) => tag_of(c),
        Obj::Terminal(t) => match docs.iter().find_map(|d| d.terminals.get(&t)) {
            Some(term) => format!("{}:{}", tag_of(term.component), term.label.clone().or_else(|| term.key.clone()).unwrap_or_else(|| t.short())),
            None => t.short(),
        },
        Obj::Conductor(c) => {
            match docs.iter().find_map(|d| d.conductors.get(&c)).and_then(|w| w.properties.get(crate::connectivity::WIRE_NUMBER).cloned()) {
                Some(n) => format!("Wire {n}"),
                None => format!("Wire {}", c.short()),
            }
        }
        Obj::Cable(c) => match docs.iter().find_map(|d| d.cables.get(&c)).and_then(|w| w.properties.get(tag::TAG_KEY).cloned()) {
            Some(n) => format!("Cable {n}"),
            None => format!("Cable {}", c.short()),
        },
        Obj::Page(p) => {
            let title = docs.iter().find_map(|d| d.pages.get(&p)).map(|p| p.title.clone()).unwrap_or_default();
            if title.is_empty() { page_of(p) } else { format!("{} \"{title}\"", page_of(p)) }
        }
        Obj::Folder(f) => format!("Folder \"{}\"", docs.iter().find_map(|d| d.folders.get(&f)).map_or_else(|| f.short(), |f| f.name.clone())),
        Obj::Layer(l) => format!("Layer \"{}\"", docs.iter().find_map(|d| d.layers.get(&l)).map_or_else(|| l.short(), |l| l.name.clone())),
        Obj::Definition(d) => format!("Symbol \"{}\"", docs.iter().find_map(|x| x.definitions.get(&d)).map_or_else(|| d.short(), |d| d.name.clone())),
        Obj::Representation(r) => match docs.iter().find_map(|d| d.representations.get(&r)) {
            Some(rep) => {
                let subject = match rep.subject {
                    Subject::Component(c) => tag_of(c),
                    Subject::Conductor(c) => name_of(docs, Obj::Conductor(c)),
                    Subject::Cable(c) => name_of(docs, Obj::Cable(c)),
                };
                format!("{subject} symbol on {}", page_of(rep.page).to_lowercase())
            }
            None => format!("Symbol {}", r.short()),
        },
        Obj::Item(i) => match docs.iter().find_map(|d| d.items.get(&i)) {
            Some(item) => format!("Drawing item on {}", page_of(item.page).to_lowercase()),
            None => format!("Drawing item {}", i.short()),
        },
        Obj::Project => "Project".into(),
    }
}

/// The page `obj` lies on in `doc`.
fn page_in(doc: &Document, obj: Obj) -> Option<PageId> {
    match obj {
        Obj::Page(p) => Some(p),
        Obj::Item(i) => doc.items.get(&i).map(|i| i.page),
        Obj::Representation(r) => doc.representations.get(&r).map(|r| r.page),
        Obj::Component(c) => doc.representations_of(Subject::Component(c)).map(|(_, r)| r.page).next(),
        Obj::Conductor(c) => doc.representations_of(Subject::Conductor(c)).map(|(_, r)| r.page).next(),
        Obj::Cable(c) => doc.representations_of(Subject::Cable(c)).map(|(_, r)| r.page).next(),
        Obj::Terminal(t) => doc.terminals.get(&t).and_then(|t| page_in(doc, Obj::Component(t.component))),
        _ => None,
    }
}

/// Every object of the scope, in a fixed order.
fn scope_objects(scope: &Scope) -> Vec<Obj> {
    let t = &scope.touched;
    let mut out: Vec<Obj> = Vec::new();
    let pages: BTreeSet<PageId> = t.pages.iter().chain(&t.page_properties).copied().collect();
    out.extend(pages.into_iter().map(Obj::Page));
    out.extend(t.folders.iter().map(|x| Obj::Folder(*x)));
    out.extend(t.layers.iter().map(|x| Obj::Layer(*x)));
    out.extend(t.definitions.iter().map(|x| Obj::Definition(*x)));
    out.extend(t.components.iter().map(|x| Obj::Component(*x)));
    out.extend(t.terminals.iter().map(|x| Obj::Terminal(*x)));
    out.extend(t.representations.iter().map(|x| Obj::Representation(*x)));
    out.extend(t.items.iter().map(|x| Obj::Item(*x)));
    out.extend(t.conductors.iter().map(|x| Obj::Conductor(*x)));
    out.extend(t.cables.iter().map(|x| Obj::Cable(*x)));
    if scope.project {
        out.push(Obj::Project);
    }
    out
}

/// Whether `obj` differs between `a` and `b`.
fn differs(a: &Document, b: &Document, obj: Obj) -> bool {
    match obj {
        Obj::Page(i) => a.pages.get(&i) != b.pages.get(&i),
        Obj::Folder(i) => a.folders.get(&i) != b.folders.get(&i),
        Obj::Layer(i) => a.layers.get(&i) != b.layers.get(&i),
        Obj::Definition(i) => a.definitions.get(&i) != b.definitions.get(&i),
        Obj::Component(i) => a.components.get(&i) != b.components.get(&i),
        Obj::Terminal(i) => a.terminals.get(&i) != b.terminals.get(&i),
        Obj::Representation(i) => a.representations.get(&i) != b.representations.get(&i),
        Obj::Item(i) => a.items.get(&i) != b.items.get(&i),
        Obj::Conductor(i) => a.conductors.get(&i) != b.conductors.get(&i),
        Obj::Cable(i) => a.cables.get(&i) != b.cables.get(&i),
        Obj::Project => a.properties != b.properties || a.settings != b.settings,
    }
}

/// Whether `doc` holds `obj`.
fn holds(doc: &Document, obj: Obj) -> bool {
    match obj {
        Obj::Page(i) => doc.pages.contains_key(&i),
        Obj::Folder(i) => doc.folders.contains_key(&i),
        Obj::Layer(i) => doc.layers.contains_key(&i),
        Obj::Definition(i) => doc.definitions.contains_key(&i),
        Obj::Component(i) => doc.components.contains_key(&i),
        Obj::Terminal(i) => doc.terminals.contains_key(&i),
        Obj::Representation(i) => doc.representations.contains_key(&i),
        Obj::Item(i) => doc.items.contains_key(&i),
        Obj::Conductor(i) => doc.conductors.contains_key(&i),
        Obj::Cable(i) => doc.cables.contains_key(&i),
        Obj::Project => true,
    }
}

/// Copy `obj` from `from` into `to` (or remove it there when `from` lacks it).
fn copy(to: &mut Document, from: &Document, obj: Obj) {
    fn put<K: Ord + Copy, V: Clone>(to: &mut BTreeMap<K, V>, from: &BTreeMap<K, V>, k: K) {
        match from.get(&k) {
            Some(v) => to.insert(k, v.clone()),
            None => to.remove(&k),
        };
    }
    match obj {
        Obj::Page(i) => put(&mut to.pages, &from.pages, i),
        Obj::Folder(i) => put(&mut to.folders, &from.folders, i),
        Obj::Layer(i) => put(&mut to.layers, &from.layers, i),
        Obj::Definition(i) => put(&mut to.definitions, &from.definitions, i),
        Obj::Component(i) => put(&mut to.components, &from.components, i),
        Obj::Terminal(i) => put(&mut to.terminals, &from.terminals, i),
        Obj::Representation(i) => put(&mut to.representations, &from.representations, i),
        Obj::Item(i) => put(&mut to.items, &from.items, i),
        Obj::Conductor(i) => put(&mut to.conductors, &from.conductors, i),
        Obj::Cable(i) => put(&mut to.cables, &from.cables, i),
        Obj::Project => {}
    }
}

fn set<K: Ord + Copy, V>(map: &mut BTreeMap<K, V>, k: K, v: Option<V>) {
    match v {
        Some(v) => map.insert(k, v),
        None => map.remove(&k),
    };
}

/// Merge every object of the scope into `m` (a copy of the target).
fn merge_objects(ctx: &mut Ctx<'_, '_>, m: &mut Document, objects: &[Obj]) {
    let (a, t, s) = (ctx.inp.ancestor, ctx.inp.target, ctx.inp.source);
    let docs = [t, s, a];
    let all = |_: &str| true;
    for &obj in objects {
        let what = name_of(&docs, obj);
        match obj {
            Obj::Component(id) => {
                let v = object(ctx, obj, &what, a.components.get(&id), t.components.get(&id), s.components.get(&id), |ctx, a, t, s| Component {
                    properties: merge_props(ctx, obj, &what, &a.properties, &t.properties, &s.properties, &all),
                    definition: field(ctx, obj, "definition", format!("{what} · symbol"), &a.definition, &t.definition, &s.definition, &|d| {
                        d.map_or_else(|| "(none)".into(), |d| name_of(&docs, Obj::Definition(d)))
                    }),
                });
                set(&mut m.components, id, v);
            }
            Obj::Conductor(id) => {
                let v = object(ctx, obj, &what, a.conductors.get(&id), t.conductors.get(&id), s.conductors.get(&id), |ctx, a, t, s| {
                    let mut c =
                        Conductor { properties: merge_props(ctx, obj, &what, &a.properties, &t.properties, &s.properties, &all), ends: t.ends };
                    // A connection conflict always offers Leave unconnected (design §5.3, owner
                    // answer 7): the end stays where the kept drawing has it, loose.
                    for (end, name, word) in [(End::A, "end.a", "end A"), (End::B, "end.b", "end B")] {
                        let show = |x: Option<TerminalId>| x.map_or_else(|| "unconnected".into(), |x| name_of(&docs, Obj::Terminal(x)));
                        let v = match pick(Some(&a.end(end)), Some(&t.end(end)), Some(&s.end(end))) {
                            Some(Some(v)) => v,
                            _ => {
                                let k = Conflict {
                                    id: format!("{}/{name}", obj.hex()),
                                    kind: ConflictKind::Value,
                                    object: obj,
                                    what: format!("{what} {word}"),
                                    target: show(t.end(end)),
                                    source: show(s.end(end)),
                                    choices: vec![Choice::Target, Choice::Source, Choice::Drop],
                                    chosen: None,
                                };
                                match ctx.ask(k) {
                                    Some(Choice::Source) => s.end(end),
                                    Some(Choice::Drop) => None,
                                    _ => t.end(end),
                                }
                            }
                        };
                        c.set_end(end, v);
                    }
                    c
                });
                set(&mut m.conductors, id, v);
            }
            Obj::Cable(id) => {
                let v = object(ctx, obj, &what, a.cables.get(&id), t.cables.get(&id), s.cables.get(&id), |ctx, a, t, s| Cable {
                    properties: merge_props(ctx, obj, &what, &a.properties, &t.properties, &s.properties, &all),
                    members: merge_members(&a.members, &t.members, &s.members),
                });
                set(&mut m.cables, id, v);
            }
            Obj::Page(id) => {
                let v = object(ctx, obj, &what, a.pages.get(&id), t.pages.get(&id), s.pages.get(&id), |ctx, a, t, s| Page {
                    title: field(ctx, obj, "title", format!("{what} · title"), &a.title, &t.title, &s.title, &|x| format!("\"{x}\"")),
                    properties: merge_props(ctx, obj, &what, &a.properties, &t.properties, &s.properties, &all),
                });
                set(&mut m.pages, id, v);
            }
            Obj::Folder(id) => {
                let v = object(ctx, obj, &what, a.folders.get(&id), t.folders.get(&id), s.folders.get(&id), |ctx, a, t, s| Folder {
                    name: field(ctx, obj, "name", format!("{what} · name"), &a.name, &t.name, &s.name, &|x| format!("\"{x}\"")),
                    parent: field(ctx, obj, "parent", format!("{what} · placed in"), &a.parent, &t.parent, &s.parent, &|x| {
                        x.map_or_else(|| "the top level".into(), |f| name_of(&docs, Obj::Folder(f)))
                    }),
                });
                set(&mut m.folders, id, v);
            }
            Obj::Representation(id) if let Some(v) = folded(ctx, Obj::Representation(id)) => {
                set(&mut m.representations, id, v.and_then(|d| d.representations.get(&id).cloned()));
            }
            Obj::Item(id) if let Some(v) = folded(ctx, Obj::Item(id)) => {
                set(&mut m.items, id, v.and_then(|d| d.items.get(&id).cloned()));
            }
            Obj::Representation(id) => {
                let v =
                    object(ctx, obj, &what, a.representations.get(&id), t.representations.get(&id), s.representations.get(&id), |ctx, a, t, s| {
                        let show = |r: &Representation| format!("on {}", name_of(&docs, Obj::Page(r.page)).to_lowercase());
                        field(ctx, obj, "placement", format!("{what} · placed differently on both"), a, t, s, &show)
                    });
                set(&mut m.representations, id, v);
            }
            Obj::Item(id) => {
                let v = object(ctx, obj, &what, a.items.get(&id), t.items.get(&id), s.items.get(&id), |ctx, a, t, s| {
                    field(ctx, obj, "item", format!("{what} · changed on both"), a, t, s, &|_: &Item| "its drawing".into())
                });
                set(&mut m.items, id, v);
            }
            Obj::Terminal(id) => {
                let v = object(ctx, obj, &what, a.terminals.get(&id), t.terminals.get(&id), s.terminals.get(&id), |ctx, a, t, s| {
                    let show = |x: &Terminal| x.label.clone().or_else(|| x.key.clone()).unwrap_or_else(|| "(no label)".into());
                    field(ctx, obj, "record", format!("{what} · changed on both"), a, t, s, &show)
                });
                set(&mut m.terminals, id, v);
            }
            Obj::Layer(id) => {
                let v = object(ctx, obj, &what, a.layers.get(&id), t.layers.get(&id), s.layers.get(&id), |ctx, a, t, s| {
                    field(ctx, obj, "layer", format!("{what} · changed on both"), a, t, s, &|l| format!("\"{}\"", l.name))
                });
                set(&mut m.layers, id, v);
            }
            Obj::Definition(id) => {
                let v = object(ctx, obj, &what, a.definitions.get(&id), t.definitions.get(&id), s.definitions.get(&id), |ctx, a, t, s| {
                    field(ctx, obj, "definition", format!("{what} · changed on both"), a, t, s, &|d| format!("\"{}\"", d.name))
                });
                set(&mut m.definitions, id, v);
            }
            Obj::Project => {
                m.properties = merge_props(ctx, obj, "Project", &a.properties, &t.properties, &s.properties, ctx.inp.keep_project_key);
                m.settings =
                    field(ctx, obj, "settings", "Project settings · changed on both".into(), &a.settings, &t.settings, &s.settings, &|_| {
                        "its settings".into()
                    });
            }
        }
    }
}

/// A wire's drawing (its symbol or route item) changed on both sides while one of its ends is a
/// connection conflict: it follows that conflict's answer instead of being a second conflict
/// (tester 020b, defect 2): the source's drawing for `branch`, else the target's. `None` when
/// it is not such a case; `Some(None)` when the chosen side has no such object.
fn folded<'a>(ctx: &Ctx<'a, '_>, obj: Obj) -> Option<Option<&'a Document>> {
    let (a, t, s) = (ctx.inp.ancestor, ctx.inp.target, ctx.inp.source);
    let conductor = |d: &Document| -> Option<ConductorId> {
        let rep = match obj {
            Obj::Representation(r) => d.representations.get(&r)?,
            Obj::Item(i) => d.representation_of_item(i)?.1,
            _ => return None,
        };
        match rep.subject {
            Subject::Conductor(c) => Some(c),
            _ => None,
        }
    };
    let changed_on_both = match obj {
        Obj::Representation(r) => {
            let (x, y, z) = (a.representations.get(&r), t.representations.get(&r), s.representations.get(&r));
            y.is_some() && z.is_some() && pick(x, y, z).is_none()
        }
        Obj::Item(i) => {
            let (x, y, z) = (a.items.get(&i), t.items.get(&i), s.items.get(&i));
            y.is_some() && z.is_some() && pick(x, y, z).is_none()
        }
        _ => false,
    };
    if !changed_on_both {
        return None;
    }
    let c = conductor(t).or_else(|| conductor(s))?;
    let (ca, ct, cs) = (a.conductors.get(&c), t.conductors.get(&c)?, s.conductors.get(&c)?);
    let end = End::BOTH.into_iter().find(|e| pick(Some(&ca.and_then(|x| x.end(*e))), Some(&ct.end(*e)), Some(&cs.end(*e))).is_none())?;
    let id = format!("{}/end.{}", c.hex(), if end == End::A { "a" } else { "b" });
    Some(match ctx.choices.get(&id) {
        Some(Choice::Source) => Some(s),
        _ => Some(t),
    })
}

/// Cable members: adds and removes from both sides combine (design §5.1); a member's core is
/// the side that changed it (the target's when both did).
fn merge_members(a: &[CableMember], t: &[CableMember], s: &[CableMember]) -> Vec<CableMember> {
    fn has(l: &[CableMember], c: ConductorId) -> Option<&CableMember> {
        l.iter().find(|m| m.conductor == c)
    }
    let mut out: Vec<CableMember> = Vec::new();
    for m in t {
        let (am, sm) = (has(a, m.conductor), has(s, m.conductor));
        if am.is_some() && sm.is_none() {
            continue; // removed by the source
        }
        let core = pick(am.map(|x| &x.core), Some(&m.core), sm.map(|x| &x.core)).flatten().unwrap_or_else(|| m.core.clone());
        out.push(CableMember { conductor: m.conductor, core });
    }
    for m in s {
        if has(t, m.conductor).is_none() && has(a, m.conductor).is_none() {
            out.push(m.clone()); // added by the source
        }
    }
    out
}

/// A three-way merge of an order (pages or folders) over the objects `keep` holds.
fn merge_order<T: Copy + Ord>(ctx: &mut Ctx<'_, '_>, id: &str, what: &str, a: &[T], t: &[T], s: &[T], keep: &dyn Fn(T) -> bool) -> Vec<T> {
    let (sa, st, ss): (BTreeSet<T>, BTreeSet<T>, BTreeSet<T>) =
        (a.iter().copied().collect(), t.iter().copied().collect(), s.iter().copied().collect());
    let common = |l: &[T]| -> Vec<T> { l.iter().copied().filter(|x| sa.contains(x) && st.contains(x) && ss.contains(x)).collect() };
    let (fa, ft, fs) = (common(a), common(t), common(s));
    let base: &[T] = if ft == fa {
        s
    } else if fs == fa || fs == ft {
        t
    } else {
        let c = Conflict {
            id: id.into(),
            kind: ConflictKind::Order,
            object: Obj::Project,
            what: what.into(),
            target: format!("as on {}", ctx.t()),
            source: format!("as on {}", ctx.s()),
            choices: vec![Choice::Target, Choice::Source],
            chosen: None,
        };
        match ctx.ask(c) {
            Some(Choice::Source) => s,
            _ => t,
        }
    };
    let mut out: Vec<T> = base.iter().copied().filter(|x| keep(*x)).collect();
    let mut placed: BTreeSet<T> = out.iter().copied().collect();
    for list in [t, s] {
        for (i, x) in list.iter().enumerate() {
            if placed.contains(x) || !keep(*x) {
                continue;
            }
            // After its nearest predecessor already placed, else first.
            let at = list.get(..i).unwrap_or_default().iter().rev().find_map(|p| out.iter().position(|q| q == p)).map_or(0, |p| p + 1);
            out.insert(at, *x);
            placed.insert(*x);
        }
    }
    out
}

/// What `obj` in `m` refers to, and how.
fn refs(m: &Document, obj: Obj) -> Vec<(Obj, Use)> {
    let mut out = Vec::new();
    match obj {
        Obj::Item(i) => {
            if let Some(it) = m.items.get(&i) {
                out.push((Obj::Page(it.page), Use::Whole));
                out.push((Obj::Layer(it.style.layer), Use::Whole));
                if let ItemKind::Symbol(sym) = &it.kind {
                    out.push((Obj::Definition(sym.definition), Use::Whole));
                }
            }
        }
        Obj::Component(c) => {
            if let Some(d) = m.components.get(&c).and_then(|c| c.definition) {
                out.push((Obj::Definition(d), Use::ComponentDefinition));
            }
        }
        Obj::Terminal(t) => {
            if let Some(t) = m.terminals.get(&t) {
                out.push((Obj::Component(t.component), Use::Whole));
            }
        }
        Obj::Representation(r) => {
            if let Some(r) = m.representations.get(&r) {
                out.push((
                    match r.subject {
                        Subject::Component(c) => Obj::Component(c),
                        Subject::Conductor(c) => Obj::Conductor(c),
                        Subject::Cable(c) => Obj::Cable(c),
                    },
                    Use::Whole,
                ));
                out.push((Obj::Page(r.page), Use::Whole));
                out.extend(r.items.iter().map(|i| (Obj::Item(*i), Use::RepItem)));
                out.extend(r.connection_points.keys().map(|t| (Obj::Terminal(*t), Use::ConnectionPoint)));
            }
        }
        Obj::Conductor(c) => {
            if let Some(c) = m.conductors.get(&c) {
                for e in End::BOTH {
                    if let Some(t) = c.end(e) {
                        out.push((Obj::Terminal(t), Use::End(e)));
                    }
                }
            }
        }
        Obj::Cable(c) => {
            if let Some(c) = m.cables.get(&c) {
                out.extend(c.members.iter().map(|x| (Obj::Conductor(x.conductor), Use::Member)));
            }
        }
        Obj::Page(p) => {
            if let Some(f) = crate::book::folder_of(m, p) {
                out.push((Obj::Folder(f), Use::PageFolder));
            }
        }
        Obj::Folder(f) => {
            if let Some(p) = m.folders.get(&f).and_then(|f| f.parent) {
                out.push((Obj::Folder(p), Use::FolderParent));
            }
        }
        Obj::Layer(_) | Obj::Definition(_) | Obj::Project => {}
    }
    out
}

/// Every object of `m` that refers to `y` (a scan: used only when a "remove what uses it"
/// choice removes an object whole).
fn users_of(m: &Document, y: Obj) -> Vec<Obj> {
    let mut all: Vec<Obj> = Vec::new();
    match y {
        Obj::Terminal(t) => {
            all.extend(m.conductors.iter().filter(|(_, c)| c.ends.contains(&Some(t))).map(|(k, _)| Obj::Conductor(*k)));
            all.extend(m.representations.iter().filter(|(_, r)| r.connection_points.contains_key(&t)).map(|(k, _)| Obj::Representation(*k)));
        }
        Obj::Component(c) => {
            all.extend(m.terminals.iter().filter(|(_, t)| t.component == c).map(|(k, _)| Obj::Terminal(*k)));
            all.extend(m.representations.iter().filter(|(_, r)| r.subject == Subject::Component(c)).map(|(k, _)| Obj::Representation(*k)));
        }
        Obj::Conductor(c) => {
            all.extend(m.cables.iter().filter(|(_, x)| x.members.iter().any(|mm| mm.conductor == c)).map(|(k, _)| Obj::Cable(*k)));
            all.extend(m.representations.iter().filter(|(_, r)| r.subject == Subject::Conductor(c)).map(|(k, _)| Obj::Representation(*k)));
        }
        Obj::Cable(c) => {
            all.extend(m.representations.iter().filter(|(_, r)| r.subject == Subject::Cable(c)).map(|(k, _)| Obj::Representation(*k)));
        }
        Obj::Representation(_) | Obj::Project => {}
        Obj::Item(i) => {
            all.extend(m.representations.iter().filter(|(_, r)| r.items.contains(&i)).map(|(k, _)| Obj::Representation(*k)));
        }
        Obj::Page(p) => {
            all.extend(m.items.iter().filter(|(_, x)| x.page == p).map(|(k, _)| Obj::Item(*k)));
            all.extend(m.representations.iter().filter(|(_, r)| r.page == p).map(|(k, _)| Obj::Representation(*k)));
        }
        Obj::Layer(l) => all.extend(m.items.iter().filter(|(_, x)| x.style.layer == l).map(|(k, _)| Obj::Item(*k))),
        Obj::Definition(d) => {
            all.extend(m.items.iter().filter(|(_, x)| matches!(&x.kind, ItemKind::Symbol(s) if s.definition == d)).map(|(k, _)| Obj::Item(*k)));
            all.extend(m.components.iter().filter(|(_, c)| c.definition == Some(d)).map(|(k, _)| Obj::Component(*k)));
        }
        Obj::Folder(f) => {
            all.extend(m.folders.iter().filter(|(_, x)| x.parent == Some(f)).map(|(k, _)| Obj::Folder(*k)));
            all.extend(m.pages.keys().filter(|p| crate::book::folder_of(m, **p) == Some(f)).map(|k| Obj::Page(*k)));
        }
    }
    all
}

/// Stop `x`'s use of `y` (it was not brought back): detach, drop the entry, or remove `x`.
fn drop_use(m: &mut Document, x: Obj, y: Obj, how: Use, check: &mut BTreeSet<Obj>) {
    match (x, how) {
        (Obj::Conductor(c), Use::End(e)) => {
            if let Some(c) = m.conductors.get_mut(&c) {
                c.set_end(e, None);
            }
        }
        (Obj::Representation(r), Use::ConnectionPoint) => {
            if let (Some(r), Obj::Terminal(t)) = (m.representations.get_mut(&r), y) {
                r.connection_points.remove(&t);
            }
        }
        (Obj::Cable(c), Use::Member) => {
            if let (Some(c), Obj::Conductor(w)) = (m.cables.get_mut(&c), y) {
                c.members.retain(|mm| mm.conductor != w);
            }
        }
        (Obj::Page(p), Use::PageFolder) => {
            if let Some(p) = m.pages.get_mut(&p) {
                p.properties.remove(crate::book::FOLDER_KEY);
            }
        }
        (Obj::Folder(f), Use::FolderParent) => {
            if let Some(f) = m.folders.get_mut(&f) {
                f.parent = None;
            }
        }
        (Obj::Component(c), Use::ComponentDefinition) => {
            if let Some(c) = m.components.get_mut(&c) {
                c.definition = None;
            }
        }
        (Obj::Representation(r), Use::RepItem) => {
            let symbol = m.representations.get(&r).is_some_and(|x| x.form == Form::Symbol);
            if symbol {
                remove_whole(m, x, check);
            } else if let (Some(rep), Obj::Item(i)) = (m.representations.get_mut(&r), y) {
                rep.items.retain(|k| *k != i);
            }
        }
        _ => remove_whole(m, x, check),
    }
}

/// Remove `x` from `m`; what used it is checked again.
fn remove_whole(m: &mut Document, x: Obj, check: &mut BTreeSet<Obj>) {
    let users = users_of(m, x);
    let empty = Document::default();
    copy(m, &empty, x);
    check.extend(users);
}

/// Give every reference kept in `m` a target: each missing object is a **needed** conflict
/// (bring it back, or leave unconnected / remove what uses it).
fn resolve_needed(ctx: &mut Ctx<'_, '_>, m: &mut Document, mut check: BTreeSet<Obj>) -> BTreeSet<Obj> {
    let (a, t, s) = (ctx.inp.ancestor, ctx.inp.target, ctx.inp.source);
    let mut changed: BTreeSet<Obj> = BTreeSet::new();
    // What was brought back here, and what could not be settled (contradicting answers).
    let mut brought: BTreeSet<Obj> = BTreeSet::new();
    let mut stuck: BTreeSet<Obj> = BTreeSet::new();
    for _ in 0..64 {
        let mut missing: BTreeMap<Obj, Vec<(Obj, Use)>> = BTreeMap::new();
        for x in &check {
            if !holds(m, *x) {
                continue;
            }
            for (y, how) in refs(m, *x) {
                if !holds(m, y) && !stuck.contains(&y) {
                    missing.entry(y).or_default().push((*x, how));
                }
            }
        }
        if missing.is_empty() {
            break;
        }
        changed.extend(check.iter().copied());
        let mut next: BTreeSet<Obj> = BTreeSet::new();
        for (y, users) in missing {
            // Whoever still has it: the source, else the target, else the ancestor.
            let from = [s, t, a].into_iter().find(|d| holds(d, y));
            let docs = [t, s, a];
            let user_names: Vec<String> = users.iter().take(3).map(|(x, _)| name_of(&docs, *x)).collect();
            let more = users.len().saturating_sub(3);
            let removed_on = if holds(t, y) { ctx.s().to_string() } else { ctx.t().to_string() };
            let only_ends = users.iter().all(|(_, u)| matches!(u, Use::End(_) | Use::ConnectionPoint));
            let mut choices = Vec::new();
            if from.is_some() {
                choices.push(Choice::BringBack);
            }
            choices.push(Choice::Drop);
            let c = Conflict {
                id: format!("{}/needed", y.hex()),
                kind: ConflictKind::Needed,
                object: y,
                what: format!(
                    "{} · removed on {removed_on}; still used by {}{}",
                    name_of(&docs, y),
                    user_names.join(", "),
                    if more > 0 { format!(" and {more} more") } else { String::new() }
                ),
                target: if only_ends { "leave unconnected".into() } else { "remove what uses it".into() },
                source: "bring it back".into(),
                choices,
                chosen: None,
            };
            let chosen = ctx.ask(c);
            // Bringing something back that needs `y` while `y`'s answer removes what uses it
            // contradicts itself (tester 020b, defect 3): say so, before Merge.
            if matches!(chosen, Some(Choice::Drop))
                && let Some((x, _)) = users.iter().find(|(x, _)| brought.contains(x))
            {
                ctx.problems.push(format!(
                    "Bringing back {} needs {}: choose Bring it back for {} too, or leave {} out, first.",
                    name_of(&docs, *x),
                    name_of(&docs, y),
                    name_of(&docs, y),
                    name_of(&docs, *x)
                ));
                stuck.insert(y);
                continue;
            }
            match (chosen, from) {
                (Some(Choice::Drop), _) | (_, None) => {
                    for (x, how) in users {
                        drop_use(m, x, y, how, &mut next);
                        next.insert(x);
                    }
                }
                // Brought back (also provisionally while unanswered, so what it needs shows).
                (_, Some(d)) => {
                    copy(m, d, y);
                    brought.insert(y);
                    next.insert(y);
                    changed.insert(y);
                }
            }
        }
        check = next;
    }
    changed.extend(check);
    changed
}

/// The next free tag with `prefix` (above `high` and every tag already given out here).
fn next_tag(prefix: &str, high: &mut HighWater, padding: u8) -> String {
    let n = high.get(prefix).copied().unwrap_or(0).saturating_add(1);
    high.insert(prefix.to_string(), n);
    tag::format(prefix, n, padding)
}

/// Two devices with one tag (018: one tag, one device) after the merge: a **tag** conflict per
/// tag, renumbering the source's device (the default suggestion, owner answer 12) or the
/// target's.
fn resolve_tags(ctx: &mut Ctx<'_, '_>, m: &mut Document, objects: &BTreeSet<Obj>) -> BTreeSet<Obj> {
    let (t, s) = (ctx.inp.target, ctx.inp.source);
    let mut high = ctx.inp.high.clone();
    for c in m.components.values() {
        if let Some(x) = c.properties.get(tag::TAG_KEY) {
            tag::note(&mut high, x);
        }
    }
    let padding = m.settings.tag_padding;
    let mut out = BTreeSet::new();
    let mut done: BTreeSet<String> = BTreeSet::new();
    let ours: Vec<ComponentId> = objects.iter().filter_map(|o| if let Obj::Component(c) = o { Some(*c) } else { None }).collect();
    for c in ours {
        let Some(tag_text) = m.components.get(&c).and_then(|x| x.properties.get(tag::TAG_KEY)).cloned() else { continue };
        // Only a tag the merge brought from the source.
        if t.components.get(&c).and_then(|x| x.properties.get(tag::TAG_KEY)) == Some(&tag_text) {
            continue;
        }
        let key = tag::key(&tag_text);
        if done.contains(&key) {
            continue;
        }
        let same = |d: &ComponentId| *d != c && m.components.get(d).and_then(|x| x.properties.get(tag::TAG_KEY)).is_some_and(|x| tag::key(x) == key);
        let mut others: Vec<ComponentId> = t.components_tagged(&tag_text).into_iter().filter(|d| same(d)).collect();
        let more: Vec<ComponentId> = objects
            .iter()
            .filter_map(|o| if let Obj::Component(d) = o { Some(*d) } else { None })
            .filter(|d| same(d) && !others.contains(d))
            .collect();
        others.extend(more);
        let Some(other) = others.first().copied() else { continue };
        done.insert(key.clone());
        let prefix = tag::parse(&tag_text).map(|p| p.0);
        let fresh = match &prefix {
            Some(p) => next_tag(p, &mut high, padding),
            None => format!("{tag_text}-2"),
        };
        let conflict = Conflict {
            id: format!("rule/tag/{key}"),
            kind: ConflictKind::Tag,
            object: Obj::Component(c),
            what: format!("{tag_text} · {} and {} each have a device {tag_text}", ctx.t(), ctx.s()),
            target: format!("renumber {}'s to {fresh}", ctx.t()),
            source: format!("renumber {}'s to {fresh}", ctx.s()),
            choices: vec![Choice::RenumberSource, Choice::RenumberTarget],
            chosen: None,
        };
        let which = match ctx.ask(conflict) {
            Some(Choice::RenumberSource) => Some(if s.components.contains_key(&c) { c } else { other }),
            Some(Choice::RenumberTarget) => Some(if s.components.contains_key(&c) { other } else { c }),
            _ => None,
        };
        if let Some(d) = which
            && let Some(x) = m.components.get_mut(&d)
        {
            x.properties.insert(tag::TAG_KEY.into(), fresh);
            out.insert(Obj::Component(d));
        }
    }
    out
}

/// The next free page number after `n`.
fn next_number(n: &str, taken: &BTreeSet<String>, step: u32) -> String {
    if let Ok(v) = n.trim().parse::<u64>() {
        let mut x = v;
        for _ in 0..10_000 {
            x = x.saturating_add(u64::from(step.max(1)));
            if !taken.contains(&x.to_string()) {
                return x.to_string();
            }
        }
    }
    (2..10_000).map(|i| format!("{n}.{i}")).find(|c| !taken.contains(c)).unwrap_or_else(|| format!("{n}.x"))
}

/// Two pages with one number after the merge (019): a **page number** conflict, renumbering
/// the source's page or the target's to the next free number with the project's step.
fn resolve_page_numbers(ctx: &mut Ctx<'_, '_>, m: &mut Document, objects: &BTreeSet<Obj>) -> BTreeSet<Obj> {
    let (t, s) = (ctx.inp.target, ctx.inp.source);
    let mut out = BTreeSet::new();
    let mut taken: BTreeSet<String> = m.pages.keys().filter_map(|p| crate::book::stored_number(m, *p)).map(str::to_string).collect();
    let step = m.settings.page_number_step;
    let ours: Vec<PageId> = objects.iter().filter_map(|o| if let Obj::Page(p) = o { Some(*p) } else { None }).collect();
    for p in ours {
        let Some(num) = crate::book::stored_number(m, p).map(str::to_string) else { continue };
        if crate::book::stored_number(t, p) == Some(num.as_str()) {
            continue;
        }
        let Some(other) = m.pages.keys().copied().find(|q| *q != p && crate::book::stored_number(m, *q) == Some(num.as_str())) else { continue };
        let fresh = next_number(&num, &taken, step);
        let c = Conflict {
            id: format!("rule/page/{num}"),
            kind: ConflictKind::PageNumber,
            object: Obj::Page(p),
            what: format!("Page {num} · {} and {} each have a page {num}", ctx.t(), ctx.s()),
            target: format!("renumber {}'s to {fresh}", ctx.t()),
            source: format!("renumber {}'s to {fresh}", ctx.s()),
            choices: vec![Choice::RenumberSource, Choice::RenumberTarget],
            chosen: None,
        };
        let src_is_p = s.pages.contains_key(&p) && !t.pages.contains_key(&p) || crate::book::stored_number(s, p) == Some(num.as_str());
        let which = match ctx.ask(c) {
            Some(Choice::RenumberSource) => Some(if src_is_p { p } else { other }),
            Some(Choice::RenumberTarget) => Some(if src_is_p { other } else { p }),
            _ => None,
        };
        if let Some(q) = which
            && let Some(page) = m.pages.get_mut(&q)
        {
            page.properties.insert(crate::book::NUMBER_KEY.into(), fresh.clone());
            taken.insert(fresh);
            out.insert(Obj::Page(q));
        }
    }
    out
}

/// Two sibling folders with one name after the merge: a **folder name** conflict, renaming the
/// source's (`Spare (2)`) or the target's.
fn resolve_folder_names(ctx: &mut Ctx<'_, '_>, m: &mut Document, objects: &BTreeSet<Obj>) -> BTreeSet<Obj> {
    let (t, s) = (ctx.inp.target, ctx.inp.source);
    let mut out = BTreeSet::new();
    let ours: Vec<FolderId> = objects.iter().filter_map(|o| if let Obj::Folder(f) = o { Some(*f) } else { None }).collect();
    for f in ours {
        let Some(me) = m.folders.get(&f).cloned() else { continue };
        if t.folders.get(&f) == Some(&me) {
            continue;
        }
        let lower = me.name.to_lowercase();
        let Some(other) = m.folders.iter().find(|(k, x)| **k != f && x.parent == me.parent && x.name.to_lowercase() == lower).map(|(k, _)| *k) else {
            continue;
        };
        let fresh = (2..1_000)
            .map(|i| format!("{} ({i})", me.name))
            .find(|n| !m.folders.values().any(|x| x.parent == me.parent && x.name.to_lowercase() == n.to_lowercase()))
            .unwrap_or_else(|| format!("{} (new)", me.name));
        let c = Conflict {
            id: format!("rule/folder/{}", f.hex()),
            kind: ConflictKind::FolderName,
            object: Obj::Folder(f),
            what: format!("Folder \"{}\" · added on both", me.name),
            target: format!("rename {}'s to \"{fresh}\"", ctx.t()),
            source: format!("rename {}'s to \"{fresh}\"", ctx.s()),
            choices: vec![Choice::RenumberSource, Choice::RenumberTarget],
            chosen: None,
        };
        let src = s.folders.contains_key(&f) && !t.folders.contains_key(&f);
        let which = match ctx.ask(c) {
            Some(Choice::RenumberSource) => Some(if src { f } else { other }),
            Some(Choice::RenumberTarget) => Some(if src { other } else { f }),
            _ => None,
        };
        if let Some(q) = which
            && let Some(x) = m.folders.get_mut(&q)
        {
            x.name = fresh;
            out.insert(Obj::Folder(q));
        }
    }
    out
}

/// The source's changes since the ancestor, for the preview.
fn changes(inp: &Inputs<'_>, objects: &[Obj]) -> (Vec<Change>, Vec<PageId>) {
    let (a, s) = (inp.ancestor, inp.source);
    let docs = [s, a, inp.target];
    let mut out = Vec::new();
    let mut seen: BTreeSet<Obj> = BTreeSet::new();
    for &obj in objects {
        if !differs(a, s, obj) {
            continue;
        }
        // A drawing item of a symbol is shown as that symbol (moving a symbol changes its item).
        let shown_as = match obj {
            Obj::Item(i) => s.representation_of_item(i).or_else(|| a.representation_of_item(i)).map_or(obj, |(r, _)| Obj::Representation(r)),
            _ => obj,
        };
        if seen.contains(&shown_as) {
            continue;
        }
        // A symbol of a wire, cable or device listed itself is shown through it.
        if let Obj::Representation(r) = shown_as
            && let Some(rep) = s.representations.get(&r).or_else(|| a.representations.get(&r))
        {
            let subject = match rep.subject {
                Subject::Component(c) => Obj::Component(c),
                Subject::Conductor(c) => Obj::Conductor(c),
                Subject::Cable(c) => Obj::Cable(c),
            };
            if objects.contains(&subject) && differs(a, s, subject) && holds(a, subject) == holds(a, shown_as) {
                continue;
            }
        }
        seen.insert(shown_as);
        let (sign, detail) = match (holds(a, obj), holds(s, obj)) {
            (false, true) => ('+', "added".to_string()),
            (true, false) => ('−', "removed".to_string()),
            _ => ('~', change_detail(a, s, obj)),
        };
        if obj == Obj::Project && detail.is_empty() {
            continue;
        }
        let (sign, detail) = if shown_as == obj { (sign, detail) } else { ('~', "moved or changed".to_string()) };
        let page = page_in(s, shown_as).or_else(|| page_in(a, shown_as));
        out.push(Change { sign, object: shown_as, text: format!("{} {detail}", name_of(&docs, shown_as)).trim().to_string(), page });
    }
    let order: BTreeMap<PageId, usize> = s.page_order.iter().enumerate().map(|(i, p)| (*p, i)).collect();
    out.sort_by(|x, y| {
        let (px, py) = (x.page.and_then(|p| order.get(&p)), y.page.and_then(|p| order.get(&p)));
        (px.is_none(), px, &x.text).cmp(&(py.is_none(), py, &y.text))
    });
    let mut pages: Vec<PageId> = Vec::new();
    for c in &out {
        if let Some(p) = c.page
            && !pages.contains(&p)
        {
            pages.push(p);
        }
    }
    (out, pages)
}

/// `· Description "Pump" → "Pump 1"` for the first changed property (and how many more).
fn change_detail(a: &Document, s: &Document, obj: Obj) -> String {
    let props = |d: &Document| -> Option<Properties> {
        match obj {
            Obj::Component(c) => d.components.get(&c).map(|x| x.properties.clone()),
            Obj::Conductor(c) => d.conductors.get(&c).map(|x| x.properties.clone()),
            Obj::Cable(c) => d.cables.get(&c).map(|x| x.properties.clone()),
            Obj::Page(p) => d.pages.get(&p).map(|x| x.properties.clone()),
            Obj::Project => Some(d.properties.clone()),
            _ => None,
        }
    };
    if let Obj::Conductor(c) = obj
        && let (Some(x), Some(y)) = (a.conductors.get(&c), s.conductors.get(&c))
    {
        let docs = [s, a];
        for (e, word) in [(End::A, "end A"), (End::B, "end B")] {
            if x.end(e) != y.end(e) {
                let show = |t: Option<TerminalId>| t.map_or_else(|| "unconnected".into(), |t| name_of(&docs, Obj::Terminal(t)));
                return format!("· {word} {} → {}", show(x.end(e)), show(y.end(e)));
            }
        }
    }
    match (props(a), props(s)) {
        (Some(pa), Some(ps)) => {
            let keys: Vec<&String> = pa.keys().chain(ps.keys()).collect::<BTreeSet<_>>().into_iter().filter(|k| pa.get(*k) != ps.get(*k)).collect();
            match keys.first() {
                Some(k) => {
                    let q = |v: Option<&String>| v.map_or_else(|| "(none)".into(), |v| format!("\"{v}\""));
                    let more = if keys.len() > 1 { format!(" (+{} more)", keys.len() - 1) } else { String::new() };
                    format!("· {} {} → {}{more}", key_words(k), q(pa.get(*k)), q(ps.get(*k)))
                }
                None if obj == Obj::Project => {
                    if a.settings == s.settings {
                        String::new()
                    } else {
                        "· settings changed".into()
                    }
                }
                None => "changed".into(),
            }
        }
        _ => "changed".into(),
    }
}

/// Work out the merge of `inp` with `choices` (module docs).
pub fn plan(inp: &Inputs<'_>, choices: &BTreeMap<String, Choice>) -> Plan {
    let mut ctx = Ctx { inp, choices, conflicts: Vec::new(), seen: BTreeSet::new(), problems: Vec::new() };
    let objects = scope_objects(inp.scope);
    let (changes, pages) = changes(inp, &objects);
    let mut m = inp.target.clone();
    merge_objects(&mut ctx, &mut m, &objects);
    // Orders.
    let (a, t, s) = (inp.ancestor, inp.target, inp.source);
    if inp.scope.touched.page_order || a.page_order != s.page_order || a.page_order != t.page_order {
        let pages_in = m.pages.clone();
        m.page_order = merge_order(&mut ctx, "rule/pageorder", "Page order · changed on both", &a.page_order, &t.page_order, &s.page_order, &|p| {
            pages_in.contains_key(&p)
        });
    }
    if inp.scope.touched.folder_order || a.folder_order != s.folder_order || a.folder_order != t.folder_order {
        let folders_in = m.folders.clone();
        m.folder_order =
            merge_order(&mut ctx, "rule/folderorder", "Folder order · changed on both", &a.folder_order, &t.folder_order, &s.folder_order, &|f| {
                folders_in.contains_key(&f)
            });
    }
    // Rules.
    let mut all: BTreeSet<Obj> = objects.iter().copied().collect();
    let needed = resolve_needed(&mut ctx, &mut m, all.clone());
    all.extend(needed);
    let renamed = resolve_tags(&mut ctx, &mut m, &all);
    all.extend(renamed);
    let renumbered = resolve_page_numbers(&mut ctx, &mut m, &all);
    all.extend(renumbered);
    let folders = resolve_folder_names(&mut ctx, &mut m, &all);
    all.extend(folders);
    // Page order lists every page exactly once.
    m.page_order.retain(|p| m.pages.contains_key(p));
    for p in m.pages.keys() {
        if !m.page_order.contains(p) {
            m.page_order.push(*p);
        }
    }
    m.folder_order.retain(|f| m.folders.contains_key(f));
    for f in m.folders.keys() {
        if !m.folder_order.contains(f) {
            m.folder_order.push(*f);
        }
    }
    let unanswered: Vec<String> = ctx.conflicts.iter().filter(|c| c.chosen.is_none()).map(|c| c.id.clone()).collect();
    // The same check the merge's transaction gets, on what it touches: every reference kept
    // has its object (a preview that says yes never ends in a refused Merge).
    if unanswered.is_empty() && ctx.problems.is_empty() {
        let docs = [inp.target, inp.source, inp.ancestor];
        'objects: for x in &all {
            if !holds(&m, *x) {
                continue;
            }
            for (y, _) in refs(&m, *x) {
                if !holds(&m, y) {
                    ctx.problems.push(format!(
                        "{} would refer to {}, which the merge leaves out: choose differently for one of them first.",
                        name_of(&docs, *x),
                        name_of(&docs, y)
                    ));
                    break 'objects;
                }
            }
        }
    }
    let problem = ctx.problems.first().cloned();
    let ops = if unanswered.is_empty() && problem.is_none() {
        let mut scope = Touched::default();
        for o in &all {
            match *o {
                Obj::Page(i) => {
                    scope.pages.insert(i);
                }
                Obj::Folder(i) => {
                    scope.folders.insert(i);
                }
                Obj::Layer(i) => {
                    scope.layers.insert(i);
                }
                Obj::Definition(i) => {
                    scope.definitions.insert(i);
                }
                Obj::Component(i) => {
                    scope.components.insert(i);
                }
                Obj::Terminal(i) => {
                    scope.terminals.insert(i);
                }
                Obj::Representation(i) => {
                    scope.representations.insert(i);
                }
                Obj::Item(i) => {
                    scope.items.insert(i);
                }
                Obj::Conductor(i) => {
                    scope.conductors.insert(i);
                }
                Obj::Cable(i) => {
                    scope.cables.insert(i);
                }
                Obj::Project => {}
            }
        }
        scope.page_order = true;
        scope.folder_order = true;
        diff::ops_between_in(inp.target, &m, inp.keep_project_key, Some(&scope), inp.scope.project || all.contains(&Obj::Project))
    } else {
        Vec::new()
    };
    Plan { changes, pages, conflicts: ctx.conflicts, unanswered, ops, problem }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn doc_with(tags: &[(u128, &str)]) -> Document {
        let mut d = Document::default();
        for (id, t) in tags {
            let mut c = Component::default();
            c.properties.insert(tag::TAG_KEY.into(), (*t).into());
            d.components.insert(ComponentId(*id), c);
        }
        d.rebuild_index();
        d
    }

    fn run(a: &Document, t: &Document, s: &Document, scope: &Scope, choices: &BTreeMap<String, Choice>) -> Plan {
        let all = |_: &str| true;
        let high = HighWater::new();
        let inp = Inputs { ancestor: a, target: t, source: s, scope, target_name: "main", source_name: "\"B\"", keep_project_key: &all, high: &high };
        plan(&inp, choices)
    }

    fn scope_of(ids: &[u128]) -> Scope {
        let mut s = Scope::default();
        for i in ids {
            s.touched.components.insert(ComponentId(*i));
        }
        s
    }

    #[test]
    fn one_sided_changes_merge_and_two_sided_ones_conflict() {
        let a = doc_with(&[(1, "-K01"), (2, "-K02")]);
        let mut t = a.clone();
        t.components.get_mut(&ComponentId(1)).unwrap().properties.insert("rating".into(), "16 A".into());
        let mut s = a.clone();
        s.components.get_mut(&ComponentId(1)).unwrap().properties.insert("rating".into(), "20 A".into());
        s.components.get_mut(&ComponentId(2)).unwrap().properties.insert("description".into(), "Pump".into());
        let scope = scope_of(&[1, 2]);
        let p = run(&a, &t, &s, &scope, &BTreeMap::new());
        assert_eq!(p.conflicts.len(), 1);
        let id = p.conflicts[0].id.clone();
        assert!(id.ends_with("/prop.rating"), "{id}");
        assert!(p.ops.is_empty());
        assert_eq!(p.unanswered, vec![id.clone()]);
        let mut ch = BTreeMap::new();
        ch.insert(id.clone(), Choice::Source);
        let p = run(&a, &t, &s, &scope, &ch);
        assert!(p.unanswered.is_empty());
        let mut doc = t.clone();
        for op in &p.ops {
            op.apply(&mut doc).unwrap();
        }
        assert_eq!(doc.components[&ComponentId(1)].properties["rating"], "20 A");
        assert_eq!(doc.components[&ComponentId(2)].properties["description"], "Pump");
        ch.insert(id, Choice::Value("25 A".into()));
        let p = run(&a, &t, &s, &scope, &ch);
        let mut doc = t.clone();
        for op in &p.ops {
            op.apply(&mut doc).unwrap();
        }
        assert_eq!(doc.components[&ComponentId(1)].properties["rating"], "25 A");
    }

    #[test]
    fn a_tag_added_on_both_sides_is_renumbered() {
        let a = doc_with(&[]);
        let t = doc_with(&[(1, "-K05")]);
        let s = doc_with(&[(2, "-K05")]);
        let scope = scope_of(&[1, 2]);
        let p = run(&a, &t, &s, &scope, &BTreeMap::new());
        let c = p.conflicts.iter().find(|c| c.kind == ConflictKind::Tag).expect("a tag conflict");
        assert_eq!(c.id, "rule/tag/K5");
        let mut ch = BTreeMap::new();
        ch.insert(c.id.clone(), Choice::RenumberSource);
        let p = run(&a, &t, &s, &scope, &ch);
        assert!(p.unanswered.is_empty(), "{:?}", p.unanswered);
        let mut doc = t.clone();
        for op in &p.ops {
            op.apply(&mut doc).unwrap();
        }
        assert_eq!(doc.components[&ComponentId(1)].properties["tag"], "-K05");
        assert_eq!(doc.components[&ComponentId(2)].properties["tag"], "-K06");
    }

    #[test]
    fn removed_on_one_side_and_changed_on_the_other_offers_both() {
        let a = doc_with(&[(1, "-K01")]);
        let t = doc_with(&[]);
        let mut s = a.clone();
        s.components.get_mut(&ComponentId(1)).unwrap().properties.insert("description".into(), "x".into());
        let scope = scope_of(&[1]);
        let p = run(&a, &t, &s, &scope, &BTreeMap::new());
        assert_eq!(p.conflicts[0].kind, ConflictKind::Removed);
        let mut ch = BTreeMap::new();
        ch.insert(p.conflicts[0].id.clone(), Choice::Source);
        let p = run(&a, &t, &s, &scope, &ch);
        let mut doc = t.clone();
        for op in &p.ops {
            op.apply(&mut doc).unwrap();
        }
        assert!(doc.components.contains_key(&ComponentId(1)));
    }
}
