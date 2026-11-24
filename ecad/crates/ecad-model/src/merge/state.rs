//! What a merge works on: the inputs, the scope, conflicts and choices, the plan, and the
//! per-object helpers (names, pages, copying) the other parts share.

use std::collections::{BTreeMap, BTreeSet};

use crate::document::invariant::Touched;
use crate::document::op::{ObjectRef, Op};
use crate::naming::id::{CableId, ConnectionPointId, DefinitionId, DeviceId, FolderId, ItemId, LayerId, PageId, RepresentationId, RouteId, WireId};
use crate::naming::tag::HighWater;
use crate::{Document, End, Subject};

/// An object a merge compares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Obj {
    Page(PageId),
    Folder(FolderId),
    Layer(LayerId),
    Definition(DefinitionId),
    Device(DeviceId),
    ConnectionPoint(ConnectionPointId),
    Representation(RepresentationId),
    Item(ItemId),
    Wire(WireId),
    Cable(CableId),
    /// A route.
    Route(RouteId),
    /// A drawing aid.
    Aid(crate::naming::id::AidId),
    /// A loose part.
    LoosePart(crate::naming::id::LoosePartId),
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
            Obj::Device(i) => i.hex(),
            Obj::ConnectionPoint(i) => i.hex(),
            Obj::Representation(i) => i.hex(),
            Obj::Item(i) => i.hex(),
            Obj::Wire(i) => i.hex(),
            Obj::Cable(i) => i.hex(),
            Obj::Route(i) => i.hex(),
            Obj::Aid(i) => i.hex(),
            Obj::LoosePart(i) => i.hex(),
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
            Obj::Device(_) => "device",
            Obj::ConnectionPoint(_) => "connection point",
            Obj::Representation(_) => "symbol",
            Obj::Item(_) => "drawing item",
            Obj::Wire(_) => "wire",
            Obj::Cable(_) => "cable",
            Obj::Route(_) => "route",
            Obj::Aid(_) => "drawing aid",
            Obj::LoosePart(_) => "loose part",
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
            if matches!(
                op,
                Op::SetProperty { object: ObjectRef::Project | ObjectRef::Standard, .. }
                    | Op::SetProjectSettings { .. }
                    | Op::SetStyle { .. }
                    | Op::SetStandard { .. }
                    | Op::ModifyProject { .. }
                    | Op::AddDeviceLink { .. }
                    | Op::RemoveDeviceLink { .. }
            ) {
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
            + t.devices.len()
            + t.connection_points.len()
            + t.representations.len()
            + t.wires.len()
            + t.cables.len()
            + t.routes.len()
            + t.aids.len()
            + t.loose_parts.len()
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
    /// Make the two devices (or cables) with one tag one: the source's joins the
    /// target's.
    MakeOne,
}

impl Choice {
    /// Whether `self` is the same choice as `other` (a typed value matches any typed value).
    pub(super) fn same_kind(&self, other: &Choice) -> bool {
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

/// One conflict and its choices.
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
    pub(super) fn allows(&self, c: &Choice) -> bool {
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
    /// Answers given for conflicts the merge does not raise (any more), in words: they have no
    /// effect.
    pub ignored: Vec<String>,
    /// Why Make them one is not offered, per tag conflict ID.
    pub unavailable: BTreeMap<String, String>,
    /// Provisional tags of the source given the next provisional number.
    pub provisional_renumbered: usize,
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
    /// Checks a value typed in an Edit field as an ordinary edit of that object's property would:
    /// the value as stored, or why it is refused.
    pub edit_rule: &'a dyn Fn(Obj, &str, &str) -> Result<String, RuleError>,
    /// The ops making device `from` part of device `into` in a state (the device merge), or why
    /// they cannot be one (Make them one). The state's index is current.
    pub join: &'a dyn Fn(&Document, DeviceId, DeviceId) -> Result<Vec<Op>, RuleError>,
}

/// Why a rule the caller supplies ([`Inputs::edit_rule`], [`Inputs::join`]) refuses: the caller's own
/// error (the electrical rules' refusal), kept as an error; the plan shows its words.
pub type RuleError = Box<dyn std::error::Error + Send + Sync>;

/// Why two objects with the same tag cannot be made one, when the merge itself knows.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum JoinError {
    /// The tag (`-W1`) is a cable on one side and a device on the other.
    #[error("{0} is a cable on one side and a device on the other.")]
    CableAndDevice(String),
}

/// How a kept object uses another one (what "leave it unconnected / remove what uses it" does).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Use {
    End(End),
    /// A route's end (0 or 1) lands on it.
    RouteEnd(usize),
    ConnectionPoint,
    Member,
    PageFolder,
    FolderParent,
    RepItem,
    DeviceDefinition,
    /// The object cannot exist without it: removed with it.
    Whole,
}

pub(super) struct Ctx<'a, 'b> {
    pub(super) inp: &'a Inputs<'b>,
    pub(super) choices: &'a BTreeMap<String, Choice>,
    pub(super) conflicts: Vec<Conflict>,
    pub(super) seen: BTreeSet<String>,
    pub(super) problems: Vec<String>,
    pub(super) unavailable: BTreeMap<String, String>,
    pub(super) provisional: usize,
}

impl Ctx<'_, '_> {
    /// Record `c` (once per ID) and return its valid choice, if any.
    pub(super) fn ask(&mut self, mut c: Conflict) -> Option<Choice> {
        let chosen = self.choices.get(&c.id).filter(|ch| c.allows(ch)).cloned();
        if self.seen.insert(c.id.clone()) {
            c.chosen = chosen.clone();
            self.conflicts.push(c);
        }
        chosen
    }

    pub(super) fn t(&self) -> &str {
        self.inp.target_name
    }

    pub(super) fn s(&self) -> &str {
        self.inp.source_name
    }
}

/// The three-way pick: `Some(value)` when it resolves, `None` for a conflict.
pub(super) fn pick<T: PartialEq + Clone>(a: Option<&T>, t: Option<&T>, s: Option<&T>) -> Option<Option<T>> {
    if t == s || a == s {
        Some(t.cloned())
    } else if a == t {
        Some(s.cloned())
    } else {
        None
    }
}

/// A property key in words (`rating` → `Rating`, `wire_number` → `Wire number`).
pub(super) fn key_words(k: &str) -> String {
    let t = k.replace('_', " ");
    let mut c = t.chars();
    c.next().map_or_else(String::new, |f| f.to_uppercase().collect::<String>() + c.as_str())
}

pub(super) fn shown(v: Option<&String>) -> String {
    v.map_or_else(|| "(none)".to_string(), Clone::clone)
}

/// A value as a conflict shows it; unset is `(none)`.
pub(super) fn shown_value<T: std::fmt::Display>(x: Option<&T>) -> String {
    x.map_or_else(|| "(none)".to_string(), |v| format!("\"{v}\""))
}

/// A property value as a person reads it: a value that is a
/// component's ID (32 hex digits, a component of one of `docs`) is shown as that component's tag,
/// never as the ID; any other value as stored.
pub fn value_words(docs: &[&Document], v: &str) -> String {
    let t = v.trim();
    if t.len() == 32
        && let Some(c) = DeviceId::parse(t)
        && docs.iter().any(|d| d.devices.contains_key(&c))
    {
        return name_of(docs, Obj::Device(c));
    }
    v.to_string()
}

/// The object that owns a connection point.
pub(super) fn owner_obj(owner: crate::ConnectionPointOwner) -> Obj {
    match owner {
        crate::ConnectionPointOwner::Device(c) => Obj::Device(c),
        crate::ConnectionPointOwner::Aid(a) => Obj::Aid(a),
    }
}

/// The name of `obj` for texts, from whichever state holds it.
pub fn name_of(docs: &[&Document], obj: Obj) -> String {
    let tag_of = |c: DeviceId| docs.iter().find_map(|d| d.devices.get(&c)).and_then(|c| c.tag().map(str::to_string)).unwrap_or_else(|| c.short());
    let page_of = |p: PageId| {
        docs.iter()
            .find(|d| d.pages.contains_key(&p))
            .map_or_else(|| p.short(), |d| format!("Page {}", crate::document::book::stored_number(d, p).map_or_else(|| p.short(), str::to_string)))
    };
    match obj {
        Obj::Device(c) => tag_of(c),
        Obj::ConnectionPoint(t) => match docs.iter().find_map(|d| d.connection_points.get(&t)) {
            Some(term) => format!(
                "{}:{}",
                match term.owner {
                    crate::ConnectionPointOwner::Device(c) => tag_of(c),
                    crate::ConnectionPointOwner::Aid(a) => a.short(),
                },
                term.label.clone().or_else(|| term.key.as_ref().map(ToString::to_string)).unwrap_or_else(|| t.short())
            ),
            None => t.short(),
        },
        Obj::Wire(c) => match docs.iter().find_map(|d| d.wires.get(&c)).and_then(|w| w.number_text().map(str::to_string)) {
            Some(n) => format!("Wire {n}"),
            None => format!("Wire {}", c.short()),
        },
        Obj::Cable(c) => match docs.iter().find_map(|d| d.cables.get(&c)).and_then(|w| w.tag().map(str::to_string)) {
            Some(n) => format!("Cable {n}"),
            None => format!("Cable {}", c.short()),
        },
        Obj::Route(r) => match docs.iter().find_map(|d| d.routes.get(&r)) {
            Some(x) => format!("Line on {}", page_of(x.page).to_lowercase()),
            None => format!("Line {}", r.short()),
        },
        Obj::Aid(x) => match docs.iter().find_map(|d| d.aids.get(&x)) {
            Some(crate::objects::aid::Aid::Continuation { name: Some(n), .. }) => format!("Continuation {n}"),
            Some(crate::objects::aid::Aid::Continuation { name: None, .. }) => format!("Continuation {}", x.short()),
            Some(crate::objects::aid::Aid::Box(_)) | None => format!("Box {}", x.short()),
        },
        Obj::LoosePart(x) => match docs.iter().find_map(|d| d.loose_parts.get(&x)) {
            Some(p) => format!("Loose part \"{}\"", p.description),
            None => format!("Loose part {}", x.short()),
        },
        Obj::Page(p) => match docs.iter().find_map(|d| d.pages.get(&p)) {
            Some(page) => format!("{} \"{}\"", page_of(p), page.title),
            None => page_of(p),
        },
        Obj::Folder(f) => format!("Folder \"{}\"", docs.iter().find_map(|d| d.folders.get(&f)).map_or_else(|| f.short(), |f| f.name.clone())),
        Obj::Layer(l) => format!("Layer \"{}\"", docs.iter().find_map(|d| d.layers.get(&l)).map_or_else(|| l.short(), |l| l.name.clone())),
        Obj::Definition(d) => format!("Symbol \"{}\"", docs.iter().find_map(|x| x.definitions.get(&d)).map_or_else(|| d.short(), |d| d.name.clone())),
        Obj::Representation(r) => match docs.iter().find_map(|d| d.representations.get(&r)) {
            Some(rep) => {
                let subject = match rep.subject {
                    Subject::Device(c) => tag_of(c),
                    Subject::Wire(c) => name_of(docs, Obj::Wire(c)),
                    Subject::Cable(c) => name_of(docs, Obj::Cable(c)),
                    Subject::Route(c) => name_of(docs, Obj::Route(c)),
                    Subject::Aid(c) => name_of(docs, Obj::Aid(c)),
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
pub(super) fn page_in(doc: &Document, obj: Obj) -> Option<PageId> {
    match obj {
        Obj::Page(p) => Some(p),
        Obj::Item(i) => doc.items.get(&i).map(|i| i.page),
        Obj::Representation(r) => doc.representations.get(&r).map(|r| r.page),
        Obj::Device(c) => doc.representations_of(Subject::Device(c)).map(|(_, r)| r.page).next(),
        Obj::Wire(c) => doc.representations_of(Subject::Wire(c)).map(|(_, r)| r.page).next(),
        Obj::Cable(c) => doc.representations_of(Subject::Cable(c)).map(|(_, r)| r.page).next(),
        Obj::Route(r) => doc.routes.get(&r).map(|r| r.page),
        Obj::Aid(x) => doc.representations_of(Subject::Aid(x)).map(|(_, r)| r.page).next(),
        Obj::ConnectionPoint(t) => doc.connection_points.get(&t).and_then(|t| page_in(doc, owner_obj(t.owner))),
        _ => None,
    }
}

/// Every object of the scope, in a fixed order.
pub(super) fn scope_objects(scope: &Scope) -> Vec<Obj> {
    let t = &scope.touched;
    let mut out: Vec<Obj> = Vec::new();
    let pages: BTreeSet<PageId> = t.pages.iter().chain(&t.page_properties).copied().collect();
    out.extend(pages.into_iter().map(Obj::Page));
    out.extend(t.folders.iter().map(|x| Obj::Folder(*x)));
    out.extend(t.layers.iter().map(|x| Obj::Layer(*x)));
    out.extend(t.definitions.iter().map(|x| Obj::Definition(*x)));
    out.extend(t.devices.iter().map(|x| Obj::Device(*x)));
    out.extend(t.connection_points.iter().map(|x| Obj::ConnectionPoint(*x)));
    out.extend(t.representations.iter().map(|x| Obj::Representation(*x)));
    out.extend(t.items.iter().map(|x| Obj::Item(*x)));
    out.extend(t.wires.iter().map(|x| Obj::Wire(*x)));
    out.extend(t.cables.iter().map(|x| Obj::Cable(*x)));
    out.extend(t.routes.iter().map(|x| Obj::Route(*x)));
    out.extend(t.aids.iter().map(|x| Obj::Aid(*x)));
    out.extend(t.loose_parts.iter().map(|x| Obj::LoosePart(*x)));
    if scope.project {
        out.push(Obj::Project);
    }
    out
}

/// Whether `obj` differs between `a` and `b`.
pub(super) fn differs(a: &Document, b: &Document, obj: Obj) -> bool {
    match obj {
        Obj::Page(i) => a.pages.get(&i) != b.pages.get(&i),
        Obj::Folder(i) => a.folders.get(&i) != b.folders.get(&i),
        Obj::Layer(i) => a.layers.get(&i) != b.layers.get(&i),
        Obj::Definition(i) => a.definitions.get(&i) != b.definitions.get(&i),
        Obj::Device(i) => a.devices.get(&i) != b.devices.get(&i),
        Obj::ConnectionPoint(i) => a.connection_points.get(&i) != b.connection_points.get(&i),
        Obj::Representation(i) => a.representations.get(&i) != b.representations.get(&i),
        Obj::Item(i) => a.items.get(&i) != b.items.get(&i),
        Obj::Wire(i) => a.wires.get(&i) != b.wires.get(&i),
        Obj::Cable(i) => a.cables.get(&i) != b.cables.get(&i),
        Obj::Route(i) => a.routes.get(&i) != b.routes.get(&i),
        Obj::Aid(i) => a.aids.get(&i) != b.aids.get(&i),
        Obj::LoosePart(i) => a.loose_parts.get(&i) != b.loose_parts.get(&i),
        Obj::Project => {
            a.display != b.display || a.project != b.project || a.settings != b.settings || a.standard != b.standard || a.styles != b.styles
        }
    }
}

/// Whether `doc` holds `obj`.
pub(super) fn holds(doc: &Document, obj: Obj) -> bool {
    match obj {
        Obj::Page(i) => doc.pages.contains_key(&i),
        Obj::Folder(i) => doc.folders.contains_key(&i),
        Obj::Layer(i) => doc.layers.contains_key(&i),
        Obj::Definition(i) => doc.definitions.contains_key(&i),
        Obj::Device(i) => doc.devices.contains_key(&i),
        Obj::ConnectionPoint(i) => doc.connection_points.contains_key(&i),
        Obj::Representation(i) => doc.representations.contains_key(&i),
        Obj::Item(i) => doc.items.contains_key(&i),
        Obj::Wire(i) => doc.wires.contains_key(&i),
        Obj::Cable(i) => doc.cables.contains_key(&i),
        Obj::Route(i) => doc.routes.contains_key(&i),
        Obj::Aid(i) => doc.aids.contains_key(&i),
        Obj::LoosePart(i) => doc.loose_parts.contains_key(&i),
        Obj::Project => true,
    }
}

/// Copy `obj` from `from` into `to` (or remove it there when `from` lacks it).
pub(super) fn copy(to: &mut Document, from: &Document, obj: Obj) {
    pub(super) fn put<K: Ord + Copy, V: Clone>(to: &mut BTreeMap<K, V>, from: &BTreeMap<K, V>, k: K) {
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
        Obj::Device(i) => put(&mut to.devices, &from.devices, i),
        Obj::ConnectionPoint(i) => put(&mut to.connection_points, &from.connection_points, i),
        Obj::Representation(i) => put(&mut to.representations, &from.representations, i),
        Obj::Item(i) => put(&mut to.items, &from.items, i),
        Obj::Wire(i) => put(&mut to.wires, &from.wires, i),
        Obj::Cable(i) => put(&mut to.cables, &from.cables, i),
        Obj::Route(i) => put(&mut to.routes, &from.routes, i),
        Obj::Aid(i) => put(&mut to.aids, &from.aids, i),
        Obj::LoosePart(i) => put(&mut to.loose_parts, &from.loose_parts, i),
        Obj::Project => {}
    }
}

pub(super) fn set<K: Ord + Copy, V>(map: &mut BTreeMap<K, V>, k: K, v: Option<V>) {
    match v {
        Some(v) => map.insert(k, v),
        None => map.remove(&k),
    };
}
