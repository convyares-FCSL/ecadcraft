//! The rules of the drawn cable commands (briefs 004, 009, 026, 028, 032, 059): `cableslash` (a new
//! cable, or onto one that exists), `cableadd` and `cablecore`. Each reads the document and returns
//! the ops to commit and what to report.
//!
//! Excludes: `cableview` and `cablefan` (presentation and fan geometry, measured with the engine's
//! fonts), the interactive forms and the JSON.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use ecad_model::{
    CableId, Core, CorePosition, Document, Form, IdAllocator, Item, ItemId, ItemKind, Ledger, Op, PageId, Primitive, Pt, Representation,
    RepresentationId, Style, Subject, WireId,
};

use crate::cables::slash::{self, Crossings, MAX_CABLE_MEMBERS};
use crate::devices::tags::TagHigh;
use crate::error::CmdError;

/// No conductor with the ID given.
const NO_CONDUCTOR: &str = "no such conductor";
/// No cable with the ID given.
const NO_CABLE: &str = "no such cable";

/// What the slash `from → to` crosses on `page`.
///
/// # Errors
/// `from` and `to` the same point; more wires than a cable holds; no wire.
pub fn crossed(doc: &Document, page: PageId, from: Pt, to: Pt, tol: f64) -> Result<Crossings, CmdError> {
    if from.near(to, tol) {
        return Err(CmdError::input("`from` and `to` must differ"));
    }
    let found = slash::crossings(doc, page, from, to, tol);
    if found.members.len() > MAX_CABLE_MEMBERS {
        return Err(CmdError::refused(slash::too_many()));
    }
    if found.members.is_empty() {
        return Err(CmdError::refused(slash::no_wires(found.ignored.len())));
    }
    Ok(found)
}

/// Which cable a slash makes or continues.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SlashTarget {
    /// A new cable.
    New,
    /// A cable not drawn yet: the crossed wires are assigned to it (brief 059, FD5).
    Assign(CableId),
    /// A drawn cable continued on this page (brief 032 §3.3).
    Continue(CableId),
}

/// The cable a slash with `existing` (an ID) or `tag` goes to: that cable, or the one with the
/// tag, else a new one.
///
/// # Errors
/// No cable with the ID given.
pub fn slash_target(doc: &Document, existing: Option<CableId>, tag: Option<&str>) -> Result<SlashTarget, CmdError> {
    let k = match existing {
        Some(k) if doc.cables.contains_key(&k) => Some(k),
        Some(_) => return Err(CmdError::refused(NO_CABLE)),
        None => tag.and_then(|t| crate::references::cables::cable_tagged(doc, t)),
    };
    Ok(match k {
        Some(k) if !crate::cables::is_drawn(doc, k) => SlashTarget::Assign(k),
        Some(k) => SlashTarget::Continue(k),
        None => SlashTarget::New,
    })
}

/// A new cable made by a slash.
#[derive(Debug, Clone, PartialEq)]
pub struct Slashed {
    pub cable: CableId,
    /// The slash's line.
    pub item: ItemId,
    /// Its representation.
    pub rep: RepresentationId,
    pub ops: Vec<Op>,
}

/// The ops of a new cable whose members are the wires `found` crosses, in crossing order, with the
/// slash `from → to` on `page` as its drawn representation. Its tag is `tag`, or the next free one.
///
/// # Errors
/// A crossed wire already in a cable; a tag that does not read or is taken.
#[allow(clippy::too_many_arguments)]
pub fn slash_new(
    ledger: &Ledger, ids: &mut IdAllocator, high: &mut TagHigh, page: PageId, style: Style, from: Pt, to: Pt, found: &Crossings, tag: Option<&str>,
) -> Result<Slashed, CmdError> {
    let doc = ledger.document();
    let blocked: Vec<(CorePosition, CableId)> = slash::blocked(doc, &found.members).into_iter().map(|(m, k)| (m.position, k)).collect();
    if !blocked.is_empty() {
        return Err(CmdError::refused(slash::already_in_cable(doc, &blocked, "Nothing was created.")));
    }
    let (cable, item, rep): (CableId, ItemId, RepresentationId) = (ids.fresh(), ids.fresh(), ids.fresh());
    let mut c = ecad_model::Cable::default();
    // Brief 026: every new cable gets a tag (`-W01`, or `-W?1` with provisional tags).
    let t = crate::devices::tags::new_cable_tag(ledger, high, tag)?;
    c.properties.insert(ecad_model::naming::tag::TAG_KEY.into(), t);
    c.cores = found.members.iter().map(|m| Core { position: m.position, ..Core::unplaced(None) }).collect();
    let mut ops = vec![Op::CreateCable { id: cable, cable: c }];
    ops.push(Op::AddItem { id: item, item: Item { page, style, kind: ItemKind::Primitive(Primitive::Line { a: from, b: to }) } });
    ops.push(Op::LinkRepresentation {
        id: rep,
        rep: Representation {
            subject: Subject::Cable(cable),
            page,
            form: Form::Drawn,
            role: ecad_model::Role::Slash,
            items: vec![item],
            connection_points: BTreeMap::new(),
        },
    });
    Ok(Slashed { cable, item, rep, ops })
}

/// The hex of the wires a slash crossed, in crossing order (the `conductors` of a result).
pub fn crossing_wires(members: &[slash::Crossing]) -> Vec<String> {
    members.iter().filter_map(|m| m.wire()).map(|w| w.hex()).collect()
}

/// The hex of the loose lines (no wire yet) a slash crossed, in crossing order (the `lines` of a
/// result).
pub fn crossing_lines(members: &[slash::Crossing]) -> Vec<String> {
    members.iter().filter_map(|m| m.position.route()).map(|r| r.hex()).collect()
}

/// The slash onto a cable not drawn yet (FD5): the ops, the slash's line and representation, and
/// the core label each crossed line took.
#[derive(Debug, Clone, PartialEq)]
pub struct SlashAssign {
    /// The slash's line.
    pub item: ItemId,
    /// Its representation.
    pub rep: RepresentationId,
    /// Each crossed line's core label, in crossing order.
    pub cores: Vec<Option<String>>,
    pub ops: Vec<Op>,
}

/// `cableslash` onto cable `k` that is not drawn (FD5): the crossed wires become its members in
/// crossing order with its free cores' labels, plus the slash `from → to` on `page`.
///
/// # Errors
/// A wire already in a cable, more wires than free cores, too many members; nothing is written.
#[allow(clippy::too_many_arguments)]
pub fn plan_slash_assign(
    doc: &Document, ids: &mut IdAllocator, page: PageId, style: Style, k: CableId, from: Pt, to: Pt, found: &Crossings,
) -> Result<SlashAssign, CmdError> {
    let blocked: Vec<(CorePosition, CableId)> = slash::blocked(doc, &found.members).into_iter().map(|(m, c)| (m.position, c)).collect();
    if !blocked.is_empty() {
        return Err(crate::cables::commands::cables::already_in_cable(slash::already_in_cable(doc, &blocked, "Nothing was changed.")));
    }
    if found.members.len() > MAX_CABLE_MEMBERS {
        return Err(crate::cables::commands::cables::too_many());
    }
    let cores = crate::cables::commands::cables::assign_cores(doc, k, found.members.len())?;
    let (item, rep): (ItemId, RepresentationId) = (ids.fresh(), ids.fresh());
    let mut ops = Vec::new();
    // Design 072 §4.5: each crossed line takes its core, the stored unplaced one of that label.
    if let Some(before) = doc.cables.get(&k) {
        let mut after = before.clone();
        for (m, core) in found.members.iter().zip(cores.iter()) {
            crate::styles::cable::place_core(&mut after, core.as_deref().and_then(|c| ecad_model::CoreLabel::new(c).ok()), m.position);
        }
        ops.extend(Op::modify_cable(k, before, after));
    }
    ops.push(Op::AddItem { id: item, item: Item { page, style, kind: ItemKind::Primitive(Primitive::Line { a: from, b: to }) } });
    ops.push(Op::LinkRepresentation {
        id: rep,
        rep: Representation {
            subject: Subject::Cable(k),
            page,
            form: Form::Drawn,
            role: ecad_model::Role::Slash,
            items: vec![item],
            connection_points: BTreeMap::new(),
        },
    });
    Ok(SlashAssign { item, rep, cores, ops })
}

/// The ops of `cableadd`: `conductors` become members of `existing`, or of a new cable tagged
/// `tag` (or the next free tag). A cable with a core offer gives each new member the next free
/// core, in the order given (brief 028, design 028 §6.2). Returns the cable and the ops.
///
/// # Errors
/// A wire already in a cable or listed twice; no cable with the ID given; more members than a
/// cable holds; more wires than free cores onto a cable not drawn yet (brief 059); a tag that
/// does not read or is taken. Nothing is changed then.
pub fn add_to_cable(
    ledger: &Ledger, ids: &mut IdAllocator, high: &mut TagHigh, existing: Option<CableId>, conductors: &[WireId], tag: Option<&str>,
) -> Result<(CableId, Vec<Op>), CmdError> {
    let doc = ledger.document();
    // A conductor is in at most one cable (design 004, owner answer O5; §18.4 defers several).
    let blocked: Vec<(CorePosition, CableId)> =
        conductors.iter().filter_map(|w| doc.cables_of(*w).first().map(|k| (CorePosition::Wire(*w), *k))).collect();
    if !blocked.is_empty() {
        return Err(CmdError::refused(slash::already_in_cable(doc, &blocked, "Nothing was changed.")));
    }
    // Each wire once per call, and the cap holds for the whole cable, not only for one call.
    let mut seen = BTreeSet::new();
    if let Some(w) = conductors.iter().find(|w| !seen.insert(**w)) {
        return Err(CmdError::refused(format!("`conductors` lists wire {} twice; nothing was changed", w.hex())));
    }
    let had = match existing {
        Some(c) => doc.cables.get(&c).ok_or(CmdError::refused(NO_CABLE))?.cores.iter().filter(|m| m.position != CorePosition::Unplaced).count(),
        None => 0,
    };
    if had.saturating_add(conductors.len()) > MAX_CABLE_MEMBERS {
        return Err(CmdError::refused(format!("a cable holds at most {MAX_CABLE_MEMBERS} wires; nothing was changed")));
    }
    let mut ops = Vec::new();
    // Brief 059 (design 059 §4.3): onto a cable not drawn yet, never more wires than free cores.
    if let Some(k) = existing
        && !crate::cables::is_drawn(doc, k)
    {
        crate::cables::commands::cables::assign_cores(doc, k, conductors.len())?;
    }
    let mut spare: VecDeque<String> =
        existing.and_then(|k| crate::terminal_strips::cable_cores(doc, k)).map(|c| c.spare.into_iter().collect()).unwrap_or_default();
    let (cable, before, mut after) = match existing.and_then(|c| doc.cables.get(&c).map(|x| (c, x))) {
        Some((c, x)) => (c, Some(x), x.clone()),
        None => {
            let c: CableId = ids.fresh();
            let mut cable = ecad_model::Cable::default();
            // Brief 026: every new cable gets a tag (`-W01`, or `-W?1` with provisional tags).
            let t = crate::devices::tags::new_cable_tag(ledger, high, tag)?;
            cable.properties.insert(ecad_model::naming::tag::TAG_KEY.into(), t);
            (c, None, cable)
        }
    };
    // Design 072 §4.5: each wire takes the next free core, the stored unplaced one of that label.
    for w in conductors {
        let label = spare.pop_front().and_then(|l| ecad_model::CoreLabel::new(&l).ok());
        crate::styles::cable::place_core(&mut after, label, CorePosition::Wire(*w));
    }
    match before {
        Some(b) => ops.extend(Op::modify_cable(cable, b, after)),
        None => ops.push(Op::CreateCable { id: cable, cable: after }),
    }
    Ok((cable, ops))
}

/// A core label as given: `None` (null), or text (empty or spaces clear it).
///
/// # Errors
/// A label with a control character, or too long.
fn core_label(text: Option<&str>) -> Result<Option<String>, CmdError> {
    crate::terminal_strips::core_label(text)
}

/// Core `index` of `cable` relabelled `new` (design 009 §5.3).
///
/// # Errors
/// A label the stored form does not take.
fn relabel(cable: &mut ecad_model::Cable, index: usize, new: Option<&str>) -> Result<(), CmdError> {
    let label = new.map(ecad_model::CoreLabel::new).transpose().map_err(|e| CmdError::refused(format!("Core label: {e}")))?;
    if let Some(m) = cable.cores.get_mut(index) {
        m.label = label;
    }
    Ok(())
}

/// One member's core label set by `cablecore`.
#[derive(Debug, Clone, PartialEq)]
pub struct CoreSet {
    pub cable: CableId,
    pub conductor: WireId,
    /// The member's index in the cable.
    pub index: usize,
    /// The label as stored.
    pub core: Option<String>,
    /// The other members with the same label (design 009 §5.4, duplicates allowed).
    pub duplicates: Vec<WireId>,
    /// Empty when the label is unchanged.
    pub ops: Vec<Op>,
}

/// `cablecore {conductor, core}` (design 009 §5.2): the ops setting or clearing wire
/// `conductor`'s core label.
///
/// # Errors
/// No such conductor; the wire is not in a cable; a label that does not read.
pub fn core(doc: &Document, conductor: WireId, core: Option<&str>) -> Result<CoreSet, CmdError> {
    let core = core_label(core)?;
    if !doc.wires.contains_key(&conductor) {
        return Err(CmdError::refused(NO_CONDUCTOR));
    }
    let Some(cable) = doc.cables_of(conductor).first().copied() else {
        return Err(CmdError::refused(format!("Wire {} is not in a cable.", slash::wire_name(doc, conductor))));
    };
    let cab = doc.cables.get(&cable).ok_or(CmdError::refused(NO_CABLE))?;
    let members = cab.cores.as_slice();
    let index = members.iter().position(|m| m.wire() == Some(conductor)).ok_or(CmdError::refused(NO_CONDUCTOR))?;
    let old = members.get(index).and_then(|m| m.label_str().map(str::to_string));
    let duplicates = match core.as_deref() {
        Some(c) => members.iter().filter(|m| m.wire() != Some(conductor) && m.label_str() == Some(c)).filter_map(|m| m.wire()).collect(),
        None => Vec::new(),
    };
    let mut after = cab.clone();
    if old != core {
        relabel(&mut after, index, core.as_deref())?;
    }
    let ops = Op::modify_cable(cable, cab, after).into_iter().collect();
    Ok(CoreSet { cable, conductor, index, core, duplicates, ops })
}

/// `cablecore {cable, cores}`: one label (or null) per member of `cable`, in member order. The
/// labels as stored, and the ops (empty when nothing changes).
///
/// # Errors
/// No such cable; not one label per member; a label that does not read (`Core 2: …`).
pub fn cores(doc: &Document, cable: CableId, given: &[Option<String>]) -> Result<(Vec<Option<String>>, Vec<Op>), CmdError> {
    let cab = doc.cables.get(&cable).ok_or(CmdError::refused(NO_CABLE))?;
    let members = &cab.cores;
    if given.len() != members.len() {
        return Err(CmdError::refused(format!(
            "`cores` must list one label (or null) per member: cable {} has {} members",
            crate::cables::cable_title(doc, cable),
            members.len()
        )));
    }
    let mut cores = Vec::with_capacity(given.len());
    for (i, v) in given.iter().enumerate() {
        cores.push(core_label(v.as_deref()).map_err(|e| CmdError::refused(format!("Core {}: {e}", i + 1)))?);
    }
    let mut after = cab.clone();
    for (index, (m, new)) in members.iter().zip(&cores).enumerate() {
        if m.label_str() != new.as_deref() {
            relabel(&mut after, index, new.as_deref())?;
        }
    }
    Ok((cores, Op::modify_cable(cable, cab, after).into_iter().collect()))
}
