//! Deterministic synthetic projects (brief 012 §2).
//!
//! # Semantics
//! - Every page holds the same pattern: `components / pages` coil symbols in a row, each with
//!   terminals A1 (top) and A2 (bottom); `wires / pages` wires, wire `j` running from A2 of
//!   component `j mod n` to A1 of component `(j + 1) mod n`, attached at both ends, so parallel
//!   wires share a pair of terminals and each pair is one net; `cables / pages` cables, cable `c`
//!   taking every wire of pair `c` with core labels `1, 2, …` and a slash across them.
//! - The first cable of each page except the last is continued on the next page as brief 010's
//!   `cableref` and `cablecontinue` record it: a reference end on each page (an ordinary drawn
//!   representation of the cable with role [`REFERENCE_ROLE`] and one Point item), one stub per
//!   member on the next page (a second route of the same conductor) and a slash there.
//! - Only existing ops are used, shaped as the commands record them (`place`, `wire`,
//!   `cableslash`, `newpage`, `cableref`, `cablecontinue`, `move`, `undo`, `redo`; a page rename is
//!   recorded as a page removed and added again, the only way an existing op can rename one).
//! - **Lean**: one transaction per object. **Churned**: the same final document, each object step
//!   followed by an undo, a redo and an edit made and reverted (a move or a rename), so the log
//!   is about five times longer.
//! - IDs come from fixed seeds, no clock is read and the file header records no creation time,
//!   so the same tier gives byte-identical files.
//!
//! The ops are applied once, in order, without the per-transaction invariant check (generation
//! is not what is measured); opening the file replays and checks every transaction.
//!
//! This module does not use CADCraft.

use std::collections::BTreeMap;

use ecad_model::{
    Author, Cable, CableId, CableMember, Color, Component, ComponentId, Conductor, ConductorId, Definition,
    DefinitionId, Document, End, Form, IdAllocator, Item, ItemId, ItemKind, Layer, LayerId, Ledger, LengthUnit,
    ObjectRef, Op, Page, PageId, Placement, Primitive, ProjectId, ProjectSettings, Pt, Representation,
    RepresentationId, Style, Subject, SymbolInstance, Terminal, TerminalDef, TerminalId, Transaction, TxId, TxKind,
};
use ecad_store::{Medium, MemoryMedium};

/// The role brief 010 gives a cable reference end: its own constant, so the generated references
/// are the ones `ecad_model::derive::cable_references` derives.
pub use ecad_model::derive::REFERENCE_ROLE;
/// Seed of every ID the generator allocates.
pub const SEED: u64 = 0x0ECA_D012;
/// What the file header names as its writer.
pub const WRITER: &str = "ecad-bench (synthetic project)";

/// Horizontal distance between components on a page.
const PITCH: f64 = 40.0;
/// The row the components sit on.
const ROW_Y: f64 = 100.0;
/// Distance between parallel wires of one pair.
const LANE: f64 = 2.0;
/// Most generated objects of one kind, so a hand-made tier cannot ask for unbounded memory.
const MAX_OBJECTS: usize = 1_000_000;

/// A project size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tier {
    pub name: &'static str,
    pub pages: usize,
    pub components: usize,
    pub wires: usize,
    pub cables: usize,
}

/// The three measured tiers (brief 012 §2).
pub const TIERS: [Tier; 3] = [
    Tier { name: "small", pages: 10, components: 100, wires: 500, cables: 50 },
    Tier { name: "medium", pages: 100, components: 1_000, wires: 5_000, cables: 500 },
    Tier { name: "large", pages: 500, components: 5_000, wires: 25_000, cables: 2_500 },
];

/// The tier called `name`.
pub fn tier(name: &str) -> Option<Tier> {
    TIERS.iter().copied().find(|t| t.name == name)
}

/// How much editing history a project carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Lean,
    Churned,
}

impl Variant {
    pub const BOTH: [Variant; 2] = [Variant::Lean, Variant::Churned];

    pub fn name(self) -> &'static str {
        match self {
            Variant::Lean => "lean",
            Variant::Churned => "churned",
        }
    }
}

/// A net as the generator intends it: terminals and conductors, each sorted (as
/// `ecad_model::derive::nets` reports them).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntendedNet {
    pub terminals: Vec<TerminalId>,
    pub conductors: Vec<ConductorId>,
}

/// A cable continued from one page on the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Continuation {
    pub cable: CableId,
    pub from: PageId,
    pub to: PageId,
    /// The reference end on `from` and on `to`.
    pub references: [RepresentationId; 2],
}

/// What the generator built and meant.
#[derive(Debug, Clone)]
pub struct Generated {
    pub tier: Tier,
    pub variant: Variant,
    pub project: ProjectId,
    pub ledger: Ledger,
    /// Pages in order.
    pub pages: Vec<PageId>,
    /// Nets in the order `ecad_model::derive::nets` reports them.
    pub nets: Vec<IntendedNet>,
    pub continuations: Vec<Continuation>,
}

impl Generated {
    /// The project file's bytes.
    ///
    /// # Errors
    /// Encoding failed.
    pub fn file_bytes(&self) -> Result<Vec<u8>, String> {
        let file = ecad_file::ProjectFile::write_new(MemoryMedium::default(), self.project, &self.ledger, None, WRITER)
            .map_err(|e| format!("writing the {} {} project: {e}", self.tier.name, self.variant.name()))?;
        file.into_medium().read_all().map_err(|e| e.to_string())
    }
}

/// A two-terminal coil, 10 × 20, terminals A1 (top) and A2 (bottom): the same shape as the
/// trial's built-in experiment definition.
fn coil() -> Definition {
    let r = [(-5.0, -10.0), (5.0, -10.0), (5.0, 10.0), (-5.0, 10.0)];
    Definition {
        name: "Coil (bench)".into(),
        origin: None,
        graphics: vec![Primitive::Polyline {
            vertices: r.iter().map(|(x, y)| Pt::new(*x, *y)).collect(),
            closed: true,
        }],
        terminals: vec![
            TerminalDef { key: "A1".into(), label: Some("A1".into()), at: Pt::new(0.0, 10.0) },
            TerminalDef { key: "A2".into(), label: Some("A2".into()), at: Pt::new(0.0, -10.0) },
        ],
    }
}

/// The log being built and the document it leads to.
struct Builder {
    doc: Document,
    log: Vec<Transaction>,
    tx_ids: IdAllocator,
    churned: bool,
}

impl Builder {
    fn push(&mut self, label: &str, kind: TxKind, ops: Vec<Op>) -> Result<TxId, String> {
        for op in &ops {
            op.apply(&mut self.doc).map_err(|e| format!("generator step {label:?}: {e}"))?;
        }
        let id: TxId = self.tx_ids.fresh();
        let seq = self.log.len() as u64 + 1;
        self.log.push(Transaction { id, seq, author: Author::User, label: label.into(), kind, time_ms: None, ops });
        Ok(id)
    }

    /// One object step. Churned: then undo it, redo it, and make and revert `churn` (labelled
    /// `churn_label`), all as separate transactions.
    fn step(&mut self, label: &str, ops: Vec<Op>, churn_label: &str, churn: Vec<Op>) -> Result<(), String> {
        let inverse: Vec<Op> = ops.iter().rev().map(Op::inverse).collect();
        let edit = self.push(label, TxKind::Edit, ops.clone())?;
        if !self.churned {
            return Ok(());
        }
        let undo = self.push(label, TxKind::Undo { of: edit }, inverse)?;
        self.push(label, TxKind::Redo { of: undo }, ops)?;
        if !churn.is_empty() {
            let revert: Vec<Op> = churn.iter().rev().map(Op::inverse).collect();
            self.push(churn_label, TxKind::Edit, churn)?;
            self.push(churn_label, TxKind::Edit, revert)?;
        }
        Ok(())
    }
}

/// An edit that moves item `id` by `dx` (a symbol's origin, or every point of a primitive).
fn moved(id: ItemId, before: Item, dx: f64) -> Op {
    let mut after = before.clone();
    let shift = |p: &mut Pt| p.x += dx;
    match &mut after.kind {
        ItemKind::Symbol(s) => shift(&mut s.placement.origin),
        ItemKind::Primitive(Primitive::Point { at }) => shift(at),
        ItemKind::Primitive(Primitive::Line { a, b }) => {
            shift(a);
            shift(b);
        }
        ItemKind::Primitive(Primitive::Polyline { vertices, .. }) => vertices.iter_mut().for_each(shift),
        ItemKind::Primitive(_) => {}
    }
    Op::ModifyItem { id, before, after }
}

fn renamed(object: ObjectRef, tag: &str) -> Op {
    Op::SetProperty { object, key: "tag".into(), before: Some(tag.into()), after: Some(format!("{tag} (old name)")) }
}

fn drawn(subject: Subject, page: PageId, role: &str, items: Vec<ItemId>) -> Representation {
    Representation { subject, page, form: Form::Drawn, role: role.into(), items, connection_points: BTreeMap::new() }
}

/// The ops of one wire as `wire` records it: a conductor tagged `tag`, its route on `page`
/// through `vertices`, and its ends attached to `ends` (A, then B). Returns the new conductor.
pub fn wire_ops(
    ids: &mut IdAllocator,
    page: PageId,
    style: &Style,
    vertices: Vec<Pt>,
    ends: [TerminalId; 2],
    tag: &str,
) -> (ConductorId, Vec<Op>) {
    let (w, item, rep): (ConductorId, ItemId, RepresentationId) = (ids.fresh(), ids.fresh(), ids.fresh());
    let mut conductor = Conductor::default();
    conductor.properties.insert("tag".into(), tag.into());
    let [a, b] = ends;
    let ops = vec![
        Op::CreateConductor { id: w, conductor },
        Op::AddItem {
            id: item,
            item: Item {
                page,
                style: style.clone(),
                kind: ItemKind::Primitive(Primitive::Polyline { vertices, closed: false }),
            },
        },
        Op::LinkRepresentation { id: rep, rep: drawn(Subject::Conductor(w), page, "route", vec![item]) },
        Op::AttachEnd { conductor: w, end: End::A, before: None, after: Some(a) },
        Op::AttachEnd { conductor: w, end: End::B, before: None, after: Some(b) },
    ];
    (w, ops)
}

/// The add-wire transaction the edit timing commits on an opened project: a wire on the first
/// page between the first two distinct terminals shown there, on layer `0`. `ids` must be a
/// stream the project has not used.
///
/// # Errors
/// The project has no page, no layer `0`, or fewer than two terminals on its first page.
pub fn added_wire(doc: &Document, ids: &mut IdAllocator, tag: &str) -> Result<Vec<Op>, String> {
    let page = *doc.page_order.first().ok_or("the project has no page")?;
    let layer =
        doc.layers.iter().find(|(_, l)| l.name == "0").map(|(id, _)| *id).ok_or("the project has no layer 0")?;
    let shown = ecad_model::derive::terminal_positions(doc, page);
    let (t1, _, p1) = *shown.first().ok_or("no terminal on the first page")?;
    let (t2, _, p2) = *shown.iter().find(|(t, _, _)| *t != t1).ok_or("fewer than two terminals on the first page")?;
    let style = Style { layer, color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 };
    let bend = Pt::new(p1.x, p2.y);
    Ok(wire_ops(ids, page, &style, vec![p1, bend, p2], [t1, t2], tag).1)
}

/// Generate `tier` with the history of `variant`.
///
/// # Errors
/// A tier whose shape the pattern cannot hold (no pages, more cables than components per page,
/// counts that do not divide by the pages, or more than a million objects of a kind), or an
/// internal inconsistency (reported, never a panic).
pub fn generate(tier: Tier, variant: Variant) -> Result<Generated, String> {
    let p_count = tier.pages;
    let shape_err = || {
        format!(
            "tier {}: pages must be at least 1, every count must divide by the pages, cables per page may not \
             exceed components per page, and no count may exceed {MAX_OBJECTS}",
            tier.name
        )
    };
    let counts = [tier.pages, tier.components, tier.wires, tier.cables];
    if p_count == 0
        || counts.iter().any(|c| *c > MAX_OBJECTS || c % p_count != 0)
        || tier.cables / p_count > tier.components / p_count
        || (tier.wires > 0 && tier.components == 0)
    {
        return Err(shape_err());
    }
    let (n, m, k) = (tier.components / p_count, tier.wires / p_count, tier.cables / p_count);

    let mut ids = IdAllocator::seeded(SEED);
    let mut b = Builder {
        doc: Document::default(),
        log: Vec::new(),
        tx_ids: IdAllocator::seeded(SEED.rotate_left(32)),
        churned: variant == Variant::Churned,
    };
    let project: ProjectId = ids.fresh();
    let (first_page, layer): (PageId, LayerId) = (ids.fresh(), ids.fresh());
    // As `Project::create` records a new project.
    b.push(
        "New project",
        TxKind::Edit,
        vec![
            Op::AddPage { id: first_page, page: Page { title: "1".into() }, index: 0 },
            Op::AddLayer { id: layer, layer: Layer { name: "0".into() } },
            Op::SetProjectSettings {
                before: ProjectSettings::default(),
                after: ProjectSettings { length_unit: LengthUnit::Millimetre },
            },
        ],
    )?;
    let style = Style { layer, color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 };
    let definition: DefinitionId = ids.fresh();
    let def = coil();

    let mut pages = Vec::with_capacity(p_count);
    let mut nets = Vec::new();
    let mut continuations = Vec::new();
    // The first cable of the previous page and its members, to continue on this page.
    let mut previous: Option<(PageId, CableId, Vec<ConductorId>, Pt)> = None;
    let (mut comp_no, mut wire_no, mut cable_no) = (0usize, 0usize, 0usize);

    for p in 0..p_count {
        let page = if p == 0 {
            first_page
        } else {
            let id: PageId = ids.fresh();
            let title = (p + 1).to_string();
            let renamed_page = Page { title: format!("{title} (draft)") };
            b.step(
                "New page",
                vec![Op::AddPage { id, page: Page { title: title.clone() }, index: p }],
                "Rename page",
                vec![
                    Op::RemovePage { id, page: Page { title }, index: p },
                    Op::AddPage { id, page: renamed_page, index: p },
                ],
            )?;
            id
        };
        pages.push(page);

        // Components (`place`): terminals A1 and A2 of each, in placement order.
        let mut terminals: Vec<(TerminalId, TerminalId, Pt)> = Vec::with_capacity(n);
        let mut comp_ids: Vec<ComponentId> = Vec::with_capacity(n);
        for i in 0..n {
            comp_no += 1;
            let (c, item, rep): (ComponentId, ItemId, RepresentationId) = (ids.fresh(), ids.fresh(), ids.fresh());
            let mut ops = Vec::new();
            if comp_no == 1 {
                ops.push(Op::EmbedDefinition { id: definition, definition: def.clone() });
            }
            let mut component = Component { definition: Some(definition), ..Component::default() };
            component.properties.insert("tag".into(), format!("-K{comp_no}"));
            ops.push(Op::CreateComponent { id: c, component });
            let mut keyed = Vec::new();
            for td in &def.terminals {
                let t: TerminalId = ids.fresh();
                ops.push(Op::AddTerminal {
                    id: t,
                    terminal: Terminal { component: c, label: td.label.clone(), key: Some(td.key.clone()) },
                });
                keyed.push(t);
            }
            let origin = Pt::new(PITCH * i as f64 + PITCH / 2.0, ROW_Y);
            let placement = Placement { origin, ..Placement::default() };
            let symbol =
                Item { page, style: style.clone(), kind: ItemKind::Symbol(SymbolInstance { definition, placement }) };
            ops.push(Op::AddItem { id: item, item: symbol.clone() });
            ops.push(Op::LinkRepresentation {
                id: rep,
                rep: Representation {
                    subject: Subject::Component(c),
                    page,
                    form: Form::Symbol,
                    role: "main".into(),
                    items: vec![item],
                    connection_points: BTreeMap::new(),
                },
            });
            b.step("Place symbol", ops, "Move", vec![moved(item, symbol, 10.0)])?;
            let (Some(a1), Some(a2)) = (keyed.first().copied(), keyed.get(1).copied()) else {
                return Err("generator: the coil has two terminals".into());
            };
            terminals.push((a1, a2, origin));
            comp_ids.push(c);
        }

        // Wires (`wire`), attached at both ends; pair `j mod n`.
        let mut pair_wires: Vec<Vec<ConductorId>> = vec![Vec::new(); n];
        let mut pair_lows: Vec<f64> = vec![0.0; n];
        for j in 0..m {
            wire_no += 1;
            let pair = j % n;
            let lane = (j / n) as f64;
            let (Some((_, from_t, from_at)), Some((to_t, _, to_at))) =
                (terminals.get(pair).copied(), terminals.get((pair + 1) % n).copied())
            else {
                return Err("generator: a wire's component is missing".into());
            };
            let a = Pt::new(from_at.x, from_at.y - 10.0);
            let z = Pt::new(to_at.x, to_at.y + 10.0);
            let low = ROW_Y - 20.0 - LANE * lane;
            let high = ROW_Y + 20.0 + LANE * lane;
            let mid = a.x + PITCH / 2.0;
            let vertices = vec![a, Pt::new(a.x, low), Pt::new(mid, low), Pt::new(mid, high), Pt::new(z.x, high), z];
            let tag = format!("W{wire_no}");
            let (w, ops) = wire_ops(&mut ids, page, &style, vertices, [from_t, to_t], &tag);
            b.step("Wire", ops, "Rename", vec![renamed(ObjectRef::Conductor(w), &tag)])?;
            if let Some(list) = pair_wires.get_mut(pair) {
                list.push(w);
            }
            if let Some(l) = pair_lows.get_mut(pair) {
                *l = low;
            }
        }
        for (pair, wires) in pair_wires.iter().enumerate() {
            if wires.is_empty() {
                continue;
            }
            let (Some((_, from_t, _)), Some((to_t, _, _))) = (terminals.get(pair), terminals.get((pair + 1) % n))
            else {
                return Err("generator: a net's component is missing".into());
            };
            let mut ts = vec![*from_t, *to_t];
            ts.sort();
            let mut cs = wires.clone();
            cs.sort();
            nets.push(IntendedNet { terminals: ts, conductors: cs });
        }

        // Cables (`cableslash`): cable `c` takes every wire of pair `c`, cores `1, 2, …`.
        let mut first_cable: Option<(CableId, Vec<ConductorId>, Pt)> = None;
        for c in 0..k {
            cable_no += 1;
            let members = pair_wires.get(c).cloned().unwrap_or_default();
            let x = terminals.get(c).map_or(0.0, |t| t.2.x) + PITCH / 4.0;
            let lowest = pair_lows.get(c).copied().unwrap_or(ROW_Y);
            let (from, to) = (Pt::new(x - 2.0, ROW_Y - 17.0), Pt::new(x + 2.0, lowest - 3.0));
            let (cable, item, rep): (CableId, ItemId, RepresentationId) = (ids.fresh(), ids.fresh(), ids.fresh());
            let tag = format!("-W{cable_no}");
            let mut value = Cable::default();
            value.properties.insert("tag".into(), tag.clone());
            let mut ops = vec![Op::CreateCable { id: cable, cable: value }];
            for (index, w) in members.iter().enumerate() {
                ops.push(Op::AddCableMember {
                    cable,
                    index,
                    member: CableMember { conductor: *w, core: Some((index + 1).to_string()) },
                });
            }
            ops.push(Op::AddItem {
                id: item,
                item: Item {
                    page,
                    style: style.clone(),
                    kind: ItemKind::Primitive(Primitive::Line { a: from, b: to }),
                },
            });
            ops.push(Op::LinkRepresentation { id: rep, rep: drawn(Subject::Cable(cable), page, "slash", vec![item]) });
            b.step("Cable slash", ops, "Rename", vec![renamed(ObjectRef::Cable(cable), &tag)])?;
            if c == 0 {
                first_cable = Some((cable, members, Pt::new(x + PITCH / 4.0, lowest - 6.0)));
            }
        }

        // The previous page's first cable continues here (`cableref` there, `cablecontinue` here).
        if let Some((from_page, cable, members, at)) = previous.take() {
            let (item, rep): (ItemId, RepresentationId) = (ids.fresh(), ids.fresh());
            let point =
                Item { page: from_page, style: style.clone(), kind: ItemKind::Primitive(Primitive::Point { at }) };
            b.step(
                "Cable reference",
                vec![
                    Op::AddItem { id: item, item: point.clone() },
                    Op::LinkRepresentation {
                        id: rep,
                        rep: drawn(Subject::Cable(cable), from_page, REFERENCE_ROLE, vec![item]),
                    },
                ],
                "Move",
                vec![moved(item, point, 5.0)],
            )?;
            let here = Pt::new(PITCH / 2.0, ROW_Y + 80.0);
            let (ref_item, ref_rep): (ItemId, RepresentationId) = (ids.fresh(), ids.fresh());
            let ref_point =
                Item { page, style: style.clone(), kind: ItemKind::Primitive(Primitive::Point { at: here }) };
            let mut ops = vec![
                Op::AddItem { id: ref_item, item: ref_point.clone() },
                Op::LinkRepresentation {
                    id: ref_rep,
                    rep: drawn(Subject::Cable(cable), page, REFERENCE_ROLE, vec![ref_item]),
                },
            ];
            for (lane, w) in members.iter().enumerate() {
                let y = here.y - 5.0 * lane as f64;
                let (i, r): (ItemId, RepresentationId) = (ids.fresh(), ids.fresh());
                ops.push(Op::AddItem {
                    id: i,
                    item: Item {
                        page,
                        style: style.clone(),
                        kind: ItemKind::Primitive(Primitive::Line {
                            a: Pt::new(here.x, y),
                            b: Pt::new(here.x + 30.0, y),
                        }),
                    },
                });
                ops.push(Op::LinkRepresentation { id: r, rep: drawn(Subject::Conductor(*w), page, "route", vec![i]) });
            }
            let span = 5.0 * members.len() as f64;
            let (slash_item, slash_rep): (ItemId, RepresentationId) = (ids.fresh(), ids.fresh());
            ops.push(Op::AddItem {
                id: slash_item,
                item: Item {
                    page,
                    style: style.clone(),
                    kind: ItemKind::Primitive(Primitive::Line {
                        a: Pt::new(here.x + 13.0, here.y + 3.0),
                        b: Pt::new(here.x + 17.0, here.y - span),
                    }),
                },
            });
            ops.push(Op::LinkRepresentation {
                id: slash_rep,
                rep: drawn(Subject::Cable(cable), page, "slash", vec![slash_item]),
            });
            b.step("Continue cable", ops, "Move", vec![moved(ref_item, ref_point, 5.0)])?;
            continuations.push(Continuation { cable, from: from_page, to: page, references: [rep, ref_rep] });
        }
        previous = first_cable.map(|(cable, members, at)| (page, cable, members, at));
    }
    b.doc.check().map_err(|e| format!("generated {} {} project is invalid: {e}", tier.name, variant.name()))?;
    nets.sort_by(|a, z| a.conductors.cmp(&z.conductors));
    let head = b.log.len() as u64;
    let ledger = Ledger::from_checkpoint(b.doc, head, b.log, SEED)
        .map_err(|e| format!("generated {} {} history is invalid: {e}", tier.name, variant.name()))?;
    Ok(Generated { tier, variant, project, ledger, pages, nets, continuations })
}
