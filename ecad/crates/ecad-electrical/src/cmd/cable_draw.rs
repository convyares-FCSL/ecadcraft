//! The rules of the drawn cable commands (briefs 004, 009, 026, 028, 032, 059): `cableslash` (a new
//! cable, or onto one that exists), `cableadd` and `cablecore`. Each reads the document and returns
//! the ops to commit and what to report.
//!
//! Excludes: `cableview` and `cablefan` (presentation and fan geometry, measured with the engine's
//! fonts), the interactive forms and the JSON.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use ecad_model::{
    CableId, CableMember, Document, Form, IdAllocator, Item, ItemId, ItemKind, Ledger, Op, PageId, Primitive, Pt, Representation, RepresentationId,
    Style, Subject, WireId,
};

use crate::devices::TagHigh;
use crate::error::CmdError;
use crate::slash::{self, Crossings, MAX_CABLE_MEMBERS};

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
        None => tag.and_then(|t| crate::references::cable_tagged(doc, t)),
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
    let blocked: Vec<(WireId, CableId)> = slash::blocked(doc, &found.members).into_iter().map(|(m, k)| (m.conductor, k)).collect();
    if !blocked.is_empty() {
        return Err(CmdError::refused(slash::already_in_cable(doc, &blocked, "Nothing was created.")));
    }
    let (cable, item, rep): (CableId, ItemId, RepresentationId) = (ids.fresh(), ids.fresh(), ids.fresh());
    let mut c = ecad_model::Cable::default();
    // Brief 026: every new cable gets a tag (`-W01`, or `-W?1` with provisional tags).
    let t = crate::devices::new_cable_tag(ledger, high, tag)?;
    c.properties.insert(ecad_model::tag::TAG_KEY.into(), t);
    let mut ops = vec![Op::CreateCable { id: cable, cable: c }];
    for (index, m) in found.members.iter().enumerate() {
        ops.push(Op::AddCableMember { cable, index, member: CableMember { conductor: m.conductor, core: None } });
    }
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
    let blocked: Vec<(WireId, CableId)> = conductors.iter().filter_map(|w| doc.cables_of(*w).first().map(|k| (*w, *k))).collect();
    if !blocked.is_empty() {
        return Err(CmdError::refused(slash::already_in_cable(doc, &blocked, "Nothing was changed.")));
    }
    // Each wire once per call, and the cap holds for the whole cable, not only for one call.
    let mut seen = BTreeSet::new();
    if let Some(w) = conductors.iter().find(|w| !seen.insert(**w)) {
        return Err(CmdError::refused(format!("`conductors` lists wire {} twice; nothing was changed", w.hex())));
    }
    let had = match existing {
        Some(c) => doc.cables.get(&c).ok_or(CmdError::refused(NO_CABLE))?.members.len(),
        None => 0,
    };
    if had.saturating_add(conductors.len()) > MAX_CABLE_MEMBERS {
        return Err(CmdError::refused(format!("a cable holds at most {MAX_CABLE_MEMBERS} wires; nothing was changed")));
    }
    let mut ops = Vec::new();
    let (cable, mut index) = match existing {
        Some(c) => (c, had),
        None => {
            let c: CableId = ids.fresh();
            let mut cable = ecad_model::Cable::default();
            // Brief 026: every new cable gets a tag (`-W01`, or `-W?1` with provisional tags).
            let t = crate::devices::new_cable_tag(ledger, high, tag)?;
            cable.properties.insert(ecad_model::tag::TAG_KEY.into(), t);
            ops.push(Op::CreateCable { id: c, cable });
            (c, 0)
        }
    };
    // Brief 059 (design 059 §4.3): onto a cable not drawn yet, never more wires than free cores.
    if let Some(k) = existing
        && !crate::cables::is_drawn(doc, k)
    {
        crate::cmd::cable::assign_cores(doc, k, conductors.len())?;
    }
    let mut spare: VecDeque<String> =
        existing.and_then(|k| crate::strips::cable_cores(doc, k)).map(|c| c.spare.into_iter().collect()).unwrap_or_default();
    for w in conductors {
        ops.push(Op::AddCableMember { cable, index, member: CableMember { conductor: *w, core: spare.pop_front() } });
        index += 1;
    }
    Ok((cable, ops))
}

/// A core label as given: `None` (null), or text (empty or spaces clear it).
///
/// # Errors
/// A label with a control character, or too long.
fn core_label(text: Option<&str>) -> Result<Option<String>, CmdError> {
    crate::strips::core_label(text)
}

/// The ops changing member `index` of `cable` from `old` to `new` (design 009 §5.3).
fn core_ops(cable: CableId, index: usize, conductor: WireId, old: Option<String>, new: Option<String>) -> [Op; 2] {
    [
        Op::RemoveCableMember { cable, index, member: CableMember { conductor, core: old } },
        Op::AddCableMember { cable, index, member: CableMember { conductor, core: new } },
    ]
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
    let members = doc.cables.get(&cable).map(|c| c.members.as_slice()).unwrap_or_default();
    let index = members.iter().position(|m| m.conductor == conductor).ok_or(CmdError::refused(NO_CONDUCTOR))?;
    let old = members.get(index).and_then(|m| m.core.clone());
    let duplicates = match core.as_deref() {
        Some(c) => members.iter().filter(|m| m.conductor != conductor && m.core.as_deref() == Some(c)).map(|m| m.conductor).collect(),
        None => Vec::new(),
    };
    let ops = if old == core { Vec::new() } else { core_ops(cable, index, conductor, old, core.clone()).to_vec() };
    Ok(CoreSet { cable, conductor, index, core, duplicates, ops })
}

/// `cablecore {cable, cores}`: one label (or null) per member of `cable`, in member order. The
/// labels as stored, and the ops (empty when nothing changes).
///
/// # Errors
/// No such cable; not one label per member; a label that does not read (`Core 2: …`).
pub fn cores(doc: &Document, cable: CableId, given: &[Option<String>]) -> Result<(Vec<Option<String>>, Vec<Op>), CmdError> {
    let members = &doc.cables.get(&cable).ok_or(CmdError::refused(NO_CABLE))?.members;
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
    let mut ops = Vec::new();
    for (index, (m, new)) in members.iter().zip(&cores).enumerate() {
        if m.core != *new {
            ops.extend(core_ops(cable, index, m.conductor, m.core.clone(), new.clone()));
        }
    }
    Ok((cores, ops))
}
