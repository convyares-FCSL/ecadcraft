//! Wire and cable rating checks (brief 057, design 057): is each panel wire and field cable big
//! enough for the device that protects it, **Ib ≤ In ≤ Iz**, and is a cable's volt drop within its
//! limit. Calculations are checks, not tools: every result is a row in the Checks list.
//!
//! # Semantics
//! - A **rated subject** ([`RatedSubject`]) is a panel wire (a conductor that is not a cable core)
//!   or a field cable that has rating data (design 057 §7): a wire whose category has a rated
//!   current, or with `rating.*`, `load` or `protected_by` of its own; a cable with a cable type,
//!   `rating.*`, `load` or `protected_by`, or a readable size when the project has chosen a rating
//!   table. Anything else makes no rows, so a project that does not rate wires is not nagged.
//! - Iz comes from one of three bases ([`Basis`], §3): the datasheet (category or cable type), the
//!   project's **rating table** (user-supplied, [`dataset`]; ECAD ships no values) or a typed value.
//!   Factors are rounded to 2 dp before multiplying; comparisons use the unrounded product.
//! - In comes from the protective device ([`trace`]), Ib from a [`load::LoadSource`].
//!
//! # Update map (design 057 §10)
//! [`Kept`] holds every subject's result and three indexes: `protects` (device → subjects),
//! `path` (net → subjects, from the trace) and `users` (wire category / cable type → subjects).
//! [`Kept::update`] re-evaluates only what an op reaches ([`Touch`]):
//!
//! | Op | Re-checked |
//! | --- | --- |
//! | a conductor's or cable's own property | that subject (re-traced) |
//! | a device's `rating` / `rating.<n>` | `protects[device]` (result only) |
//! | a device's `tag` | subjects it protects or that name it (result only) |
//! | a location box's panel temperature or location (`ModifyAid`), a device's `location` | every wire's result |
//! | connectivity (conductors, ends, points, components, definitions, links, cable members) | subjects whose path holds a touched net, and those on a touched net (re-traced) |
//! | `SetStyle` of a wire category or cable type | its users (result only) |
//! | `SetStyle` of a rating table, `SetStandard`, a project or standard `rating.*` / `wire.*` key, the potentials | every result (no trace) |
//! | anything else | nothing |
//!
//! The calculations are split by side (design 072 §2.4): [`circuit`] works out a subject's Iz,
//! factors and volt drop (lane D's `Circuit`), [`protection`] the device that protects it (lane
//! B's `Protects`); this module keeps the shared words, the update map and the Checks rows.
//!
//! This module reads the document; it changes nothing.

pub mod circuit;
pub mod commands;
pub mod kept;
pub mod load;
pub mod protection;
pub mod rows;
pub mod trace;

use circuit::{
    BoxTemp, Context, Rating, Temp, box_temp, box_temps, cable_is_rated, evaluate, fmt_amps, fmt_percent, is_panel_wire, location_code, sheet,
    subjects, wire_is_rated,
};
use protection::{Device, device_of};
use std::collections::{BTreeMap, BTreeSet};

use ecad_model::objects::styles::wire::{ConductorClass, Material};

use ecad_model::objects::wire::CircuitInputs;
use ecad_model::{CableId, DeviceId, Document, NetKey, ObjectRef, Op, StyleRef, WireId, Word};

use crate::defaults::{self, Scope};
use crate::devices::ratings::dataset::{Dataset, Insulation};
use crate::styles::{self, cable, ratingtable, wire};
use load::{Load, LoadFrom, LoadSource, TypedLoads};
use trace::{Found, How};

/// The style fields lane 057 adds (design 057 §4.4, FP3 and FP4): wire categories take the five
/// rating fields, cable types all of them.
pub mod fields {
    // Each catalogue that lists one of these holds its own copy in its static `FIELDS`, whose
    // slot is filled once there; the constants themselves are never read directly (brief 066).
    #![allow(clippy::declare_interior_mutable_const, reason = "copied into each catalogue's static field list")]

    use crate::defaults::{Kind, Slot};
    use crate::styles::Field;

    fn is_current(s: &str) -> bool {
        crate::devices::ratings::parse::current(s).is_some()
    }

    fn is_size(s: &str) -> bool {
        crate::devices::ratings::parse::cross_section(s).is_some()
    }

    pub const RATED_CURRENT: Field = Field {
        key: "rated_current",
        label: "Rated current (A)",
        kind: Kind::Parsed { max_chars: 32, parse: is_current },
        slot: Slot::new(),
        over: None,
    };
    pub const RATED_AT: Field =
        Field { key: "rated_at", label: "Rated at (\u{b0}C)", kind: Kind::Number { min: -40.0, max: 90.0 }, slot: Slot::new(), over: None };
    pub const MAX_TEMPERATURE: Field = Field {
        key: "max_temperature",
        label: "Max conductor temperature (\u{b0}C)",
        kind: Kind::Number { min: 50.0, max: 250.0 },
        slot: Slot::new(),
        over: None,
    };
    pub const MATERIAL: Field =
        Field { key: "material", label: "Material", kind: Kind::Choice(<super::Material as ecad_model::Word>::WORDS), slot: Slot::new(), over: None };
    pub const CONDUCTOR_CLASS: Field = Field {
        key: "conductor_class",
        label: "Conductor class",
        kind: Kind::Choice(<super::ConductorClass as ecad_model::Word>::WORDS),
        slot: Slot::new(),
        over: None,
    };
    pub const CROSS_SECTION: Field = Field {
        key: "cross_section",
        label: "Cross-section (mm\u{b2})",
        kind: Kind::Parsed { max_chars: 32, parse: is_size },
        slot: Slot::new(),
        over: None,
    };
}

/// A refusal of a wire category or cable type record whose reference temperature is not below its
/// maximum (design 057 §3.1); `None` for every other record.
pub(crate) fn check_style(catalogue: ecad_model::StyleCatalogue, r: &ecad_model::objects::styles::StyleRecord) -> Option<String> {
    if catalogue != wire::CATALOGUE.catalogue && catalogue != ecad_model::StyleCatalogue::Cable {
        return None;
    }
    let rating = match &r.body {
        ecad_model::objects::styles::StyleBody::Wire(w) => &w.rating,
        ecad_model::objects::styles::StyleBody::Cable(c) => &c.rating,
        _ => return None,
    };
    let (at, max) = (rating.rated_at.map_or(30.0, |c| c.0), rating.max_temperature.map_or(70.0, |c| c.0));
    (at >= max).then(|| {
        format!(
            "Rated at {} \u{b0}C is not below the conductor's {} \u{b0}C.",
            crate::devices::ratings::dataset::fmt_num(at),
            crate::devices::ratings::dataset::fmt_num(max)
        )
    })
}

/// A panel wire or a field cable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RatedSubject {
    Wire(WireId),
    Cable(CableId),
}

impl RatedSubject {
    pub fn object(self) -> ObjectRef {
        match self {
            RatedSubject::Wire(c) => ObjectRef::Wire(c),
            RatedSubject::Cable(k) => ObjectRef::Cable(k),
        }
    }

    /// The wire or cable itself, as a [`Circuit`].
    pub fn circuit(self, doc: &Document) -> Option<&dyn Circuit> {
        match self {
            RatedSubject::Wire(c) => doc.wires().get(&c).map(|x| x as &dyn Circuit),
            RatedSubject::Cable(k) => doc.cables().get(&k).map(|x| x as &dyn Circuit),
        }
    }

    /// Its own rating inputs and load: a wire's or a cable's fields (design 072 §4.2, §4.5).
    pub fn inputs(self, doc: &Document) -> (CircuitInputs, Option<f64>) {
        self.circuit(doc).map_or_else(Default::default, |c| (*c.inputs(), c.load()))
    }

    /// Its words in a row: the wire number (or `wire`), the cable tag.
    pub fn words(self, doc: &Document) -> String {
        match self {
            RatedSubject::Wire(c) => doc.wires().get(&c).and_then(|x| x.number_text()).map(str::to_string).unwrap_or_else(|| {
                let conn = doc.connectivity();
                conn.net_of_conductor(c)
                    .and_then(|n| conn.net(n))
                    .and_then(|n| ecad_model::connectivity::wiring::net_number(doc, n))
                    .unwrap_or_else(|| "wire".into())
            }),
            RatedSubject::Cable(k) => crate::cables::cable_title(doc, k),
        }
    }
}

/// What the rating check reads from a panel wire or a field cable (design 072 §4.5): its typed
/// rating inputs and load, and the few words still kept as properties.
///
/// # Semantics
/// [`evaluate`], the volt drop and the protection side read a subject only through this trait, so
/// a wire and a cable are rated by the same code. `length` and `type` are a cable's properties
/// until they get types of their own.
pub trait Circuit {
    /// Its own rating inputs; each overrides the project's.
    fn inputs(&self) -> &CircuitInputs;
    /// Its typed design current Ib, in A.
    fn load(&self) -> Option<f64>;
    /// Its length as typed (`25 m`).
    fn length(&self) -> Option<&str>;
    /// Its free-text type (`4G2.5`), read for a size when it has no cable type.
    fn type_text(&self) -> Option<&str>;
    /// The device it names as its protection, over the trace's.
    fn protected_by(&self) -> Option<ecad_model::objects::wire::ProtectedBy>;
}

fn named<'a>(p: &'a ecad_model::Properties, key: &str) -> Option<&'a str> {
    p.get(key).map(String::as_str).filter(|v| !v.trim().is_empty())
}

impl Circuit for ecad_model::objects::wire::Wire {
    fn inputs(&self) -> &CircuitInputs {
        &self.rating
    }
    fn load(&self) -> Option<f64> {
        self.load.map(|a| a.0)
    }
    fn length(&self) -> Option<&str> {
        named(&self.display, "length")
    }
    fn type_text(&self) -> Option<&str> {
        named(&self.display, "type")
    }
    fn protected_by(&self) -> Option<ecad_model::objects::wire::ProtectedBy> {
        self.protected_by
    }
}

impl Circuit for ecad_model::Cable {
    fn inputs(&self) -> &CircuitInputs {
        &self.rating
    }
    fn load(&self) -> Option<f64> {
        self.load.map(|a| a.0)
    }
    fn length(&self) -> Option<&str> {
        named(&self.properties, "length")
    }
    fn type_text(&self) -> Option<&str> {
        named(&self.properties, "type")
    }
    fn protected_by(&self) -> Option<ecad_model::objects::wire::ProtectedBy> {
        self.protected_by
    }
}

use ecad_model::objects::wire::Basis;

/// Property keys (FP1, FP2, FP6).
pub(crate) const BASIS: &str = "rating.basis";
pub(crate) const MANUAL: &str = "rating.manual";
pub(crate) const BUNDLING: &str = "rating.bundling";
pub(crate) const LOAD: &str = "load";
pub(crate) const PROTECTED_BY: &str = "protected_by";
pub(crate) const METHOD: &str = "rating.method";
pub const AMBIENT: &str = "rating.ambient";
pub const GROUPED: &str = "rating.grouped";
pub(crate) const VOLTAGE: &str = "rating.voltage";
pub(crate) const SYSTEM: &str = "rating.system";
pub const VD_LIMIT: &str = "rating.vd_limit";

/// A problem a result raises (one Checks row each; the count kinds are summed project-wide).
#[derive(Debug, Clone, PartialEq)]
pub enum Issue {
    /// Iz < In.
    Under {
        larger: Option<f64>,
    },
    /// The panel or ambient is at or above θmax.
    TooHot {
        place: String,
        temp: f64,
        max: f64,
    },
    LoadOver,
    VoltDropOver,
    BundlingUnset,
    /// `protected_by` names a device that is not there (read so from a file).
    DeviceMissing(String),
    /// No device found (`true`: the trace stopped at its bound).
    NoDevice(bool),
    TwoDevices(String, String),
    /// A device whose rating does not read.
    DeviceUnread(DeviceId, String),
    NamedDevice(String),
    NoData(String),
    /// The basis is a rating table and none is chosen.
    NoTableChosen,
    NoTable(String),
    Assumed,
    NoCurrent,
    BadValue(&'static str, String),
}
