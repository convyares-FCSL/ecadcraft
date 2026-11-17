//! The rules of the device commands (design 018 §6–§8; experiments 004 and 008): tag settings
//! (`tagsettings`), deleting a wire, device or cable (`ecaddelete`, `devicedelete`), and the ops of
//! `convertcomponent` and `addterminal`. Typed input in, ops and a plain result (or a
//! [`CmdError`]) out.
//!
//! Excludes: what a handle on the page is (the engine crate's page map), committing and the JSON
//! answer (the engine crate's shims).

use std::collections::BTreeMap;

use ecad_model::{
    CableId, Component, ComponentId, Conductor, ConductorId, Document, End, Form, IdAllocator, ItemId, Op, PageId, ProjectSettings, Pt,
    Representation, RepresentationId, Subject, Terminal, TerminalId,
};

use crate::devices;
use crate::error::CmdError;

// ---------------------------------------------------------------- tagsettings

/// Tag settings to write: the padding, and per starter symbol key the prefix (`None`: back to the
/// built-in one).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TagSettings {
    pub padding: Option<u8>,
    pub prefixes: Vec<(String, Option<String>)>,
}

/// The tag settings `tagsettings` is given (brief 018, owner decision 2026-11-08): `padding` digits,
/// and per starter symbol the project's own prefix (`None`: the built-in one).
///
/// # Errors
/// A padding out of range, an unknown symbol, a prefix that is not 1 to
/// [`devices::MAX_PREFIX_LETTERS`] letters.
pub fn tag_settings(padding: Option<u64>, prefixes: impl IntoIterator<Item = (String, Option<String>)>) -> Result<TagSettings, CmdError> {
    let max = ecad_model::tag::MAX_PADDING;
    let padding = match padding {
        None => None,
        Some(n) => Some(
            Some(n)
                .filter(|n| (1..=u64::from(max)).contains(n))
                .and_then(|n| u8::try_from(n).ok())
                .ok_or_else(|| CmdError::refused(format!("`padding` must be a whole number from 1 to {max}")))?,
        ),
    };
    let mut changes = Vec::new();
    for (k, v) in prefixes {
        let symbol = devices::starter(&k)?;
        let prefix = match v {
            None => None,
            Some(t) => Some(
                devices::valid_prefix(&t)
                    .ok_or_else(|| CmdError::refused(format!("A prefix is 1 to {} letters, for example K or SR.", devices::MAX_PREFIX_LETTERS)))?,
            ),
        };
        changes.push((symbol.key.to_string(), prefix));
    }
    Ok(TagSettings { padding, prefixes: changes })
}

/// Apply tag settings read by [`tag_settings`] to `after`: a prefix equal to the built-in one is
/// stored as none.
pub fn apply_tag_settings(after: &mut ProjectSettings, settings: TagSettings) {
    if let Some(n) = settings.padding {
        after.tag_padding = Some(n);
    }
    for (k, v) in settings.prefixes {
        crate::devices::set_symbol_prefix(after, ecad_model::prefix::PrefixOf::Starter(k.into()), v.as_deref());
    }
}

// ---------------------------------------------------------------- ecaddelete

/// What `ecaddelete` deletes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Doomed {
    Conductor(ConductorId),
    Component(ComponentId),
    Cable(CableId),
}

/// A planned delete.
#[derive(Debug, Clone, PartialEq)]
pub struct Deleted {
    pub ops: Vec<Op>,
    /// A rack's cards that left it (brief 045).
    pub released: usize,
    /// A cable's wires, which stay (design 004 §8).
    pub kept: Option<usize>,
    /// What to say, when there is something to say.
    pub message: Option<String>,
}

/// Ops removing every representation of `subject` with its items.
fn remove_representations(doc: &Document, subject: Subject) -> Vec<Op> {
    let mut ops = Vec::new();
    for (rid, r) in doc.representations_of(subject) {
        ops.push(Op::UnlinkRepresentation { id: rid, rep: r.clone() });
        for i in &r.items {
            if let Some(item) = doc.items.get(i) {
                ops.push(Op::RemoveItem { id: *i, item: item.clone() });
            }
        }
    }
    ops
}

fn kept_words(n: usize) -> String {
    match n {
        0 => "0 wires are".into(),
        1 => "1 wire is".into(),
        n => format!("{n} wires are"),
    }
}

/// `ecaddelete {conductor} | {component} | {cable}`: the object with its drawings, in one
/// transaction. A wire leaves its cables; a device's connection points let go of their wires and a
/// rack's cards leave it (design 045 §2.2, FP1); a cable's wires stay (design 004 §8, owner answer
/// O3).
///
/// # Errors
/// No such conductor, component or cable.
pub fn plan_delete(doc: &Document, doomed: Doomed) -> Result<Deleted, CmdError> {
    let mut ops = Vec::new();
    match doomed {
        Doomed::Conductor(c) => {
            let conductor = doc.conductors.get(&c).ok_or(CmdError::refused("no such conductor"))?;
            for end in End::BOTH {
                if let Some(t) = conductor.end(end) {
                    ops.push(Op::AttachEnd { conductor: c, end, before: Some(t), after: None });
                }
            }
            for cable in doc.cables_of(c) {
                if let Some((i, m)) = doc.cables.get(&cable).and_then(|k| k.members.iter().enumerate().find(|(_, m)| m.conductor == c)) {
                    ops.push(Op::RemoveCableMember { cable, index: i, member: m.clone() });
                }
            }
            ops.extend(remove_representations(doc, Subject::Conductor(c)));
            ops.push(Op::DeleteConductor { id: c, conductor: Conductor { ends: [None, None], ..conductor.clone() } });
            Ok(Deleted { ops, released: 0, kept: None, message: None })
        }
        Doomed::Component(c) => {
            let component = doc.components.get(&c).ok_or(CmdError::refused("no such component"))?;
            for (t, _) in doc.terminals_of(c) {
                for (w, end) in doc.ends_at(t) {
                    ops.push(Op::AttachEnd { conductor: w, end, before: Some(t), after: None });
                }
            }
            ops.extend(remove_representations(doc, Subject::Component(c)));
            for (t, term) in doc.terminals_of(c) {
                ops.push(Op::RemoveTerminal { id: t, terminal: term.clone() });
            }
            // Brief 045 (design 045 §2.2, FP1): a rack's cards leave it in the same transaction.
            let (released_ops, released) = if crate::plc::rack::is_rack(doc, c) { crate::plc::rack::release_cards(doc, c) } else { (Vec::new(), 0) };
            ops.extend(released_ops);
            ops.push(Op::DeleteComponent { id: c, component: component.clone() });
            let message = (released > 0).then(|| format!("Deleted {}; {}", devices::tag_words(doc, c), crate::plc::rack::released_words(released)));
            Ok(Deleted { ops, released, kept: None, message })
        }
        Doomed::Cable(k) => {
            let cable = doc.cables.get(&k).ok_or(CmdError::refused("no such cable"))?;
            for (index, member) in cable.members.iter().enumerate().rev() {
                ops.push(Op::RemoveCableMember { cable: k, index, member: member.clone() });
            }
            ops.extend(remove_representations(doc, Subject::Cable(k)));
            ops.push(Op::DeleteCable { id: k, cable: ecad_model::Cable { members: Vec::new(), ..cable.clone() } });
            let kept = cable.members.len();
            let message = format!("Cable {} deleted; its {} kept.", k.hex(), kept_words(kept));
            Ok(Deleted { ops, released: 0, kept: Some(kept), message: Some(message) })
        }
    }
}

// ---------------------------------------------------------------- convertcomponent, addterminal

/// A component made of drawn geometry ([`plan_convert`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Converted {
    pub component: ComponentId,
    pub representation: RepresentationId,
    pub ops: Vec<Op>,
}

/// `convertcomponent` (experiment 004): plain geometry `items` on `page` become one new component,
/// drawn.
pub fn plan_convert(ids: &mut IdAllocator, page: PageId, items: Vec<ItemId>) -> Converted {
    let (component, representation): (ComponentId, RepresentationId) = (ids.fresh(), ids.fresh());
    let ops = vec![
        Op::CreateComponent { id: component, component: Component::default() },
        Op::LinkRepresentation {
            id: representation,
            rep: Representation {
                subject: Subject::Component(component),
                page,
                form: Form::Drawn,
                role: "drawn".into(),
                items,
                connection_points: BTreeMap::new(),
            },
        },
    ];
    Converted { component, representation, ops }
}

/// The refusal of an item that belongs to no component.
pub const NOT_A_COMPONENTS_ITEM: &str = "that item is not part of a component";

/// `addterminal` (experiment 008): a connection point labelled `label` at `at` on the drawn
/// component that `item` is part of.
///
/// # Errors
/// An item of no component; a placed symbol (its connection points come from its definition).
pub fn plan_addterminal(
    doc: &Document, ids: &mut IdAllocator, item: ItemId, at: Pt, label: Option<String>,
) -> Result<(TerminalId, Vec<Op>), CmdError> {
    let (rid, r) = doc.representation_of_item(item).ok_or(CmdError::refused(NOT_A_COMPONENTS_ITEM))?;
    let Subject::Component(c) = r.subject else { return Err(CmdError::refused(NOT_A_COMPONENTS_ITEM)) };
    if r.form != Form::Drawn {
        return Err(CmdError::refused("connection points of placed symbols come from their definition"));
    }
    let t: TerminalId = ids.fresh();
    let mut after = r.clone();
    after.connection_points.insert(t, at);
    let ops = vec![
        Op::AddTerminal { id: t, terminal: Terminal { component: c, label, key: None } },
        Op::ModifyRepresentation { id: rid, before: r.clone(), after },
    ];
    Ok((t, ops))
}
