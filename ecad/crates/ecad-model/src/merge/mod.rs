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
//! - The result is ordinary ops from the target's state ([`crate::document::diff::ops_between_in`]) over
//!   the scope and what the rules added to it.
//! - Version names are the target's (they are branch-local history); project properties are
//!   merged only where `keep_project_key` allows (revision fields are never merged into the
//!   release line, owner answer 10).
//!
//! - A tag conflict also offers **Make them one** (brief 026, FP5, [`Choice::MakeOne`]): the
//!   source's device joins the target's through [`Inputs::join`] (018's device merge), or the
//!   source's cable's members join the target's cable; the target's property values are kept.
//!   When they cannot be one, the choice is left out and [`Plan::unavailable`] says why.
//!   Devices and cables share one tag namespace. Provisional tags (`-K?1`) added on both sides are
//!   not conflicts: the source's gets the next provisional number ([`Plan::provisional_renumbered`]).
//!
//! Excluded: "combine" for sibling folders (the rename choice covers it); an inline editor for
//! anything but property values.

use std::collections::{BTreeMap, BTreeSet};

use crate::document::diff;
use crate::document::doc::{
    Cable, ConnectionPoint, Core, CorePosition, Device, Document, End, Folder, Form, Item, ItemKind, Page, Properties, Representation, Subject, Wire,
};
use crate::document::invariant::Touched;
use crate::document::op::{ObjectRef, Op};
use crate::naming::id::{CableId, ConnectionPointId, DefinitionId, DeviceId, FolderId, ItemId, LayerId, PageId, RepresentationId, RouteId, WireId};
use crate::naming::tag::{self, HighWater};
use crate::objects::route::{Route, RouteEnd};
use crate::objects::styles::{Standard, StyleCatalogues, StyleLayer, StyleRecord, StyleRef};

/// An object a merge compares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Obj {
    Page(PageId),
    Folder(FolderId),
    Layer(LayerId),
    Definition(DefinitionId),
    Device(DeviceId),
    Terminal(ConnectionPointId),
    Representation(RepresentationId),
    Item(ItemId),
    Wire(WireId),
    Cable(CableId),
    /// A route (design 072 F31).
    Route(RouteId),
    /// A drawing aid (design 072 §6.2).
    Aid(crate::naming::id::AidId),
    /// A loose part (design 072 §6.4).
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
            Obj::Terminal(i) => i.hex(),
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
            Obj::Terminal(_) => "connection point",
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
            + t.terminals.len()
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
    /// Make the two devices (or cables) with one tag one (brief 026, FP5): the source's joins the
    /// target's.
    MakeOne,
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
    /// Answers given for conflicts the merge does not raise (any more), in words: they have no
    /// effect (reviewer 020b, finding 3).
    pub ignored: Vec<String>,
    /// Why Make them one is not offered, per tag conflict ID (brief 026).
    pub unavailable: BTreeMap<String, String>,
    /// Provisional tags of the source given the next provisional number (brief 026).
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
    /// Checks a value typed in an Edit field as an ordinary edit of that object's property would
    /// (design §5.2): the value as stored, or why it is refused.
    pub edit_rule: &'a dyn Fn(Obj, &str, &str) -> Result<String, String>,
    /// The ops making device `from` part of device `into` in a state (018's device merge), or why
    /// they cannot be one (brief 026, Make them one). The state's index is current.
    pub join: &'a dyn Fn(&Document, DeviceId, DeviceId) -> Result<Vec<Op>, String>,
}

/// How a kept object uses another one (what "leave it unconnected / remove what uses it" does).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Use {
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

struct Ctx<'a, 'b> {
    inp: &'a Inputs<'b>,
    choices: &'a BTreeMap<String, Choice>,
    conflicts: Vec<Conflict>,
    seen: BTreeSet<String>,
    problems: Vec<String>,
    unavailable: BTreeMap<String, String>,
    provisional: usize,
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

/// A value as a conflict shows it; unset is `(none)`.
fn shown_value<T: std::fmt::Display>(x: Option<&T>) -> String {
    x.map_or_else(|| "(none)".to_string(), |v| format!("\"{v}\""))
}

/// A property value as a person reads it (brief 045, FP1 condition 1): a value that is a
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
                    // Brief 045 (FP1): a value naming a component shows its tag.
                    target: shown(t.get(k).map(|v| value_words(&[ctx.inp.target, ctx.inp.source, ctx.inp.ancestor], v)).as_ref()),
                    source: shown(s.get(k).map(|v| value_words(&[ctx.inp.source, ctx.inp.target, ctx.inp.ancestor], v)).as_ref()),
                    choices: vec![Choice::Target, Choice::Source, Choice::Value(String::new())],
                    chosen: None,
                };
                match ctx.ask(c) {
                    Some(Choice::Source) => s.get(k).cloned(),
                    Some(Choice::Value(x)) => match (ctx.inp.edit_rule)(obj, k, &x) {
                        Ok(v) => Some(v),
                        Err(e) => {
                            ctx.problems.push(format!("{what} · {}: {e}", key_words(k)));
                            t.get(k).cloned()
                        }
                    },
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

/// Merge a device's type: a PLC station's and module's fields one by one when all three keep the
/// same group (design 072 §3, FP1: a station is shown by its tag); otherwise the type whole.
fn merge_device_type(
    ctx: &mut Ctx<'_, '_>, obj: Obj, what: &str, docs: &[&Document], a: &crate::DeviceType, t: &crate::DeviceType, s: &crate::DeviceType,
) -> crate::DeviceType {
    use crate::DeviceType as T;
    let words = |v: &dyn Fn() -> Option<String>| v().unwrap_or_else(|| "(none)".into());
    match (a, t, s) {
        (T::PlcModule(a), T::PlcModule(t), T::PlcModule(s)) => {
            let rack_name = |r: &Option<DeviceId>| r.map_or_else(|| "no rack".into(), |r| name_of(docs, Obj::Device(r)));
            T::PlcModule(crate::objects::plc::PlcModule {
                rack: field(ctx, obj, "plc.rack", format!("{what} · rack"), &a.rack, &t.rack, &s.rack, &rack_name),
                slot: field(ctx, obj, "plc.slot", format!("{what} · slot"), &a.slot, &t.slot, &s.slot, &|x| words(&|| x.map(|n| n.to_string()))),
                function: field(ctx, obj, "plc.function", format!("{what} · function and channels"), &a.function, &t.function, &s.function, &|f| {
                    f.word().replace('_', " ")
                }),
                attributes: field(ctx, obj, "plc.attributes", format!("{what} · attributes"), &a.attributes, &t.attributes, &s.attributes, &|x| {
                    format!("{x:?}")
                }),
                address: field(ctx, obj, "plc.addr", format!("{what} · pinned address"), &a.address, &t.address, &s.address, &|x| {
                    words(&|| x.start.map(|n| n.to_string()))
                }),
                supply: field(ctx, obj, "plc.supply", format!("{what} · supply"), &a.supply, &t.supply, &s.supply, &|x| {
                    words(&|| x.map(|v| v.to_string()))
                }),
            })
        }
        (T::PlcRack(a), T::PlcRack(t), T::PlcRack(s)) => T::PlcRack(crate::objects::plc::PlcRack {
            network: field(ctx, obj, "plc.network", format!("{what} · network"), &a.network, &t.network, &s.network, &|x| {
                words(&|| x.as_ref().map(ToString::to_string))
            }),
            address: field(ctx, obj, "plc.network.address", format!("{what} · network address"), &a.address, &t.address, &s.address, &|x| {
                words(&|| x.map(|v| v.to_string()))
            }),
            head: field(ctx, obj, "plc.head", format!("{what} · head"), &a.head, &t.head, &s.head, &|h| format!("{h:?}")),
            address_style: field(
                ctx,
                obj,
                "plc.address.style",
                format!("{what} · address style"),
                &a.address_style,
                &t.address_style,
                &s.address_style,
                &|x| words(&|| x.map(|v| crate::Word::word(v).to_string())),
            ),
        }),
        _ => field(ctx, obj, "device_type", format!("{what} · type"), a, t, s, &|d| d.group().label().into()),
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

/// The object that owns a connection point.
fn owner_obj(owner: crate::document::doc::ConnectionPointOwner) -> Obj {
    match owner {
        crate::document::doc::ConnectionPointOwner::Device(c) => Obj::Device(c),
        crate::document::doc::ConnectionPointOwner::Aid(a) => Obj::Aid(a),
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
        Obj::Terminal(t) => match docs.iter().find_map(|d| d.terminals.get(&t)) {
            Some(term) => format!(
                "{}:{}",
                match term.owner {
                    crate::document::doc::ConnectionPointOwner::Device(c) => tag_of(c),
                    crate::document::doc::ConnectionPointOwner::Aid(a) => a.short(),
                },
                term.label.clone().or_else(|| term.key.as_ref().map(ToString::to_string)).unwrap_or_else(|| t.short())
            ),
            None => t.short(),
        },
        Obj::Wire(c) => match docs.iter().find_map(|d| d.wires.get(&c)).and_then(|w| w.number_text().map(str::to_string)) {
            Some(n) => format!("Wire {n}"),
            None => format!("Wire {}", c.short()),
        },
        Obj::Cable(c) => match docs.iter().find_map(|d| d.cables.get(&c)).and_then(|w| w.properties.get(tag::TAG_KEY).cloned()) {
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
fn page_in(doc: &Document, obj: Obj) -> Option<PageId> {
    match obj {
        Obj::Page(p) => Some(p),
        Obj::Item(i) => doc.items.get(&i).map(|i| i.page),
        Obj::Representation(r) => doc.representations.get(&r).map(|r| r.page),
        Obj::Device(c) => doc.representations_of(Subject::Device(c)).map(|(_, r)| r.page).next(),
        Obj::Wire(c) => doc.representations_of(Subject::Wire(c)).map(|(_, r)| r.page).next(),
        Obj::Cable(c) => doc.representations_of(Subject::Cable(c)).map(|(_, r)| r.page).next(),
        Obj::Route(r) => doc.routes.get(&r).map(|r| r.page),
        Obj::Aid(x) => doc.representations_of(Subject::Aid(x)).map(|(_, r)| r.page).next(),
        Obj::Terminal(t) => doc.terminals.get(&t).and_then(|t| page_in(doc, owner_obj(t.owner))),
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
    out.extend(t.devices.iter().map(|x| Obj::Device(*x)));
    out.extend(t.terminals.iter().map(|x| Obj::Terminal(*x)));
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
fn differs(a: &Document, b: &Document, obj: Obj) -> bool {
    match obj {
        Obj::Page(i) => a.pages.get(&i) != b.pages.get(&i),
        Obj::Folder(i) => a.folders.get(&i) != b.folders.get(&i),
        Obj::Layer(i) => a.layers.get(&i) != b.layers.get(&i),
        Obj::Definition(i) => a.definitions.get(&i) != b.definitions.get(&i),
        Obj::Device(i) => a.devices.get(&i) != b.devices.get(&i),
        Obj::Terminal(i) => a.terminals.get(&i) != b.terminals.get(&i),
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
fn holds(doc: &Document, obj: Obj) -> bool {
    match obj {
        Obj::Page(i) => doc.pages.contains_key(&i),
        Obj::Folder(i) => doc.folders.contains_key(&i),
        Obj::Layer(i) => doc.layers.contains_key(&i),
        Obj::Definition(i) => doc.definitions.contains_key(&i),
        Obj::Device(i) => doc.devices.contains_key(&i),
        Obj::Terminal(i) => doc.terminals.contains_key(&i),
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
        Obj::Device(i) => put(&mut to.devices, &from.devices, i),
        Obj::Terminal(i) => put(&mut to.terminals, &from.terminals, i),
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

fn set<K: Ord + Copy, V>(map: &mut BTreeMap<K, V>, k: K, v: Option<V>) {
    match v {
        Some(v) => map.insert(k, v),
        None => map.remove(&k),
    };
}

/// A device's text fields as a bag keyed by their `setprop` keys, so they merge as its properties
/// do (one conflict per key, `<id>/prop.<key>`, with an edited value offered).
fn text_bag(d: &Device) -> Properties {
    crate::TextField::ALL.into_iter().filter_map(|f| d.text(f).map(|v| (f.key().to_string(), v.to_string()))).collect()
}

/// `d` with the text fields of the merged bag `texts`; a value the field can't hold keeps the
/// target's and is a problem.
fn with_texts(ctx: &mut Ctx<'_, '_>, what: &str, mut d: Device, t: &Device, texts: &Properties) -> Device {
    for f in crate::TextField::ALL {
        if let Err(e) = d.set_text(f, texts.get(f.key()).map(String::as_str)) {
            ctx.problems.push(format!("{what} · {}: {e}", key_words(f.key())));
            let _ = d.set_text(f, t.text(f));
        }
    }
    d
}

/// Merge every object of the scope into `m` (a copy of the target).
fn merge_objects(ctx: &mut Ctx<'_, '_>, m: &mut Document, objects: &[Obj]) {
    let (a, t, s) = (ctx.inp.ancestor, ctx.inp.target, ctx.inp.source);
    let docs = [t, s, a];
    let all = |_: &str| true;
    for &obj in objects {
        let what = name_of(&docs, obj);
        match obj {
            Obj::Device(id) => {
                let v = object(ctx, obj, &what, a.devices.get(&id), t.devices.get(&id), s.devices.get(&id), |ctx, a, t, s| {
                    let texts = merge_props(ctx, obj, &what, &text_bag(a), &text_bag(t), &text_bag(s), &all);
                    let d = Device {
                        tag: field(ctx, obj, tag::TAG_KEY, format!("{what} · tag"), &a.tag, &t.tag, &s.tag, &|v| {
                            v.as_ref().map_or_else(|| "(none)".into(), |t| t.to_string())
                        }),
                        tag_fixed: field(ctx, obj, tag::FIXED_KEY, format!("{what} · tag fixed"), &a.tag_fixed, &t.tag_fixed, &s.tag_fixed, &|v| {
                            crate::naming::words::flag_word(*v).into()
                        }),
                        location: field(
                            ctx,
                            obj,
                            crate::document::book::LOCATION_KEY,
                            format!("{what} · location"),
                            &a.location,
                            &t.location,
                            &s.location,
                            &|x| x.map_or_else(|| "(none)".to_string(), crate::naming::id::LocationId::short),
                        ),
                        location_fixed: field(
                            ctx,
                            obj,
                            crate::document::book::LOCATION_FIXED_KEY,
                            format!("{what} · location fixed"),
                            &a.location_fixed,
                            &t.location_fixed,
                            &s.location_fixed,
                            &|v| crate::naming::words::flag_word(*v).into(),
                        ),
                        function: field(
                            ctx,
                            obj,
                            crate::document::book::FUNCTION_KEY,
                            format!("{what} · function"),
                            &a.function,
                            &t.function,
                            &s.function,
                            &|x| x.map_or_else(|| "(none)".to_string(), crate::naming::id::FunctionId::short),
                        ),
                        function_fixed: field(
                            ctx,
                            obj,
                            crate::document::book::FUNCTION_FIXED_KEY,
                            format!("{what} · function fixed"),
                            &a.function_fixed,
                            &t.function_fixed,
                            &s.function_fixed,
                            &|v| crate::naming::words::flag_word(*v).into(),
                        ),
                        properties: merge_props(ctx, obj, &what, &a.properties, &t.properties, &s.properties, &all),
                        definition: field(ctx, obj, "definition", format!("{what} · symbol"), &a.definition, &t.definition, &s.definition, &|d| {
                            d.map_or_else(|| "(none)".into(), |d| name_of(&docs, Obj::Definition(d)))
                        }),
                        device_type: merge_device_type(ctx, obj, &what, &docs, &a.device_type, &t.device_type, &s.device_type),
                        ..Device::default()
                    };
                    with_texts(ctx, &what, d, t, &texts)
                });
                set(&mut m.devices, id, v);
            }
            Obj::Wire(id) => {
                let v = object(ctx, obj, &what, a.wires.get(&id), t.wires.get(&id), s.wires.get(&id), |ctx, a, t, s| {
                    let opt = |x: Option<String>| x.unwrap_or_else(|| "(none)".into());
                    Wire {
                        net: field(ctx, obj, "net", format!("{what} · net"), &a.net, &t.net, &s.net, &|x| x.to_string()),
                        conductor: t.conductor,
                        net_order: field(
                            ctx,
                            obj,
                            "net_order",
                            format!("{what} · place in its net"),
                            &a.net_order,
                            &t.net_order,
                            &s.net_order,
                            &|x| x.to_string(),
                        ),
                        ends: field(ctx, obj, "ends", format!("{what} · connection points"), &a.ends, &t.ends, &s.ends, &|e| {
                            format!("{} – {}", name_of(&docs, Obj::Terminal(e[0])), name_of(&docs, Obj::Terminal(e[1])))
                        }),
                        order_fixed: field(
                            ctx,
                            obj,
                            "order_fixed",
                            format!("{what} · order fixed"),
                            &a.order_fixed,
                            &t.order_fixed,
                            &s.order_fixed,
                            &|x| {
                                if *x { "yes".into() } else { "no".into() }
                            },
                        ),
                        wire_type: field(ctx, obj, "wire_type", format!("{what} · wire type"), &a.wire_type, &t.wire_type, &s.wire_type, &|x| {
                            opt(x.as_ref().map(StyleRef::text))
                        }),
                        size: field(ctx, obj, "size", format!("{what} · size"), &a.size, &t.size, &s.size, &|x| opt(x.map(|v| v.to_string()))),
                        colours: field(ctx, obj, "colours", format!("{what} · colours"), &a.colours, &t.colours, &s.colours, &|x| {
                            opt(x.as_ref().map(|v| v.to_string()))
                        }),
                        number: field(ctx, obj, "number", format!("{what} · wire number"), &a.number, &t.number, &s.number, &|x| {
                            opt(x.as_ref().map(|n| if n.fixed { format!("{} (fixed)", n.text) } else { n.text.to_string() }))
                        }),
                        potential: field(ctx, obj, "potential", format!("{what} · potential"), &a.potential, &t.potential, &s.potential, &|x| {
                            opt(x.as_ref().map(|v| v.to_string()))
                        }),
                        rating: field(ctx, obj, "rating", format!("{what} · rating inputs"), &a.rating, &t.rating, &s.rating, &|x| format!("{x:?}")),
                        load: field(ctx, obj, "load", format!("{what} · load"), &a.load, &t.load, &s.load, &|x| opt(x.map(|v| v.to_string()))),
                        protected_by: field(
                            ctx,
                            obj,
                            "protected_by",
                            format!("{what} · protected by"),
                            &a.protected_by,
                            &t.protected_by,
                            &s.protected_by,
                            &|x| opt(x.map(|p| name_of(&docs, Obj::Device(p.device)))),
                        ),
                        display: merge_props(ctx, obj, &what, &a.display, &t.display, &s.display, &all),
                    }
                });
                set(&mut m.wires, id, v);
            }
            Obj::Cable(id) => {
                let v = object(ctx, obj, &what, a.cables.get(&id), t.cables.get(&id), s.cables.get(&id), |ctx, a, t, s| Cable {
                    properties: merge_props(ctx, obj, &what, &a.properties, &t.properties, &s.properties, &all),
                    cores: merge_members(&a.cores, &t.cores, &s.cores),
                    cable_type: field(ctx, obj, "cable_type", format!("{what} · cable type"), &a.cable_type, &t.cable_type, &s.cable_type, &|x| {
                        x.as_ref().map_or_else(|| "(none)".to_string(), StyleRef::text)
                    }),
                    shield: field(ctx, obj, "shield", format!("{what} · shield"), &a.shield, &t.shield, &s.shield, &|x| {
                        x.map_or_else(|| "(none)".to_string(), |v| crate::naming::words::Word::word(v).to_string())
                    }),
                    rating: field(ctx, obj, "rating", format!("{what} · rating inputs"), &a.rating, &t.rating, &s.rating, &|x| format!("{x:?}")),
                    load: field(ctx, obj, "load", format!("{what} · load"), &a.load, &t.load, &s.load, &|x| {
                        x.map_or_else(|| "(none)".to_string(), |v| v.to_string())
                    }),
                    protected_by: field(
                        ctx,
                        obj,
                        "protected_by",
                        format!("{what} · protected by"),
                        &a.protected_by,
                        &t.protected_by,
                        &s.protected_by,
                        &|x| x.map_or_else(|| "(none)".to_string(), |p| name_of(&docs, Obj::Device(p.device))),
                    ),
                });
                set(&mut m.cables, id, v);
            }
            Obj::Aid(id) => {
                let v = object(ctx, obj, &what, a.aids.get(&id), t.aids.get(&id), s.aids.get(&id), |ctx, a, t, s| {
                    field(ctx, obj, "aid", format!("{what} · changed on both"), a, t, s, &|x| format!("{x:?}"))
                });
                set(&mut m.aids, id, v);
            }
            Obj::LoosePart(id) => {
                let v = object(ctx, obj, &what, a.loose_parts.get(&id), t.loose_parts.get(&id), s.loose_parts.get(&id), |ctx, a, t, s| {
                    field(ctx, obj, "loose_part", format!("{what} · changed on both"), a, t, s, &|x| format!("{x:?}"))
                });
                set(&mut m.loose_parts, id, v);
            }
            Obj::Route(id) => {
                let v = object(ctx, obj, &what, a.routes.get(&id), t.routes.get(&id), s.routes.get(&id), |ctx, a, t, s| Route {
                    page: field(ctx, obj, "page", format!("{what} · page"), &a.page, &t.page, &s.page, &|p| name_of(&docs, Obj::Page(*p))),
                    points: field(ctx, obj, "points", format!("{what} · drawn differently on both"), &a.points, &t.points, &s.points, &|p| {
                        format!("{} points", p.len())
                    }),
                    ends: field(ctx, obj, "ends", format!("{what} · ends"), &a.ends, &t.ends, &s.ends, &|e| format!("{e:?}")),
                });
                set(&mut m.routes, id, v);
            }
            Obj::Page(id) => {
                let v = object(ctx, obj, &what, a.pages.get(&id), t.pages.get(&id), s.pages.get(&id), |ctx, a, t, s| Page {
                    title: field(ctx, obj, "title", format!("{what} · title"), &a.title, &t.title, &s.title, &|x| format!("\"{x}\"")),
                    number: field(ctx, obj, "number", format!("{what} · number"), &a.number, &t.number, &s.number, &|x| shown_value(x.as_ref())),
                    folder: field(ctx, obj, "folder", format!("{what} · folder"), &a.folder, &t.folder, &s.folder, &|x| {
                        x.map_or_else(|| "(top level)".to_string(), FolderId::short)
                    }),
                    kind: field(ctx, obj, "kind", format!("{what} · kind"), &a.kind, &t.kind, &s.kind, &|x| {
                        x.generated_kind().map_or("schematic", crate::Word::word).to_string()
                    }),
                    page_type: field(ctx, obj, "page_type", format!("{what} · page type"), &a.page_type, &t.page_type, &s.page_type, &|x| {
                        shown_value(x.as_ref())
                    }),
                    sheet: field(ctx, obj, "sheet", format!("{what} · sheet"), &a.sheet, &t.sheet, &s.sheet, &|x| {
                        x.map_or_else(|| "(project's)".to_string(), |s| s.text())
                    }),
                    title_block: field(
                        ctx,
                        obj,
                        "title_block",
                        format!("{what} · title block"),
                        &a.title_block,
                        &t.title_block,
                        &s.title_block,
                        &|x| shown_value(x.as_ref()),
                    ),
                    location: field(ctx, obj, "location", format!("{what} · location"), &a.location, &t.location, &s.location, &|x| {
                        x.map_or_else(|| "(project's)".to_string(), crate::naming::id::LocationId::short)
                    }),
                    function: field(ctx, obj, "function", format!("{what} · function"), &a.function, &t.function, &s.function, &|x| {
                        x.map_or_else(|| "(project's)".to_string(), crate::naming::id::FunctionId::short)
                    }),
                    display: merge_props(ctx, obj, &what, &a.display, &t.display, &s.display, &all),
                });
                set(&mut m.pages, id, v);
            }
            Obj::Folder(id) => {
                let v = object(ctx, obj, &what, a.folders.get(&id), t.folders.get(&id), s.folders.get(&id), |ctx, a, t, s| Folder {
                    name: field(ctx, obj, "name", format!("{what} · name"), &a.name, &t.name, &s.name, &|x| format!("\"{x}\"")),
                    parent: field(ctx, obj, "parent", format!("{what} · placed in"), &a.parent, &t.parent, &s.parent, &|x| {
                        x.map_or_else(|| "the top level".into(), |f| name_of(&docs, Obj::Folder(f)))
                    }),
                    start: field(ctx, obj, "start", format!("{what} · start number"), &a.start, &t.start, &s.start, &|x| shown_value(x.as_ref())),
                });
                set(&mut m.folders, id, v);
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
                    let show = |x: &ConnectionPoint| {
                        x.label.clone().or_else(|| x.key.as_ref().map(ToString::to_string)).unwrap_or_else(|| "(no label)".into())
                    };
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
                m.display = merge_props(ctx, obj, "Project", &a.display, &t.display, &s.display, ctx.inp.keep_project_key);
                m.project = merge_project(ctx, obj, &a.project, &t.project, &s.project);
                m.device_links =
                    field(ctx, obj, "device_links", "Project · device links".into(), &a.device_links, &t.device_links, &s.device_links, &|x| {
                        format!("{} device links", x.len())
                    });
                m.settings =
                    field(ctx, obj, "settings", "Project settings · changed on both".into(), &a.settings, &t.settings, &s.settings, &|_| {
                        "its settings".into()
                    });
                merge_standard(ctx, m, obj);
            }
        }
    }
}

/// The project's own data (design 072 §5.3): each list merges entry by entry (an entry changed on
/// both sides differently is a conflict), the defaults field by field, the revision rows whole.
fn merge_project(ctx: &mut Ctx<'_, '_>, obj: Obj, a: &crate::Project, t: &crate::Project, s: &crate::Project) -> crate::Project {
    // The info merges field by field, like the project properties it was.
    let mut info = t.info.clone();
    for f in <crate::document::project::InfoField as crate::Word>::ALL {
        let get = |p: &crate::Project| p.info.get(*f).map(str::to_string);
        let what = format!("Project · {}", crate::Word::word(*f));
        let v = field(ctx, obj, crate::Word::word(*f), what, &get(a), &get(t), &get(s), &|x| shown_value(x.as_ref()));
        info = info.with(*f, v.as_deref()).unwrap_or(info);
    }
    crate::Project {
        info,
        locations: merge_list(ctx, obj, "location", &a.locations, &t.locations, &s.locations),
        functions: merge_list(ctx, obj, "function", &a.functions, &t.functions, &s.functions),
        location: field(ctx, obj, "location", "Project · location".into(), &a.location, &t.location, &s.location, &|x| {
            x.map_or_else(|| "(none)".to_string(), crate::naming::id::LocationId::short)
        }),
        function: field(ctx, obj, "function", "Project · function".into(), &a.function, &t.function, &s.function, &|x| {
            x.map_or_else(|| "(none)".to_string(), crate::naming::id::FunctionId::short)
        }),
        // Revision rows are merged only where the filter allows (never into the release line; an
        // update brings them unchanged), as one unit.
        user_fields: field(ctx, obj, "user_fields", "Project · user fields".into(), &a.user_fields, &t.user_fields, &s.user_fields, &|x| {
            format!("{} user fields", x.len())
        }),
        revisions: if (ctx.inp.keep_project_key)(crate::document::project::REVISIONS_KEY) {
            field(ctx, obj, "revisions", "Project · revisions".into(), &a.revisions, &t.revisions, &s.revisions, &|x| {
                format!("{} revisions", x.len())
            })
        } else {
            t.revisions.clone()
        },
    }
}

/// One list of the project, merged by entry id: the target's order, then the source's new entries.
fn merge_list<I: Copy + Ord + std::fmt::Display>(
    ctx: &mut Ctx<'_, '_>, obj: Obj, what: &str, a: &[crate::ListEntry<I>], t: &[crate::ListEntry<I>], s: &[crate::ListEntry<I>],
) -> Vec<crate::ListEntry<I>> {
    let find = |l: &[crate::ListEntry<I>], id: I| l.iter().find(|e| e.id == id).cloned();
    let mut ids: Vec<I> = t.iter().map(|e| e.id).collect();
    ids.extend(s.iter().map(|e| e.id).filter(|id| !t.iter().any(|e| e.id == *id)));
    ids.extend(a.iter().map(|e| e.id).filter(|id| !t.iter().chain(s).any(|e| e.id == *id)));
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        let v = field(ctx, obj, &format!("{what}.{id}"), format!("Project · {what} {id}"), &find(a, id), &find(t, id), &find(s, id), &|x| {
            x.as_ref().map_or_else(|| "(removed)".to_string(), |e| format!("\"{}\"", e.code))
        });
        out.extend(v);
    }
    out
}

/// The standard and the style records (brief 050a, design 050 §3.5): the standard's header is
/// one unit, its values merge per key like project properties, and each style record
/// `(layer, catalogue, id)` is one unit. A standard removed on one side and changed in any part
/// on the other is one removed conflict; removed on one side and untouched on the other, it goes.
fn merge_standard(ctx: &mut Ctx<'_, '_>, m: &mut Document, obj: Obj) {
    let (a, t, s) = (ctx.inp.ancestor, ctx.inp.target, ctx.inp.source);
    // Risk 20: a standard removed on one side and changed on the other (its header, a value or a
    // standard-layer style) is one conflict, keep it with those changes or remove it; merged per
    // part, the header alone would decide and the other side's changes would vanish.
    let removed = |d: &Document| a.standard.is_some() && d.standard.is_none();
    let changed = |d: &Document| d.standard.is_some() && d.standard != a.standard;
    let one_sided = match (removed(t), removed(s)) {
        (true, false) if changed(s) => Some(false),
        (false, true) if changed(t) => Some(true),
        _ => None,
    };
    if let Some(target_keeps) = one_sided {
        let name = a.standard.as_ref().map_or_else(String::new, |x| x.header.name.to_string());
        let (on_changed, on_removed) = if target_keeps { (ctx.t(), ctx.s()) } else { (ctx.s(), ctx.t()) };
        let word = |keeps: bool| if keeps { "changed" } else { "removed" };
        let c = Conflict {
            id: format!("{}/standard", obj.hex()),
            kind: ConflictKind::Removed,
            object: obj,
            what: format!("Standard \"{name}\" · removed on {on_removed}; changed on {on_changed}"),
            target: word(target_keeps).into(),
            source: word(!target_keeps).into(),
            choices: vec![Choice::Target, Choice::Source],
            chosen: None,
        };
        let keep = match ctx.ask(c) {
            Some(Choice::Source) => !target_keeps,
            _ => target_keeps,
        };
        let keeper = if target_keeps { t } else { s };
        m.styles = merge_style_layer(ctx, StyleLayer::Project, &a.styles, &t.styles, &s.styles);
        m.standard = if keep { keeper.standard.clone() } else { None };
        return;
    }
    let all = |_: &str| true;
    let header = |d: &Document| d.standard.as_ref().map(|x| x.header.clone());
    let head = field(ctx, obj, "standard", "Standard · changed on both".into(), &header(a), &header(t), &header(s), &|h| {
        h.as_ref().map_or_else(|| "IEC (built in)".into(), |h| format!("\"{}\"", h.name))
    });
    let empty = Properties::new();
    fn values<'d>(d: &'d Document, empty: &'d Properties) -> &'d Properties {
        d.standard.as_ref().map_or(empty, |x| &x.values)
    }
    let merged_values = merge_props(ctx, obj, "Standard", values(a, &empty), values(t, &empty), values(s, &empty), &all);
    let no_cats = StyleCatalogues::new();
    fn std_cats<'d>(d: &'d Document, none: &'d StyleCatalogues) -> &'d StyleCatalogues {
        d.standard.as_ref().map_or(none, |x| &x.styles)
    }
    let std_styles = merge_style_layer(ctx, StyleLayer::Standard, std_cats(a, &no_cats), std_cats(t, &no_cats), std_cats(s, &no_cats));
    m.styles = merge_style_layer(ctx, StyleLayer::Project, &a.styles, &t.styles, &s.styles);
    m.standard = head.map(|header| Standard { header, values: merged_values, styles: std_styles });
}

/// A style record's name in texts: `Text style "Tag large"` (`Wire category "24 V DC"`).
pub fn style_words(catalogue: crate::objects::styles::StyleCatalogue, id: &StyleRef, record: Option<&StyleRecord>) -> String {
    use crate::objects::styles::StyleCatalogue as C;
    let kind = match catalogue {
        C::Text => "Text style",
        C::Location => "Location box style",
        C::Frame => "Frame style",
        C::Wire => "Wire type",
        C::Page | C::Cable | C::RatingTable => "Style",
    };
    match record.and_then(|r| r.name.as_ref()).map(|n| n.as_str()) {
        Some(n) => format!("{kind} \"{n}\""),
        None => format!("{kind} {id}"),
    }
}

/// Merge one layer's style records, each record whole.
fn merge_style_layer(ctx: &mut Ctx<'_, '_>, layer: StyleLayer, a: &StyleCatalogues, t: &StyleCatalogues, s: &StyleCatalogues) -> StyleCatalogues {
    let mut out = t.clone();
    let cats: BTreeSet<crate::objects::styles::StyleCatalogue> = a.keys().chain(t.keys()).chain(s.keys()).copied().collect();
    for cat in cats {
        let get = |c: &'_ StyleCatalogues| c.get(&cat).cloned().unwrap_or_default();
        let (ra, rt, rs) = (get(a), get(t), get(s));
        let ids: BTreeSet<&StyleRef> = ra.keys().chain(rt.keys()).chain(rs.keys()).collect();
        for id in ids {
            let (x, y, z) = (ra.get(id), rt.get(id), rs.get(id));
            let v = match pick(x, y, z) {
                Some(v) => v,
                None => {
                    let what = style_words(cat, id, y.or(z).or(x));
                    let show = |r: Option<&StyleRecord>| match r {
                        None => "removed".to_string(),
                        Some(r) => {
                            let f: Vec<String> = r.body.entries().iter().map(|(k, v)| format!("{k} {v}")).collect();
                            if f.is_empty() { "no changes".into() } else { f.join(", ") }
                        }
                    };
                    let c = Conflict {
                        id: format!("project/style.{}.{cat}.{id}", layer.word()),
                        kind: ConflictKind::Value,
                        object: Obj::Project,
                        what: format!("{what} · changed on both"),
                        target: show(y),
                        source: show(z),
                        choices: vec![Choice::Target, Choice::Source],
                        chosen: None,
                    };
                    match ctx.ask(c) {
                        Some(Choice::Source) => z.cloned(),
                        _ => y.cloned(),
                    }
                }
            };
            crate::objects::styles::put(&mut out, cat, id, v.as_ref());
        }
    }
    out
}

/// The object with ID `hex` in any of `docs`.
fn obj_by_hex(docs: &[&Document], hex: &str) -> Option<Obj> {
    let id = u128::from_str_radix(hex, 16).ok()?;
    docs.iter().find_map(|d| {
        use crate::naming::id::*;
        [
            Obj::Device(DeviceId(id)),
            Obj::Terminal(ConnectionPointId(id)),
            Obj::Wire(WireId(id)),
            Obj::Cable(CableId(id)),
            Obj::Page(PageId(id)),
            Obj::Representation(RepresentationId(id)),
            Obj::Item(ItemId(id)),
            Obj::Folder(FolderId(id)),
            Obj::Layer(LayerId(id)),
            Obj::Definition(DefinitionId(id)),
        ]
        .into_iter()
        .find(|o| holds(d, *o))
    })
}

/// What a device kept from the source brings back with it (design §5.2: "re-adds it with the
/// branch's values and whatever it needs"): its connection points and its symbols (with their
/// drawing items) that the source has and the result lacks.
fn device_needs(m: &Document, s: &Document, c: DeviceId) -> Vec<Obj> {
    let mut out: Vec<Obj> =
        s.terminals.iter().filter(|(k, t)| t.device() == Some(c) && !m.terminals.contains_key(k)).map(|(k, _)| Obj::Terminal(*k)).collect();
    for (r, rep) in s.representations.iter().filter(|(_, r)| r.subject == Subject::Device(c)) {
        if !m.representations.contains_key(r) {
            out.push(Obj::Representation(*r));
            out.extend(rep.items.iter().filter(|i| !m.items.contains_key(i)).map(|i| Obj::Item(*i)));
        }
    }
    out
}

/// Devices kept from the source by a removed conflict's answer bring back what they need, listed
/// in the conflict's row (reviewer 020b, finding 4). A connection point or symbol another answer
/// removes is a problem with a choice, never a device merged silently without it. Returns what
/// was brought back.
fn keep_devices(ctx: &mut Ctx<'_, '_>, m: &mut Document) -> BTreeSet<Obj> {
    let (t, s) = (ctx.inp.target, ctx.inp.source);
    let docs = [s, t, ctx.inp.ancestor];
    let mut out = BTreeSet::new();
    let removed: Vec<(usize, DeviceId)> = ctx
        .conflicts
        .iter()
        .enumerate()
        .filter_map(|(i, k)| match k.object {
            Obj::Device(c) if k.kind == ConflictKind::Removed && !t.devices.contains_key(&c) && s.devices.contains_key(&c) => Some((i, c)),
            _ => None,
        })
        .collect();
    for (i, c) in removed {
        // What keeping it brings back, listed in the row whatever the answer.
        let needs = device_needs(t, s, c);
        let terminals = needs.iter().filter(|o| matches!(o, Obj::Terminal(_))).count();
        let symbols = needs.iter().filter(|o| matches!(o, Obj::Representation(_))).count();
        if let Some(k) = ctx.conflicts.get_mut(i) {
            let mut parts = Vec::new();
            if terminals > 0 {
                let names: Vec<String> = needs.iter().filter(|o| matches!(o, Obj::Terminal(_))).take(4).map(|o| name_of(&docs, *o)).collect();
                parts.push(format!("connection point{} {}", if terminals == 1 { "" } else { "s" }, names.join(", ")));
            }
            if symbols > 0 {
                parts.push(format!("{symbols} symbol{}", if symbols == 1 { "" } else { "s" }));
            }
            if !parts.is_empty() {
                k.source = format!("{} (brings back {})", k.source, parts.join(" and "));
            }
        }
        if !m.devices.contains_key(&c) {
            continue;
        }
        for o in device_needs(m, s, c) {
            // Another answer removed it: say so with the way through.
            let answered_out = ctx.conflicts.iter().any(|k| k.object == o && k.kind == ConflictKind::Removed && k.chosen == Some(Choice::Target));
            if answered_out {
                ctx.problems.push(format!(
                    "Keeping {} needs {}: keep {} too, or keep {} removed, first.",
                    name_of(&docs, Obj::Device(c)),
                    name_of(&docs, o),
                    name_of(&docs, o),
                    name_of(&docs, Obj::Device(c))
                ));
                continue;
            }
            copy(m, s, o);
            out.insert(o);
        }
    }
    out
}

/// The cores on a line that went: each stays unplaced or goes ([`Core::off_line`]), in place.
fn off_line(cores: &mut Vec<Core>, gone: impl Fn(&Core) -> bool) {
    let mut out: Vec<Core> = Vec::with_capacity(cores.len());
    for c in cores.drain(..) {
        if !gone(&c) {
            out.push(c);
        } else if let Some(k) = c.off_line(&out) {
            out.push(k);
        }
    }
    *cores = out;
}

/// Cable cores: adds and removes from both sides combine (design §5.1). A core is the same core
/// on both sides when it is on the same line, or, unplaced, has the same label. Each of its fields
/// is the side that changed it (the target's when both did).
fn merge_members(a: &[Core], t: &[Core], s: &[Core]) -> Vec<Core> {
    fn has<'a>(l: &'a [Core], c: &Core) -> Option<&'a Core> {
        l.iter().find(|m| m.position == c.position && (c.position != CorePosition::Unplaced || m.label == c.label))
    }
    fn field<T: PartialEq + Clone>(am: Option<&Core>, m: &Core, sm: Option<&Core>, f: fn(&Core) -> &T) -> T {
        pick(am.map(f), Some(f(m)), sm.map(f)).flatten().unwrap_or_else(|| f(m).clone())
    }
    let mut out: Vec<Core> = Vec::new();
    for m in t {
        let (am, sm) = (has(a, m), has(s, m));
        if am.is_some() && sm.is_none() {
            continue; // removed by the source
        }
        out.push(Core {
            label: field(am, m, sm, |x| &x.label),
            colours: field(am, m, sm, |x| &x.colours),
            purpose: field(am, m, sm, |x| &x.purpose),
            position: m.position,
            through: field(am, m, sm, |x| &x.through),
        });
    }
    for m in s {
        if has(t, m).is_none() && has(a, m).is_none() {
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
        Obj::Device(c) => {
            if let Some(d) = m.devices.get(&c).and_then(|c| c.definition) {
                out.push((Obj::Definition(d), Use::DeviceDefinition));
            }
        }
        Obj::Terminal(t) => {
            if let Some(t) = m.terminals.get(&t) {
                out.push((owner_obj(t.owner), Use::Whole));
            }
        }
        Obj::Representation(r) => {
            if let Some(r) = m.representations.get(&r) {
                out.push((
                    match r.subject {
                        Subject::Device(c) => Obj::Device(c),
                        Subject::Wire(c) => Obj::Wire(c),
                        Subject::Cable(c) => Obj::Cable(c),
                        Subject::Route(c) => Obj::Route(c),
                        Subject::Aid(c) => Obj::Aid(c),
                    },
                    Use::Whole,
                ));
                out.push((Obj::Page(r.page), Use::Whole));
                out.extend(r.items.iter().map(|i| (Obj::Item(*i), Use::RepItem)));
                out.extend(r.connection_points.keys().map(|t| (Obj::Terminal(*t), Use::ConnectionPoint)));
            }
        }
        Obj::Wire(c) => {
            if let Some(c) = m.wires.get(&c) {
                for e in End::BOTH {
                    out.push((Obj::Terminal(c.end(e)), Use::End(e)));
                }
            }
        }
        Obj::Cable(c) => {
            if let Some(c) = m.cables.get(&c) {
                out.extend(c.cores.iter().filter_map(|x| match x.position {
                    CorePosition::Wire(w) => Some((Obj::Wire(w), Use::Member)),
                    CorePosition::Route { route, .. } => Some((Obj::Route(route), Use::Member)),
                    CorePosition::Unplaced => None,
                }));
            }
        }
        Obj::Page(p) => {
            if let Some(f) = crate::document::book::folder_of(m, p) {
                out.push((Obj::Folder(f), Use::PageFolder));
            }
        }
        Obj::Folder(f) => {
            if let Some(p) = m.folders.get(&f).and_then(|f| f.parent) {
                out.push((Obj::Folder(p), Use::FolderParent));
            }
        }
        Obj::Route(r) => {
            if let Some(r) = m.routes.get(&r) {
                out.push((Obj::Page(r.page), Use::Whole));
                for (i, e) in r.ends.iter().enumerate() {
                    match e {
                        RouteEnd::Point(t) => out.push((Obj::Terminal(*t), Use::RouteEnd(i))),
                        RouteEnd::On { route, .. } => out.push((Obj::Route(*route), Use::RouteEnd(i))),
                        RouteEnd::Free => {}
                    }
                }
            }
        }
        Obj::Layer(_) | Obj::Definition(_) | Obj::Aid(_) | Obj::LoosePart(_) | Obj::Project => {}
    }
    out
}

/// Every object of `m` that refers to `y` (a scan: used only when a "remove what uses it"
/// choice removes an object whole).
fn users_of(m: &Document, y: Obj) -> Vec<Obj> {
    let mut all: Vec<Obj> = Vec::new();
    match y {
        Obj::Terminal(t) => {
            all.extend(m.wires.iter().filter(|(_, c)| c.ends.contains(&t)).map(|(k, _)| Obj::Wire(*k)));
            all.extend(m.representations.iter().filter(|(_, r)| r.connection_points.contains_key(&t)).map(|(k, _)| Obj::Representation(*k)));
            all.extend(m.routes.iter().filter(|(_, r)| r.ends.contains(&RouteEnd::Point(t))).map(|(k, _)| Obj::Route(*k)));
        }
        Obj::Aid(x) => {
            all.extend(m.representations.iter().filter(|(_, r)| r.subject == Subject::Aid(x)).map(|(k, _)| Obj::Representation(*k)));
        }
        Obj::Route(x) => {
            all.extend(m.routes.iter().filter(|(_, r)| r.ends.iter().any(|e| e.route() == Some(x))).map(|(k, _)| Obj::Route(*k)));
            all.extend(m.cables.iter().filter(|(_, c)| c.cores.iter().any(|mm| mm.route() == Some(x))).map(|(k, _)| Obj::Cable(*k)));
            all.extend(m.representations.iter().filter(|(_, r)| r.subject == Subject::Route(x)).map(|(k, _)| Obj::Representation(*k)));
        }
        Obj::Device(c) => {
            all.extend(m.terminals.iter().filter(|(_, t)| t.device() == Some(c)).map(|(k, _)| Obj::Terminal(*k)));
            all.extend(m.representations.iter().filter(|(_, r)| r.subject == Subject::Device(c)).map(|(k, _)| Obj::Representation(*k)));
        }
        Obj::Wire(c) => {
            all.extend(m.cables.iter().filter(|(_, x)| x.cores.iter().any(|mm| mm.wire() == Some(c))).map(|(k, _)| Obj::Cable(*k)));
            all.extend(m.representations.iter().filter(|(_, r)| r.subject == Subject::Wire(c)).map(|(k, _)| Obj::Representation(*k)));
        }
        Obj::Cable(c) => {
            all.extend(m.representations.iter().filter(|(_, r)| r.subject == Subject::Cable(c)).map(|(k, _)| Obj::Representation(*k)));
        }
        Obj::Representation(_) | Obj::LoosePart(_) | Obj::Project => {}
        Obj::Item(i) => {
            all.extend(m.representations.iter().filter(|(_, r)| r.items.contains(&i)).map(|(k, _)| Obj::Representation(*k)));
        }
        Obj::Page(p) => {
            all.extend(m.items.iter().filter(|(_, x)| x.page == p).map(|(k, _)| Obj::Item(*k)));
            all.extend(m.representations.iter().filter(|(_, r)| r.page == p).map(|(k, _)| Obj::Representation(*k)));
            all.extend(m.routes.iter().filter(|(_, r)| r.page == p).map(|(k, _)| Obj::Route(*k)));
        }
        Obj::Layer(l) => all.extend(m.items.iter().filter(|(_, x)| x.style.layer == l).map(|(k, _)| Obj::Item(*k))),
        Obj::Definition(d) => {
            all.extend(m.items.iter().filter(|(_, x)| matches!(&x.kind, ItemKind::Symbol(s) if s.definition == d)).map(|(k, _)| Obj::Item(*k)));
            all.extend(m.devices.iter().filter(|(_, c)| c.definition == Some(d)).map(|(k, _)| Obj::Device(*k)));
        }
        Obj::Folder(f) => {
            all.extend(m.folders.iter().filter(|(_, x)| x.parent == Some(f)).map(|(k, _)| Obj::Folder(*k)));
            all.extend(m.pages.keys().filter(|p| crate::document::book::folder_of(m, **p) == Some(f)).map(|k| Obj::Page(*k)));
        }
    }
    all
}

/// Stop `x`'s use of `y` (it was not brought back): detach, drop the entry, or remove `x`.
fn drop_use(m: &mut Document, x: Obj, y: Obj, how: Use, check: &mut BTreeSet<Obj>) {
    match (x, how) {
        // A wire runs between two connection points: without one it goes.
        (Obj::Wire(_), Use::End(_)) => remove_whole(m, x, check),
        (Obj::Route(r), Use::RouteEnd(i)) => {
            if let Some(e) = m.routes.get_mut(&r).and_then(|r| r.ends.get_mut(i)) {
                *e = RouteEnd::Free;
            }
        }
        (Obj::Representation(r), Use::ConnectionPoint) => {
            if let (Some(r), Obj::Terminal(t)) = (m.representations.get_mut(&r), y) {
                r.connection_points.remove(&t);
            }
        }
        (Obj::Cable(c), Use::Member) => match (m.cables.get_mut(&c), y) {
            (Some(c), Obj::Wire(w)) => off_line(&mut c.cores, |mm| mm.wire() == Some(w)),
            (Some(c), Obj::Route(r)) => off_line(&mut c.cores, |mm| mm.route() == Some(r)),
            _ => {}
        },
        (Obj::Page(p), Use::PageFolder) => {
            if let Some(p) = m.pages.get_mut(&p) {
                p.folder = None;
            }
        }
        (Obj::Folder(f), Use::FolderParent) => {
            if let Some(f) = m.folders.get_mut(&f) {
                f.parent = None;
            }
        }
        (Obj::Device(c), Use::DeviceDefinition) => {
            if let Some(c) = m.devices.get_mut(&c) {
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
            let only_ends = users.iter().all(|(_, u)| matches!(u, Use::ConnectionPoint));
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
fn next_tag(prefix: &str, high: &mut HighWater, style: tag::Style) -> String {
    let n = high.get(prefix).copied().unwrap_or(0).saturating_add(1);
    high.insert(prefix.to_string(), n);
    tag::format(prefix, n, style)
}

/// What holds a tag in the merged state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Holder {
    Device(DeviceId),
    Cable(CableId),
}

impl Holder {
    fn obj(self) -> Obj {
        match self {
            Holder::Device(c) => Obj::Device(c),
            Holder::Cable(c) => Obj::Cable(c),
        }
    }

    fn tag(self, d: &Document) -> Option<&str> {
        match self {
            Holder::Device(c) => d.devices.get(&c).and_then(|x| x.tag()),
            Holder::Cable(c) => d.cables.get(&c).and_then(|x| x.properties.get(tag::TAG_KEY)).map(String::as_str),
        }
    }

    fn set_tag(self, m: &mut Document, t: String) {
        match self {
            Holder::Device(c) => {
                if let Some(x) = m.devices.get_mut(&c) {
                    *x = x.with_tag(Some(&t));
                }
            }
            Holder::Cable(c) => {
                if let Some(x) = m.cables.get_mut(&c) {
                    x.properties.insert(tag::TAG_KEY.into(), t);
                }
            }
        }
    }
}

/// The objects `ops` name (for the merge's scope).
fn objs_of(ops: &[Op]) -> BTreeSet<Obj> {
    let mut out = BTreeSet::new();
    for op in ops {
        match op {
            Op::CreateDevice { id, .. } | Op::DeleteDevice { id, .. } | Op::SetDeviceDefinition { id, .. } => {
                out.insert(Obj::Device(*id));
            }
            Op::AddConnectionPoint { id, terminal } | Op::RemoveConnectionPoint { id, terminal } => {
                out.insert(Obj::Terminal(*id));
                out.insert(owner_obj(terminal.owner));
            }
            Op::LinkRepresentation { id, .. } | Op::UnlinkRepresentation { id, .. } => {
                out.insert(Obj::Representation(*id));
            }
            Op::SetProperty { object: ObjectRef::Device(c), .. } => {
                out.insert(Obj::Device(*c));
            }
            Op::CreateCable { id, .. } | Op::DeleteCable { id, .. } | Op::ModifyCable { id, .. } => {
                out.insert(Obj::Cable(*id));
            }
            _ => {}
        }
    }
    out
}

/// The highest provisional number of `prefix` in `m` (a scan: merges only).
fn top_provisional(m: &Document, prefix: &str) -> u64 {
    let lo = format!("{prefix}?");
    m.devices
        .values()
        .filter_map(|c| c.tag())
        .chain(m.cables.values().filter_map(|c| c.properties.get(tag::TAG_KEY).map(String::as_str)))
        .filter_map(|x| tag::key(x).strip_prefix(&lo).and_then(|n| n.parse::<u64>().ok()))
        .max()
        .unwrap_or(0)
}

/// Two holders with one tag (018: one tag, one device; 026: devices and cables share it) after the
/// merge: a **tag** conflict per tag, renumbering the source's (the default suggestion, owner
/// answer 12) or the target's, or making them one (brief 026). Provisional tags added on both
/// sides are renumbered silently (design 026 §8).
fn resolve_tags(ctx: &mut Ctx<'_, '_>, m: &mut Document, objects: &BTreeSet<Obj>) -> BTreeSet<Obj> {
    let t = ctx.inp.target;
    let mut high = ctx.inp.high.clone();
    let present: Vec<String> = m
        .devices
        .values()
        .filter_map(|c| c.tag())
        .chain(m.cables.values().filter_map(|c| c.properties.get(tag::TAG_KEY).map(String::as_str)))
        .map(str::to_string)
        .collect();
    for x in &present {
        tag::note(&mut high, x);
    }
    // Brief 033: the merged project's tag style (padding, dash, case).
    let style = tag::Style::of(m);
    let mut out = BTreeSet::new();
    let mut done: BTreeSet<String> = BTreeSet::new();
    let ours: Vec<Holder> = objects
        .iter()
        .filter_map(|o| match o {
            Obj::Device(c) => Some(Holder::Device(*c)),
            Obj::Cable(c) => Some(Holder::Cable(*c)),
            _ => None,
        })
        .collect();
    // The merged state with a current index, built once, for the join (brief 026).
    let mut indexed: Option<Document> = None;
    for h in ours.iter().copied() {
        let Some(tag_text) = h.tag(m).map(str::to_string) else { continue };
        // Only a tag the merge brought from the source.
        if h.tag(t) == Some(&tag_text) {
            continue;
        }
        let key = tag::key(&tag_text);
        if done.contains(&key) {
            continue;
        }
        let same = |d: Holder| d != h && d.tag(m).is_some_and(|x| tag::key(x) == key);
        let mut others: Vec<Holder> = t
            .components_tagged(&tag_text)
            .into_iter()
            .map(Holder::Device)
            .chain(t.cables_tagged(&tag_text).into_iter().map(Holder::Cable))
            .filter(|d| same(*d))
            .collect();
        let more: Vec<Holder> = ours.iter().copied().filter(|d| same(*d) && !others.contains(d)).collect();
        others.extend(more);
        let Some(other) = others.first().copied() else { continue };
        done.insert(key.clone());
        // The source's holder and the target's: `h`'s tag came from the source (the loop skips
        // every holder whose tag the target already had), also when the target has `h` itself
        // under an older tag (reviewer 026, 1).
        let (src, tgt) = (h, other);
        // Provisional duplicates were never issued: the source's takes the next one, silently.
        if let Some((p, _)) = tag::provisional(&tag_text) {
            let n = top_provisional(m, &p).saturating_add(1);
            src.set_tag(m, style.provisional(&p, n));
            // The indexed copy is stale now (reviewer 026, 5): rebuilt when next needed.
            indexed = None;
            out.insert(src.obj());
            ctx.provisional += 1;
            continue;
        }
        let prefix = tag::parse(&tag_text).map(|p| p.0);
        let fresh = match &prefix {
            Some(p) => next_tag(p, &mut high, style),
            None => format!("{tag_text}-2"),
        };
        let id = format!("rule/tag/{key}");
        // Make them one: two devices or two cables, never a device and a cable.
        let one: Result<Vec<Op>, String> = match (src, tgt) {
            (Holder::Device(a), Holder::Device(b)) => {
                let d = indexed.get_or_insert_with(|| {
                    let mut x = m.clone();
                    x.rebuild_index();
                    x
                });
                (ctx.inp.join)(d, a, b)
            }
            (Holder::Cable(a), Holder::Cable(b)) => {
                let d = indexed.get_or_insert_with(|| {
                    let mut x = m.clone();
                    x.rebuild_index();
                    x
                });
                join_cables(d, a, b).map_err(String::from)
            }
            _ => Err(format!("{tag_text} is a cable on one side and a device on the other.")),
        };
        let mut choices = vec![Choice::RenumberSource, Choice::RenumberTarget];
        match &one {
            Ok(_) => choices.push(Choice::MakeOne),
            Err(why) => {
                ctx.unavailable.insert(id.clone(), why.clone());
            }
        }
        let noun = match h {
            Holder::Device(_) => "device",
            Holder::Cable(_) => "cable",
        };
        let conflict = Conflict {
            id: id.clone(),
            kind: ConflictKind::Tag,
            object: h.obj(),
            what: format!("{tag_text} · {} and {} each have a {noun} {tag_text}", ctx.t(), ctx.s()),
            target: format!("renumber {}'s to {fresh}", ctx.t()),
            source: format!("renumber {}'s to {fresh}", ctx.s()),
            choices,
            chosen: None,
        };
        let asked = ctx.ask(conflict);
        if asked.is_none()
            && ctx.choices.get(&id) == Some(&Choice::MakeOne)
            && let Some(why) = ctx.unavailable.get(&id).cloned()
        {
            ctx.problems.push(format!("{tag_text}: they cannot be one ({why}) Renumber one of them instead."));
        }
        let which = match asked {
            Some(Choice::RenumberSource) => Some(src),
            Some(Choice::RenumberTarget) => Some(tgt),
            Some(Choice::MakeOne) => {
                if let Ok(ops) = one {
                    for op in &ops {
                        if let Err(e) = op.apply(m) {
                            ctx.problems.push(format!("{tag_text}: they cannot be one ({e})."));
                            break;
                        }
                        if let Some(d) = indexed.as_mut() {
                            let _ = op.apply(d);
                        }
                    }
                    out.extend(objs_of(&ops));
                    out.insert(tgt.obj());
                }
                None
            }
            _ => None,
        };
        if let Some(d) = which {
            d.set_tag(m, fresh);
            out.insert(d.obj());
            indexed = None;
        }
    }
    out
}

/// The ops making cable `from` part of cable `into` (brief 026, Make them one): its members join
/// (after `into`'s own; a wire in both stays once), its drawings show `into`, and it is deleted.
/// `doc` needs a current index (its drawings are found through it). Used by the merge and by
/// `setprop`'s merge choice alike.
///
/// # Errors
/// A missing cable, or `from == into`.
pub fn join_cables(doc: &Document, from: CableId, into: CableId) -> Result<Vec<Op>, crate::ParseError> {
    if from == into {
        return Err(crate::ParseError::invalid(crate::Parsed::CableJoin, "A cable cannot be merged with itself."));
    }
    let a = doc.cables.get(&from).ok_or(crate::ParseError::NotFound(crate::Noun::Cable))?;
    let b = doc.cables.get(&into).ok_or(crate::ParseError::NotFound(crate::Noun::Cable))?;
    let mut ops = Vec::new();
    let mut joined = b.clone();
    for mem in &a.cores {
        // A line in both stays once; an unplaced core joins only when `into` has no core of its label.
        let unplaced_dup = mem.position == CorePosition::Unplaced && b.cores.iter().any(|x| x.label == mem.label);
        if unplaced_dup || (mem.position != CorePosition::Unplaced && b.cores.iter().any(|x| x.position == mem.position)) {
            continue;
        }
        joined.cores.push(mem.clone());
    }
    let mut empty = a.clone();
    empty.cores.clear();
    ops.extend(Op::modify_cable(from, a, empty.clone()));
    ops.extend(Op::modify_cable(into, b, joined));
    for (rid, r) in doc.representations_of(Subject::Cable(from)) {
        ops.push(Op::UnlinkRepresentation { id: rid, rep: r.clone() });
        ops.push(Op::LinkRepresentation { id: rid, rep: crate::document::doc::Representation { subject: Subject::Cable(into), ..r.clone() } });
    }
    ops.push(Op::DeleteCable { id: from, cable: empty });
    Ok(ops)
}

// The next free page number after `n`.
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
    let mut taken: BTreeSet<String> = m.pages.keys().filter_map(|p| crate::document::book::stored_number(m, *p)).map(str::to_string).collect();
    let step = m.settings.page_number_step();
    let ours: Vec<PageId> = objects.iter().filter_map(|o| if let Obj::Page(p) = o { Some(*p) } else { None }).collect();
    for p in ours {
        let Some(num) = crate::document::book::stored_number(m, p).map(str::to_string) else { continue };
        if crate::document::book::stored_number(t, p) == Some(num.as_str()) {
            continue;
        }
        let Some(other) = m.pages.keys().copied().find(|q| *q != p && crate::document::book::stored_number(m, *q) == Some(num.as_str())) else {
            continue;
        };
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
        let src_is_p = s.pages.contains_key(&p) && !t.pages.contains_key(&p) || crate::document::book::stored_number(s, p) == Some(num.as_str());
        let which = match ctx.ask(c) {
            Some(Choice::RenumberSource) => Some(if src_is_p { p } else { other }),
            Some(Choice::RenumberTarget) => Some(if src_is_p { other } else { p }),
            _ => None,
        };
        if let Some(q) = which
            && let Some(page) = m.pages.get_mut(&q)
        {
            page.number = crate::document::book::PageNo::parse(&fresh);
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
                Subject::Device(c) => Obj::Device(c),
                Subject::Wire(c) => Obj::Wire(c),
                Subject::Cable(c) => Obj::Cable(c),
                Subject::Route(c) => Obj::Route(c),
                Subject::Aid(c) => Obj::Aid(c),
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
            // A device's tag and text fields are listed as its keys were (design 072 §2.1).
            Obj::Device(c) => d.devices.get(&c).map(|x| {
                let mut p = x.properties.clone();
                p.extend(text_bag(x));
                p.extend(x.tag().map(|t| (tag::TAG_KEY.to_string(), t.to_string())));
                p
            }),
            Obj::Wire(c) => d.wires.get(&c).map(|x| {
                let mut p = x.display.clone();
                p.extend(x.wire_type.as_ref().map(|t| (crate::connectivity::wiring::WIRE_CATEGORY.to_string(), t.text())));
                p
            }),
            Obj::Cable(c) => d.cables.get(&c).map(|x| {
                let mut p = x.properties.clone();
                p.extend(x.cable_type.as_ref().map(|t| (crate::document::doc::CABLE_TYPE_KEY.to_string(), t.text())));
                p
            }),
            Obj::Page(p) => d.pages.get(&p).map(|x| x.display.clone()),
            Obj::Project => Some(d.display.clone()),
            _ => None,
        }
    };
    if let Obj::Wire(c) = obj
        && let (Some(x), Some(y)) = (a.wires.get(&c), s.wires.get(&c))
    {
        let docs = [s, a];
        for (e, word) in [(End::A, "end A"), (End::B, "end B")] {
            if x.end(e) != y.end(e) {
                let show = |t: ConnectionPointId| name_of(&docs, Obj::Terminal(t));
                return format!("· {word} {} → {}", show(x.end(e)), show(y.end(e)));
            }
        }
        let (vx, vy) = (x.values(), y.values());
        let name = |v: &[(&'static str, String)], k: &str| v.iter().find(|(n, _)| *n == k).map(|(_, t)| t.clone());
        let mut keys: Vec<&'static str> = vx.iter().chain(&vy).map(|(n, _)| *n).collect();
        keys.dedup();
        if let Some(k) = keys.into_iter().find(|k| name(&vx, k) != name(&vy, k)) {
            let q = |t: Option<String>| t.map_or_else(|| "(none)".into(), |t| format!("\"{t}\""));
            return format!("· {k} {} → {}", q(name(&vx, k)), q(name(&vy, k)));
        }
    }
    // Design 072 §3 (FP1): a module moved to another station shows the stations' tags.
    if let Obj::Device(c) = obj
        && let (Some(x), Some(y)) = (a.devices.get(&c), s.devices.get(&c))
        && let (crate::DeviceType::PlcModule(mx), crate::DeviceType::PlcModule(my)) = (&x.device_type, &y.device_type)
        && (mx.rack, mx.slot) != (my.rack, my.slot)
    {
        let docs = [s, a];
        let at = |r: Option<DeviceId>, slot: Option<u16>| match (r, slot) {
            (Some(r), Some(n)) => format!("{} slot {n}", name_of(&docs, Obj::Device(r))),
            (Some(r), None) => name_of(&docs, Obj::Device(r)),
            (None, _) => "no rack".into(),
        };
        return format!("· rack {} → {}", at(mx.rack, mx.slot), at(my.rack, my.slot));
    }
    match (props(a), props(s)) {
        (Some(pa), Some(ps)) => {
            let keys: Vec<&String> = pa
                .keys()
                .chain(ps.keys())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .filter(|k| pa.get(*k) != ps.get(*k))
                .filter(|k| !k.starts_with(tag::HIGH_RESET_PREFIX))
                .collect();
            // `numbering.high.<P>` is internal (brief 026): never listed (reviewer 026, nit).
            match keys.first() {
                Some(k) => {
                    // Brief 045 (FP1): a value naming a component shows its tag.
                    let q = |v: Option<&String>| v.map_or_else(|| "(none)".into(), |v| format!("\"{}\"", value_words(&[s, a], v)));
                    let more = if keys.len() > 1 { format!(" (+{} more)", keys.len() - 1) } else { String::new() };
                    format!("· {} {} → {}{more}", key_words(k), q(pa.get(*k)), q(ps.get(*k)))
                }
                None if obj == Obj::Project => {
                    if a.settings != s.settings {
                        "· settings changed".into()
                    } else if a.standard != s.standard || a.styles != s.styles {
                        "· standard or styles changed".into()
                    } else {
                        String::new()
                    }
                }
                None => "changed".into(),
            }
        }
        _ => "changed".into(),
    }
}

/// A provisional tag never counts as fixed (overseer ruling, tester 026 bug 2): a merge that
/// combines one side's fixed flag with the other side's provisional tag drops the flag.
fn clear_provisional_fixed(m: &mut Document, all: &BTreeSet<Obj>) {
    for o in all {
        if let Obj::Device(i) = *o
            && let Some(c) = m.devices.get_mut(&i)
            && c.tag().is_some_and(tag::is_provisional)
        {
            c.tag_fixed = false;
        }
        let props = match *o {
            Obj::Device(_) => None,
            Obj::Cable(i) => m.cables.get_mut(&i).map(|c| &mut c.properties),
            _ => None,
        };
        let Some(props) = props else { continue };
        if props.get(tag::TAG_KEY).is_some_and(|t| tag::is_provisional(t)) {
            props.remove(tag::FIXED_KEY);
        }
    }
}

/// Work out the merge of `inp` with `choices` (module docs).
pub fn plan(inp: &Inputs<'_>, choices: &BTreeMap<String, Choice>) -> Plan {
    let mut ctx =
        Ctx { inp, choices, conflicts: Vec::new(), seen: BTreeSet::new(), problems: Vec::new(), unavailable: BTreeMap::new(), provisional: 0 };
    let objects = scope_objects(inp.scope);
    let (changes, pages) = changes(inp, &objects);
    let mut m = inp.target.clone();
    merge_objects(&mut ctx, &mut m, &objects);
    let kept_devices = keep_devices(&mut ctx, &mut m);
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
    all.extend(kept_devices);
    let needed = resolve_needed(&mut ctx, &mut m, all.clone());
    all.extend(needed);
    let renamed = resolve_tags(&mut ctx, &mut m, &all);
    all.extend(renamed);
    clear_provisional_fixed(&mut m, &all);
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
    let ignored: Vec<String> = {
        let docs = [inp.target, inp.source, inp.ancestor];
        choices
            .keys()
            .filter(|id| !ctx.seen.contains(*id))
            .map(|id| {
                let what = id.split('/').next().and_then(|h| obj_by_hex(&docs, h)).map_or_else(|| id.clone(), |o| name_of(&docs, o));
                format!("{what}: the answer has no effect (that conflict is not raised with the other answers)")
            })
            .collect()
    };
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
                Obj::Device(i) => {
                    scope.devices.insert(i);
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
                Obj::Wire(i) => {
                    scope.wires.insert(i);
                }
                Obj::Cable(i) => {
                    scope.cables.insert(i);
                }
                Obj::Route(i) => {
                    scope.routes.insert(i);
                }
                Obj::Aid(i) => {
                    scope.aids.insert(i);
                }
                Obj::LoosePart(i) => {
                    scope.loose_parts.insert(i);
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
    Plan {
        changes,
        pages,
        conflicts: ctx.conflicts,
        unanswered,
        ops,
        problem,
        ignored,
        unavailable: ctx.unavailable,
        provisional_renumbered: ctx.provisional,
    }
}

/// The objects `ops` name.
pub fn objects_of(ops: &[Op]) -> BTreeSet<Obj> {
    let mut s = Scope::default();
    s.add_ops(ops);
    let t = &s.touched;
    let mut out = BTreeSet::new();
    out.extend(t.pages.iter().chain(&t.page_properties).map(|x| Obj::Page(*x)));
    out.extend(t.devices.iter().map(|x| Obj::Device(*x)));
    out.extend(t.wires.iter().map(|x| Obj::Wire(*x)));
    out.extend(t.cables.iter().map(|x| Obj::Cable(*x)));
    out.extend(t.representations.iter().map(|x| Obj::Representation(*x)));
    out.extend(t.items.iter().map(|x| Obj::Item(*x)));
    out.extend(t.terminals.iter().map(|x| Obj::Terminal(*x)));
    out.extend(t.folders.iter().map(|x| Obj::Folder(*x)));
    if s.project {
        out.insert(Obj::Project);
    }
    out
}

/// A conflict kind's word in the merge preview's JSON (design 020 §10).
pub fn kind_word(k: ConflictKind) -> &'static str {
    match k {
        ConflictKind::Value => "value",
        ConflictKind::Removed => "removed",
        ConflictKind::Needed => "needed",
        ConflictKind::Tag => "tag",
        ConflictKind::PageNumber => "page_number",
        ConflictKind::FolderName => "folder_name",
        ConflictKind::Order => "order",
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::naming::id::{ConductorId, NetId};

    fn doc_with(tags: &[(u128, &str)]) -> Document {
        let mut d = Document::default();
        for (id, t) in tags {
            d.devices.insert(DeviceId(*id), Device::default().with_tag(Some(t)));
        }
        d.rebuild_index();
        d
    }

    fn run(a: &Document, t: &Document, s: &Document, scope: &Scope, choices: &BTreeMap<String, Choice>) -> Plan {
        let all = |_: &str| true;
        let high = HighWater::new();
        let rule = |_: Obj, _: &str, v: &str| if v.trim().is_empty() { Err("empty".to_string()) } else { Ok(v.trim().to_string()) };
        // Two devices described "coil" cannot be one (a stand-in for 018's rules).
        let join = |d: &Document, from: DeviceId, into: DeviceId| -> Result<Vec<Op>, String> {
            let coil = |c: DeviceId| d.devices.get(&c).and_then(|x| x.description.as_ref()).is_some_and(|v| v.as_str() == "coil");
            if coil(from) && coil(into) {
                return Err("already has a coil.".into());
            }
            let comp = d.devices.get(&from).cloned().ok_or("no such device")?;
            Ok(vec![Op::DeleteDevice { id: from, device: comp }])
        };
        let inp = Inputs {
            ancestor: a,
            target: t,
            source: s,
            scope,
            target_name: "main",
            source_name: "\"B\"",
            keep_project_key: &all,
            high: &high,
            edit_rule: &rule,
            join: &join,
        };
        plan(&inp, choices)
    }

    fn scope_of(ids: &[u128]) -> Scope {
        let mut s = Scope::default();
        for i in ids {
            s.touched.devices.insert(DeviceId(*i));
        }
        s
    }

    #[test]
    fn one_sided_changes_merge_and_two_sided_ones_conflict() {
        let a = doc_with(&[(1, "-K01"), (2, "-K02")]);
        let mut t = a.clone();
        t.devices.get_mut(&DeviceId(1)).unwrap().properties.insert("remark".into(), "16 A".into());
        let mut s = a.clone();
        s.devices.get_mut(&DeviceId(1)).unwrap().properties.insert("remark".into(), "20 A".into());
        s.devices.get_mut(&DeviceId(2)).unwrap().description = crate::objects::devices::Description::new("Pump").ok();
        let scope = scope_of(&[1, 2]);
        let p = run(&a, &t, &s, &scope, &BTreeMap::new());
        assert_eq!(p.conflicts.len(), 1);
        let id = p.conflicts[0].id.clone();
        assert!(id.ends_with("/prop.remark"), "{id}");
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
        assert_eq!(doc.devices[&DeviceId(1)].properties["remark"], "20 A");
        assert_eq!(doc.devices[&DeviceId(2)].text(crate::TextField::Description), Some("Pump"));
        ch.insert(id, Choice::Value("25 A".into()));
        let p = run(&a, &t, &s, &scope, &ch);
        let mut doc = t.clone();
        for op in &p.ops {
            op.apply(&mut doc).unwrap();
        }
        assert_eq!(doc.devices[&DeviceId(1)].properties["remark"], "25 A");
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
        assert_eq!(doc.devices[&DeviceId(1)].tag().unwrap(), "-K05");
        assert_eq!(doc.devices[&DeviceId(2)].tag().unwrap(), "-K06");
    }

    #[test]
    fn a_tag_added_on_both_sides_can_be_made_one() {
        let a = doc_with(&[]);
        let t = doc_with(&[(1, "-K05")]);
        let s = doc_with(&[(2, "-K05")]);
        let scope = scope_of(&[1, 2]);
        let p = run(&a, &t, &s, &scope, &BTreeMap::new());
        let c = p.conflicts.iter().find(|c| c.kind == ConflictKind::Tag).expect("a tag conflict");
        assert!(c.choices.contains(&Choice::MakeOne));
        let p = run(&a, &t, &s, &scope, &BTreeMap::from([(c.id.clone(), Choice::MakeOne)]));
        assert!(p.problem.is_none() && p.unanswered.is_empty(), "{:?}", p.problem);
        let mut doc = t.clone();
        for op in &p.ops {
            op.apply(&mut doc).unwrap();
        }
        assert!(doc.devices.contains_key(&DeviceId(1)) && !doc.devices.contains_key(&DeviceId(2)));
        // Two coils cannot be one: the choice is left out, with the reason; renumbering still works.
        let mut t2 = doc_with(&[(1, "-K05")]);
        let mut s2 = doc_with(&[(2, "-K05")]);
        for (d, id) in [(&mut t2, 1), (&mut s2, 2)] {
            d.devices.get_mut(&DeviceId(id)).unwrap().description = crate::objects::devices::Description::new("coil").ok();
        }
        let p = run(&a, &t2, &s2, &scope, &BTreeMap::new());
        let c = p.conflicts.iter().find(|c| c.kind == ConflictKind::Tag).unwrap();
        assert!(!c.choices.contains(&Choice::MakeOne));
        assert_eq!(p.unavailable.get(&c.id).map(String::as_str), Some("already has a coil."));
        let p = run(&a, &t2, &s2, &scope, &BTreeMap::from([(c.id.clone(), Choice::MakeOne)]));
        assert!(p.ops.is_empty() && p.problem.is_some());
        let p = run(&a, &t2, &s2, &scope, &BTreeMap::from([(c.id.clone(), Choice::RenumberSource)]));
        let mut doc = t2.clone();
        for op in &p.ops {
            op.apply(&mut doc).unwrap();
        }
        assert_eq!(doc.devices[&DeviceId(2)].tag().unwrap(), "-K06");
    }

    #[test]
    fn provisional_duplicates_are_not_conflicts() {
        let a = doc_with(&[]);
        let t = doc_with(&[(1, "-K?1")]);
        let s = doc_with(&[(2, "-K?1")]);
        let p = run(&a, &t, &s, &scope_of(&[1, 2]), &BTreeMap::new());
        assert!(p.conflicts.is_empty(), "{:?}", p.conflicts);
        assert_eq!(p.provisional_renumbered, 1);
        let mut doc = t.clone();
        for op in &p.ops {
            op.apply(&mut doc).unwrap();
        }
        assert_eq!(doc.devices[&DeviceId(2)].tag().unwrap(), "-K?2");
    }

    #[test]
    fn removed_on_one_side_and_changed_on_the_other_offers_both() {
        let a = doc_with(&[(1, "-K01")]);
        let t = doc_with(&[]);
        let mut s = a.clone();
        s.devices.get_mut(&DeviceId(1)).unwrap().description = crate::objects::devices::Description::new("x").ok();
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
        assert!(doc.devices.contains_key(&DeviceId(1)));
    }

    // ---- each conflict kind with each of its answers (reviewer 020b, finding 10)

    fn applied(t: &Document, p: &Plan) -> Document {
        assert!(p.problem.is_none() && p.unanswered.is_empty(), "{:?} {:?}", p.problem, p.unanswered);
        let mut doc = t.clone();
        for op in &p.ops {
            op.apply(&mut doc).unwrap();
        }
        doc
    }

    fn one(id: &str, c: Choice) -> BTreeMap<String, Choice> {
        BTreeMap::from([(id.to_string(), c)])
    }

    fn device(d: &mut Document, c: u128, tag_text: &str, terminals: &[u128]) {
        d.devices.insert(DeviceId(c), Device::default().with_tag(Some(tag_text)));
        for t in terminals {
            d.terminals.insert(
                ConnectionPointId(*t),
                ConnectionPoint { owner: DeviceId(c).into(), label: Some(format!("A{t}")), key: None, accepts: Default::default() },
            );
        }
    }

    fn wire(d: &mut Document, w: u128, a: u128, b: u128) {
        d.wires.insert(WireId(w), Wire::new(NetId(90), ConductorId(91), ConnectionPointId(a), ConnectionPointId(b)));
    }

    fn scope_all(ids: &[Obj]) -> Scope {
        let mut s = Scope::default();
        for o in ids {
            match *o {
                Obj::Device(c) => s.touched.devices.insert(c),
                Obj::Terminal(t) => s.touched.terminals.insert(t),
                Obj::Wire(c) => s.touched.wires.insert(c),
                Obj::Page(p) => s.touched.pages.insert(p),
                _ => false,
            };
        }
        s
    }

    #[test]
    fn a_value_conflict_keeps_main_and_an_empty_or_refused_edit_is_a_problem() {
        let a = doc_with(&[(1, "-K01")]);
        let mut t = a.clone();
        t.devices.get_mut(&DeviceId(1)).unwrap().properties.insert("remark".into(), "16 A".into());
        let mut s = a.clone();
        s.devices.get_mut(&DeviceId(1)).unwrap().properties.insert("remark".into(), "20 A".into());
        let scope = scope_of(&[1]);
        let id = format!("{}/prop.remark", DeviceId(1).hex());
        let p = run(&a, &t, &s, &scope, &one(&id, Choice::Target));
        assert_eq!(applied(&t, &p).devices[&DeviceId(1)].properties["remark"], "16 A");
        let p = run(&a, &t, &s, &scope, &one(&id, Choice::Value("  ".into())));
        assert!(p.problem.as_deref().is_some_and(|m| m.to_lowercase().contains("remark")), "{:?}", p.problem);
        assert!(p.ops.is_empty());
    }

    #[test]
    fn removed_on_main_and_kept_from_the_branch_brings_back_its_connection_points() {
        let mut a = Document::default();
        device(&mut a, 1, "-K01", &[11, 12]);
        let t = Document::default();
        let mut s = a.clone();
        s.devices.get_mut(&DeviceId(1)).unwrap().description = crate::objects::devices::Description::new("Pump").ok();
        let scope = scope_all(&[Obj::Device(DeviceId(1)), Obj::Terminal(ConnectionPointId(11)), Obj::Terminal(ConnectionPointId(12))]);
        let p = run(&a, &t, &s, &scope, &BTreeMap::new());
        let k = p.conflicts.iter().find(|c| c.kind == ConflictKind::Removed).unwrap();
        assert!(k.source.contains("connection points"), "{}", k.source);
        let id = k.id.clone();
        // Keep removed: nothing comes back.
        let doc = applied(&t, &run(&a, &t, &s, &scope, &one(&id, Choice::Target)));
        assert!(doc.devices.is_empty() && doc.terminals.is_empty());
        // Keep the device: its connection points come back with it.
        let doc = applied(&t, &run(&a, &t, &s, &scope, &one(&id, Choice::Source)));
        assert!(doc.devices.contains_key(&DeviceId(1)));
        assert_eq!(doc.terminals.len(), 2);
    }

    #[test]
    fn a_wire_ends_conflict_offers_main_and_branch() {
        let mut a = Document::default();
        device(&mut a, 1, "-K01", &[11, 12, 13, 14]);
        wire(&mut a, 5, 11, 12);
        let mut t = a.clone();
        wire(&mut t, 5, 11, 13);
        let mut s = a.clone();
        wire(&mut s, 5, 11, 14);
        let scope = scope_all(&[Obj::Wire(WireId(5))]);
        let id = format!("{}/ends", WireId(5).hex());
        let p = run(&a, &t, &s, &scope, &BTreeMap::new());
        assert_eq!(p.conflicts[0].id, id);
        assert_eq!(p.conflicts[0].choices, vec![Choice::Target, Choice::Source]);
        for (c, want) in [(Choice::Target, 13), (Choice::Source, 14)] {
            let doc = applied(&t, &run(&a, &t, &s, &scope, &one(&id, c.clone())));
            assert_eq!(doc.wires[&WireId(5)].end(End::B), ConnectionPointId(want), "{c:?}");
        }
    }

    #[test]
    fn a_point_the_branch_uses_and_main_removed_is_brought_back_or_its_wire_removed() {
        let mut a = Document::default();
        device(&mut a, 1, "-K01", &[11]);
        device(&mut a, 2, "-K02", &[21]);
        let mut s = a.clone();
        wire(&mut s, 5, 21, 11);
        let mut t = a.clone();
        t.terminals.remove(&ConnectionPointId(11));
        t.devices.remove(&DeviceId(1));
        let scope = scope_all(&[Obj::Wire(WireId(5)), Obj::Terminal(ConnectionPointId(11)), Obj::Device(DeviceId(1))]);
        let tid = format!("{}/needed", ConnectionPointId(11).hex());
        let cid = format!("{}/needed", DeviceId(1).hex());
        // Not brought back: a wire runs between two points, so without one it goes.
        let doc = applied(&t, &run(&a, &t, &s, &scope, &one(&tid, Choice::Drop)));
        assert!(!doc.wires.contains_key(&WireId(5)));
        assert!(!doc.terminals.contains_key(&ConnectionPointId(11)));
        // Bring back the point and its device.
        let both = BTreeMap::from([(tid.clone(), Choice::BringBack), (cid.clone(), Choice::BringBack)]);
        let doc = applied(&t, &run(&a, &t, &s, &scope, &both));
        assert!(doc.terminals.contains_key(&ConnectionPointId(11)) && doc.devices.contains_key(&DeviceId(1)));
        // Bring back the point but not what it needs: a problem with the way through, no ops.
        let contradiction = BTreeMap::from([(tid, Choice::BringBack), (cid, Choice::Drop)]);
        let p = run(&a, &t, &s, &scope, &contradiction);
        assert!(p.problem.as_deref().is_some_and(|m| m.contains("first")), "{:?}", p.problem);
        assert!(p.ops.is_empty());
    }

    #[test]
    fn a_duplicate_tag_renumbers_main_s_device_when_asked() {
        let a = doc_with(&[]);
        let t = doc_with(&[(1, "-K05")]);
        let s = doc_with(&[(2, "-K05")]);
        let p = run(&a, &t, &s, &scope_of(&[1, 2]), &one("rule/tag/K5", Choice::RenumberTarget));
        let doc = applied(&t, &p);
        assert_eq!(doc.devices[&DeviceId(1)].tag().unwrap(), "-K06");
        assert_eq!(doc.devices[&DeviceId(2)].tag().unwrap(), "-K05");
    }

    #[test]
    fn a_page_number_added_on_both_is_renumbered_either_way() {
        let page = |n: &str| {
            let mut p = Page::titled("x").unwrap();
            p.number = crate::document::book::PageNo::parse(n);
            p
        };
        let a = Document::default();
        let mut t = a.clone();
        t.pages.insert(PageId(1), page("5"));
        t.page_order.push(PageId(1));
        let mut s = a.clone();
        s.pages.insert(PageId(2), page("5"));
        s.page_order.push(PageId(2));
        let mut scope = scope_all(&[Obj::Page(PageId(1)), Obj::Page(PageId(2))]);
        scope.touched.page_order = true;
        let p = run(&a, &t, &s, &scope, &BTreeMap::new());
        let id = p.conflicts.iter().find(|c| c.kind == ConflictKind::PageNumber).map(|c| c.id.clone()).unwrap();
        for (c, renumbered) in [(Choice::RenumberSource, PageId(2)), (Choice::RenumberTarget, PageId(1))] {
            let doc = applied(&t, &run(&a, &t, &s, &scope, &one(&id, c)));
            assert_eq!(doc.pages[&renumbered].number.as_ref().map(crate::document::book::PageNo::as_str), Some("6"));
            assert_eq!(doc.page_order.len(), 2);
        }
    }

    #[test]
    fn page_order_changed_differently_on_both_sides_is_chosen() {
        let mut a = Document::default();
        for i in 1..=3u128 {
            a.pages.insert(PageId(i), Page::titled(format!("{i}")).unwrap());
            a.page_order.push(PageId(i));
        }
        let mut t = a.clone();
        t.page_order = vec![PageId(3), PageId(1), PageId(2)];
        let mut s = a.clone();
        s.page_order = vec![PageId(2), PageId(1), PageId(3)];
        let mut scope = Scope::default();
        scope.touched.page_order = true;
        for (c, want) in [(Choice::Target, t.page_order.clone()), (Choice::Source, s.page_order.clone())] {
            let doc = applied(&t, &run(&a, &t, &s, &scope, &one("rule/pageorder", c)));
            assert_eq!(doc.page_order, want);
        }
    }

    #[test]
    fn an_answer_for_a_conflict_not_raised_is_reported_as_ignored() {
        let a = doc_with(&[(1, "-K01")]);
        let mut s = a.clone();
        s.devices.get_mut(&DeviceId(1)).unwrap().description = crate::objects::devices::Description::new("Pump").ok();
        let p = run(&a, &a, &s, &scope_of(&[1]), &one(&format!("{}/prop.rating", DeviceId(1).hex()), Choice::Source));
        assert_eq!(p.ignored.len(), 1, "{:?}", p.ignored);
        assert!(p.ignored[0].starts_with("-K01"), "{:?}", p.ignored);
        assert!(!p.ops.is_empty(), "the merge itself goes ahead");
    }
}
