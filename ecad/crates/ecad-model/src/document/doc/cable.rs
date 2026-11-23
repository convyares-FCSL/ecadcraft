//! Cables and their cores.

use serde::{Deserialize, Serialize};

use crate::naming::id::{CableId, ConnectionPointId, RouteId, WireId};

use super::{Properties, document::*, drawing::*};
use crate::document::op::Op;

/// Where a cable core is: stored, never worked out from the
/// connections on read.
///
/// # Semantics
/// - `Unplaced`: the core is on no line yet.
/// - `Route`: the core is on a drawn line that carries no wire yet (a loose line); `landed` is the
///   connection point of the one end that has landed, when exactly one has.
/// - `Wire`: the core is that wire. The command that lands a line's second end moves its cores
///   from the route to the wire it forms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorePosition {
    Unplaced,
    Route { route: RouteId, landed: Option<ConnectionPointId> },
    Wire(WireId),
}

impl CorePosition {
    /// The wire, when the core is on one.
    pub fn wire(self) -> Option<WireId> {
        match self {
            CorePosition::Wire(w) => Some(w),
            CorePosition::Unplaced | CorePosition::Route { .. } => None,
        }
    }

    /// The route, when the core is on a line with no wire yet.
    pub fn route(self) -> Option<RouteId> {
        match self {
            CorePosition::Route { route, .. } => Some(route),
            CorePosition::Unplaced | CorePosition::Wire(_) => None,
        }
    }
}

/// What a cable core is for: stored apart from where it is, never
/// worked out from its position.
///
/// # Semantics
/// - `InUse`: the core is meant to carry a conductor. One left [`CorePosition::Unplaced`] is an
///   open core (a check).
/// - `Spare`: the core is ticked **Not used**; it may stay unplaced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    #[default]
    InUse,
    Spare,
}

/// The longest core label.
pub const MAX_CORE_LABEL: usize = 32;

/// A core's label: `1`, `BN`, `GNYE`.
pub type CoreLabel = crate::naming::text::Text<MAX_CORE_LABEL>;

/// One core of a cable: the cable's cores are its whole core set, so a core the
/// cable offers but no line carries yet is stored, [`CorePosition::Unplaced`].
///
/// # Semantics
/// `colours`, when set, are the core's own; `None` takes them from the cable type by core order.
/// `purpose` and `position` are independent: a spare core may sit on a line, an in-use core may be
/// unplaced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Core {
    pub label: Option<CoreLabel>,
    pub colours: Option<crate::objects::wire::Colours>,
    pub purpose: Purpose,
    pub position: CorePosition,
    /// The core's named points on the cable's references: one connection
    /// point per reference end, positioned by that reference's representation. Every point listed
    /// here joins the others, so a line landing on one page's point and a line landing on the other
    /// page's are one net, and the wire runs from device point to device point across the pages.
    /// They are aids: never a wire's end.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub through: Vec<ConnectionPointId>,
}

impl Core {
    /// An in-use core on wire `w`.
    pub fn on_wire(w: WireId, label: Option<CoreLabel>) -> Self {
        Core { label, colours: None, purpose: Purpose::InUse, position: CorePosition::Wire(w), through: Vec::new() }
    }

    /// An in-use core on no line yet.
    pub fn unplaced(label: Option<CoreLabel>) -> Self {
        Core { label, colours: None, purpose: Purpose::InUse, position: CorePosition::Unplaced, through: Vec::new() }
    }

    /// The label's text, when it has one.
    pub fn label_str(&self) -> Option<&str> {
        self.label.as_ref().map(CoreLabel::as_str)
    }

    /// Whether the core is ticked Not used.
    pub fn is_spare(&self) -> bool {
        self.purpose == Purpose::Spare
    }

    /// The core once its line is gone (a write rule, never a read): a labelled or spare core is one
    /// of the cable's core set and stays, unplaced; an unlabelled in-use core was only that line's
    /// membership and goes. `None` too when `others` already hold an unplaced core of that label.
    pub fn off_line(&self, others: &[Core]) -> Option<Core> {
        // A core with named points on the cable's references stays: they are placed there.
        if self.label.is_none() && !self.is_spare() && self.through.is_empty() {
            return None;
        }
        let taken = self.label.is_some() && others.iter().any(|o| o.position == CorePosition::Unplaced && o.label == self.label);
        (!taken).then(|| Core { position: CorePosition::Unplaced, ..self.clone() })
    }

    /// The wire this core is, when it is one.
    pub fn wire(&self) -> Option<WireId> {
        self.position.wire()
    }

    /// The route this core is on, when it has no wire yet.
    pub fn route(&self) -> Option<RouteId> {
        self.position.route()
    }
}

/// The style-choice index key of a cable's cable type ([`Cable::cable_type`]); never a property.
pub const CABLE_TYPE_KEY: &str = "cable_type";

/// A cable groups existing conductors.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Cable {
    /// Its tag (`-W01`) and what it is (the identity of the cable), as [`Device::tag`].
    pub tag: crate::naming::tag::TagSlot,
    /// What it is for, in words (`setprop` key `description`).
    pub description: Option<crate::objects::devices::Description>,
    pub properties: Properties,
    /// The whole core set, in core order.
    pub cores: Vec<Core>,
    /// Its cable type (a style of catalogue `cable`): what its size, colours and
    /// shield come from when it has none of its own. A reference to no type is kept (a check row).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cable_type: Option<crate::objects::styles::StyleRef>,
    /// The cable's own shield; `None` takes its cable type's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shield: Option<crate::objects::styles::cable::Shield>,
    /// The cable's own rating inputs; each overrides the project's.
    #[serde(default, skip_serializing_if = "crate::objects::wire::CircuitInputs::is_empty")]
    pub rating: crate::objects::wire::CircuitInputs,
    /// The design current typed for the cable (`load`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load: Option<crate::geometry::units::Amps>,
    /// The device named as its protection (`rating {protected_by}`), over the trace's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protected_by: Option<crate::objects::wire::ProtectedBy>,
}

impl Cable {
    /// Its tag as text.
    pub fn tag(&self) -> Option<&str> {
        self.tag.tag()
    }

    /// Its description as text.
    pub fn description(&self) -> Option<&str> {
        self.description.as_ref().map(|t| t.as_str())
    }

    /// Whether its tag is fixed.
    pub fn is_fixed(&self) -> bool {
        self.tag.is_fixed()
    }

    /// Whether its tag is provisional.
    pub fn is_provisional(&self) -> bool {
        self.tag.is_provisional()
    }

    /// Its tag as drawn: a provisional tag with its `?` ([`crate::naming::tag::shown`]).
    pub fn tag_shown(&self) -> Option<String> {
        self.tag.shown()
    }

    /// Its tag's identity in the tag index.
    pub fn tag_key(&self) -> Option<crate::naming::tag::Key> {
        self.tag.key()
    }

    /// The text `setprop`'s key `key` names on this cable: its tag, its description, else the
    /// property. (The fixed flag is not text: [`Cable::value`].)
    pub fn get(&self, key: &str) -> Option<&str> {
        match key {
            crate::naming::tag::TAG_KEY => self.tag(),
            crate::objects::devices::DESCRIPTION_KEY => self.description(),
            _ => self.properties.get(key).map(String::as_str),
        }
    }

    /// The value key `key` names as stored text ([`Cable::get`]).
    pub fn value(&self, key: &str) -> Option<String> {
        self.get(key).map(str::to_string)
    }

    /// Set the value key `key` names (`None`, or text blank once trimmed, clears it): the tag (as
    /// [`Cable::tag`] holds it: bare text, kept in the state it has, automatic if it had none), the
    /// description, else the property.
    ///
    /// # Errors
    /// The [`crate::naming::text::TextError`] of a tag or description that is too long or holds a
    /// hidden character.
    pub fn set_value(&mut self, key: &str, value: Option<&str>) -> std::result::Result<(), crate::naming::text::TextError> {
        let trimmed = value.map(str::trim).filter(|v| !v.is_empty());
        match key {
            crate::naming::tag::TAG_KEY => {
                let state = self.tag.state().unwrap_or_default();
                self.tag = crate::naming::tag::TagSlot::new(trimmed.map(crate::naming::tag::Tag::new).transpose()?, state);
            }
            crate::objects::devices::DESCRIPTION_KEY => {
                self.description = trimmed.map(crate::objects::devices::Description::new).transpose()?;
            }
            _ => {
                match value {
                    Some(v) => self.properties.insert(key.to_string(), v.to_string()),
                    None => self.properties.remove(key),
                };
            }
        }
        Ok(())
    }
}

/// The ops making cable `from` part of cable `into` (Make them one): its members join
/// (after `into`'s own; a wire in both stays once), its drawings show `into`, and it is deleted.
/// `doc` needs a current index (its drawings are found through it). Used by the merge and by
/// `setprop`'s merge choice alike.
///
/// # Errors
/// A missing cable, or `from == into`.
pub fn join_cables(doc: &Document, from: CableId, into: CableId) -> std::result::Result<Vec<Op>, crate::ParseError> {
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
        ops.push(Op::LinkRepresentation { id: rid, rep: crate::Representation { subject: Subject::Cable(into), ..r.clone() } });
    }
    ops.push(Op::DeleteCable { id: from, cable: empty });
    Ok(ops)
}
