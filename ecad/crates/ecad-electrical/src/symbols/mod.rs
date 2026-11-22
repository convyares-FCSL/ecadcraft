//! The starter symbols (brief 018, design 018 §4): about 18 IEC 60617-style definitions built into
//! the app, embedded into a project the first time they are used.
//!
//! # Semantics
//! - Each starter symbol is a [`Definition`] with an [`Origin`]: one fixed library ID for "ECAD
//!   starter symbols", a fixed definition ID per symbol, `library_seq` 1 and the content hash of
//!   proposal §20.10. A project's embedded copy is found by that origin, never by name.
//! - What a symbol *is* (its kind, default tag prefix, aliases and default description) is kept
//!   here, in the catalogue, keyed by origin; the stored `Definition` holds none of it.
//! - Geometry is in millimetres, local frame, y up, symbol centre at (0,0), terminals on a 5 mm
//!   grid. The shapes follow IEC 60617 forms but are drawn new for ECAD.
//!
//! Brief 034 grows the catalogue by family (contacts, protection, sensors, actuators, connectors,
//! PLC card bodies) and adds text anchors for lane T ([`anchors`]), the rating rule
//! ([`setprop_rule`]) and quiet points ([`is_quiet`]).
//!
//! The catalogue reads no project; [`setprop_rule`] reads one to find a fuse bank's fuses, and the
//! project-symbol helpers read its symbol versions.
//! Nothing here changes a project.

#![deny(clippy::wildcard_enum_match_arm)]

pub mod builder;
pub mod commands;
pub mod proposals;
pub mod reps;
use ecad_model::geometry::units::Radians;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use crate::CmdError;
use ecad_model::{
    ConnectionPointDef, Definition, DefinitionId, DeviceGroup, DeviceType, Document, LibraryId, ObjectRef, Op, Origin, PartRole, PointKey, Primitive,
    Pt, Subject, Word,
};

/// The library ID of the starter symbols.
pub const STARTER_LIBRARY: LibraryId = LibraryId(0x0000_ecad_0018_5ea1_0000_0000_0000_0001);

/// The terminal grid of the starter symbols, in millimetres (`ecad.symbol-grid`).
pub const SYMBOL_GRID_MM: f64 = 5.0;

/// How a symbol joins a device (design 018 §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A contact: its labels follow the IEC sequence digit (`13`/`14`, `21`/`22`…). `make` for
    /// a normally-open contact.
    Contact { make: bool },
    /// A single terminal: numbered 1, 2, 3…
    TerminalBlock,
    /// A PLC channel of a card of this type (brief 045, FP7): it joins a card as its next free
    /// channel, its points labelled with pins and keyed `.<i>.<role>` ([`crate::plc::channels`]).
    Channel(crate::plc::rack::CardType),
    /// A change-over contact (brief 034): common, NC and NO, numbered by the next IEC sequence
    /// digit (`11`/`12`/`14`, `21`/`22`/`24`…), see [`changeover_labels`].
    ChangeOver,
    /// Fixed labels from the definition. `exclusive`: a device holds at most one of these.
    Fixed { exclusive: bool },
    /// A tagless component of its own, never part of a device: a continuation
    /// (brief 022, FP4).
    Tagless,
    /// A connector half, or one pin of one or both halves (brief 044, design 044 §3.3): its
    /// connection points are written as the side keys `.s` / `.p` with the pin names as labels,
    /// and it joins a connector by [`crate::connectors`]' rules. Catalogue code, never stored.
    ConnectorHalf { side: Side },
}

/// Which half of a connector a symbol draws (brief 044).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Side {
    /// The socket half (female contacts): side key `.s`.
    Socket,
    /// The plug half (male contacts): side key `.p`.
    Plug,
    /// Both halves of one pin (the pin pair): `.s` and `.p`.
    Both,
}

impl Side {
    /// The side key a pin of this side is stored under (`.s` / `.p`); `None` for [`Side::Both`].
    pub fn key(self) -> Option<PointKey> {
        match self {
            Side::Socket => Some(SOCKET_KEY),
            Side::Plug => Some(PLUG_KEY),
            Side::Both => None,
        }
    }

    /// The side of a side key.
    pub fn of_key(key: &PointKey) -> Option<Side> {
        match key.side()? {
            ecad_model::Side::Socket => Some(Side::Socket),
            ecad_model::Side::Plug => Some(Side::Plug),
        }
    }

    /// The other half.
    pub fn other(self) -> Side {
        match self {
            Side::Socket => Side::Plug,
            Side::Plug => Side::Socket,
            Side::Both => Side::Both,
        }
    }

    /// `socket`, `plug` or `both`.
    pub fn word(self) -> &'static str {
        match self {
            Side::Socket => "socket",
            Side::Plug => "plug",
            Side::Both => "both",
        }
    }

    /// The side letter of the per-half property keys (`half.s.*`); `None` for [`Side::Both`].
    pub fn letter(self) -> Option<char> {
        match self {
            Side::Socket => Some('s'),
            Side::Plug => Some('p'),
            Side::Both => None,
        }
    }
}

/// The terminal key of a socket pin (design 044 FP1).
pub const SOCKET_KEY: PointKey = PointKey::Side(ecad_model::Side::Socket);
/// The terminal key of a plug pin (design 044 FP1).
pub const PLUG_KEY: PointKey = PointKey::Side(ecad_model::Side::Plug);

ecad_model::word_enum! {
    /// The key of a starter symbol (design 072 §9.1): one value per row of [`STARTERS`], in table
    /// order, stored and typed as its word (`coil`, `no`, `term2.1`…). Variants are the words
    /// with a capital first letter, `.` written `L` (level).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum StarterKey {
        Coil,
        No,
        Nc,
        Power3,
        Mcb1,
        Mcb2,
        Fuse,
        Psu,
        Pbno,
        Pbnc,
        Sel,
        Lamp,
        Motor3,
        Motor1,
        Trafo,
        TerminalBlock,
        TerminalBlockPe,
        Di,
        Do,
        Continuation,
        Co,
        Swco,
        Power4,
        Rcbo2,
        Rcd2,
        Rcd4,
        Ssrin,
        Ssrout,
        Fuseterm,
        Fusebank4,
        Fusebank6,
        Fusebank8,
        Fusebank10,
        Fusebank12,
        Prox3,
        Floatno,
        Floatnc,
        Rollerno,
        Rollernc,
        Interlocknc,
        Interlockno,
        Valve,
        Valve2,
        Tc,
        Heater,
        Socket2,
        Socket3,
        Socket4,
        Socket6,
        Socket8,
        Socket12,
        Plug2,
        Plug3,
        Plug4,
        Plug6,
        Plug8,
        Plug12,
        Dsub9s,
        Dsub15s,
        Dsub25s,
        Dsub9p,
        Dsub15p,
        Dsub25p,
        M12s4,
        M12s5,
        M12s8,
        M12p4,
        M12p5,
        M12p8,
        Rj45,
        Usba,
        Usbb,
        Pinpair,
        Pins,
        Pinp,
        Plccpu,
        Plcps,
        Plcdi,
        Plcdq,
        Plcai,
        Plcao,
        Plcrtd,
        Plciol,
        #[strum(serialize = "term2.1")]
        Term2L1,
        #[strum(serialize = "term2.2")]
        Term2L2,
        Term2,
        #[strum(serialize = "term2b.1")]
        Term2bL1,
        #[strum(serialize = "term2b.2")]
        Term2bL2,
        Term2b,
        #[strum(serialize = "term3.1")]
        Term3L1,
        #[strum(serialize = "term3.2")]
        Term3L2,
        #[strum(serialize = "term3.3")]
        Term3L3,
        Term3,
        #[strum(serialize = "termlnpe.1")]
        TermlnpeL1,
        #[strum(serialize = "termlnpe.2")]
        TermlnpeL2,
        #[strum(serialize = "termlnpe.3")]
        TermlnpeL3,
        Termlnpe,
        #[strum(serialize = "termsensor.1")]
        TermsensorL1,
        #[strum(serialize = "termsensor.2")]
        TermsensorL2,
        #[strum(serialize = "termsensor.3")]
        TermsensorL3,
        Termsensor,
        Termdisc,
        Termfuse,
        Plcim,
        Plcpm,
        Plccm,
        Chdi,
        Chdq,
        Chai,
        Chao,
        Chrtd,
        Chiol,
    }
}

use StarterKey as K;

impl StarterKey {
    /// The group a device this starter begins belongs to (design 072 §2.2): what the starter
    /// declares in its definition's [`Definition::device_type`]. Continuations are
    /// `General` until lane F makes them drawing aids.
    pub fn device_group(self) -> DeviceGroup {
        match self {
            K::Psu | K::Trafo => DeviceGroup::PowerSupply,
            K::Fuse
            | K::Mcb1
            | K::Mcb2
            | K::Rcbo2
            | K::Rcd2
            | K::Rcd4
            | K::Fuseterm
            | K::Fusebank4
            | K::Fusebank6
            | K::Fusebank8
            | K::Fusebank10
            | K::Fusebank12 => DeviceGroup::Protection,
            K::Coil | K::No | K::Nc | K::Co | K::Power3 | K::Power4 | K::Ssrin | K::Ssrout | K::Pbno | K::Pbnc | K::Sel | K::Swco => {
                DeviceGroup::Switching
            }
            K::Prox3 | K::Floatno | K::Floatnc | K::Rollerno | K::Rollernc | K::Interlockno | K::Interlocknc | K::Tc => DeviceGroup::Sensor,
            K::Motor1 | K::Motor3 => DeviceGroup::Motor,
            K::Heater | K::Valve | K::Valve2 => DeviceGroup::Load,
            K::Lamp => DeviceGroup::Indicator,
            K::TerminalBlock
            | K::TerminalBlockPe
            | K::Term2L1
            | K::Term2L2
            | K::Term2
            | K::Term2bL1
            | K::Term2bL2
            | K::Term2b
            | K::Term3L1
            | K::Term3L2
            | K::Term3L3
            | K::Term3
            | K::TermlnpeL1
            | K::TermlnpeL2
            | K::TermlnpeL3
            | K::Termlnpe
            | K::TermsensorL1
            | K::TermsensorL2
            | K::TermsensorL3
            | K::Termsensor
            | K::Termdisc
            | K::Termfuse => DeviceGroup::TerminalStrip,
            K::Socket2
            | K::Socket3
            | K::Socket4
            | K::Socket6
            | K::Socket8
            | K::Socket12
            | K::Plug2
            | K::Plug3
            | K::Plug4
            | K::Plug6
            | K::Plug8
            | K::Plug12
            | K::Dsub9s
            | K::Dsub15s
            | K::Dsub25s
            | K::Dsub9p
            | K::Dsub15p
            | K::Dsub25p
            | K::M12s4
            | K::M12s5
            | K::M12s8
            | K::M12p4
            | K::M12p5
            | K::M12p8
            | K::Rj45
            | K::Usba
            | K::Usbb
            | K::Pinpair
            | K::Pins
            | K::Pinp => DeviceGroup::Connector,
            K::Plccpu | K::Plcim => DeviceGroup::PlcRack,
            K::Plcps
            | K::Plcpm
            | K::Plccm
            | K::Plcdi
            | K::Plcdq
            | K::Plcai
            | K::Plcao
            | K::Plcrtd
            | K::Plciol
            | K::Di
            | K::Do
            | K::Chdi
            | K::Chdq
            | K::Chai
            | K::Chao
            | K::Chrtd
            | K::Chiol => DeviceGroup::PlcModule,
            K::Continuation => DeviceGroup::General,
        }
    }

    /// The ways of a fuse bank key (`fusebank8` → 8); `None` for any other key.
    pub fn fuse_bank_ways(self) -> Option<usize> {
        match self {
            K::Fusebank4 => Some(4),
            K::Fusebank6 => Some(6),
            K::Fusebank8 => Some(8),
            K::Fusebank10 => Some(10),
            K::Fusebank12 => Some(12),
            K::Coil
            | K::No
            | K::Nc
            | K::Power3
            | K::Mcb1
            | K::Mcb2
            | K::Fuse
            | K::Psu
            | K::Pbno
            | K::Pbnc
            | K::Sel
            | K::Lamp
            | K::Motor3
            | K::Motor1
            | K::Trafo
            | K::TerminalBlock
            | K::TerminalBlockPe
            | K::Di
            | K::Do
            | K::Continuation
            | K::Co
            | K::Swco
            | K::Power4
            | K::Rcbo2
            | K::Rcd2
            | K::Rcd4
            | K::Ssrin
            | K::Ssrout
            | K::Fuseterm
            | K::Prox3
            | K::Floatno
            | K::Floatnc
            | K::Rollerno
            | K::Rollernc
            | K::Interlocknc
            | K::Interlockno
            | K::Valve
            | K::Valve2
            | K::Tc
            | K::Heater
            | K::Socket2
            | K::Socket3
            | K::Socket4
            | K::Socket6
            | K::Socket8
            | K::Socket12
            | K::Plug2
            | K::Plug3
            | K::Plug4
            | K::Plug6
            | K::Plug8
            | K::Plug12
            | K::Dsub9s
            | K::Dsub15s
            | K::Dsub25s
            | K::Dsub9p
            | K::Dsub15p
            | K::Dsub25p
            | K::M12s4
            | K::M12s5
            | K::M12s8
            | K::M12p4
            | K::M12p5
            | K::M12p8
            | K::Rj45
            | K::Usba
            | K::Usbb
            | K::Pinpair
            | K::Pins
            | K::Pinp
            | K::Plccpu
            | K::Plcps
            | K::Plcdi
            | K::Plcdq
            | K::Plcai
            | K::Plcao
            | K::Plcrtd
            | K::Plciol
            | K::Term2L1
            | K::Term2L2
            | K::Term2
            | K::Term2bL1
            | K::Term2bL2
            | K::Term2b
            | K::Term3L1
            | K::Term3L2
            | K::Term3L3
            | K::Term3
            | K::TermlnpeL1
            | K::TermlnpeL2
            | K::TermlnpeL3
            | K::Termlnpe
            | K::TermsensorL1
            | K::TermsensorL2
            | K::TermsensorL3
            | K::Termsensor
            | K::Termdisc
            | K::Termfuse
            | K::Plcim
            | K::Plcpm
            | K::Plccm
            | K::Chdi
            | K::Chdq
            | K::Chai
            | K::Chao
            | K::Chrtd
            | K::Chiol => None,
        }
    }
}

impl std::fmt::Display for StarterKey {
    /// Its word.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.pad(self.word())
    }
}

/// One starter symbol in the catalogue.
#[derive(Debug, Clone)]
pub struct Starter {
    /// The short key typed at prompts and used in JSON (`coil`, `no`…).
    pub key: StarterKey,
    /// Its name and default description.
    pub name: &'static str,
    /// Words that also find it.
    pub aliases: &'static [&'static str],
    pub kind: Kind,
    /// Its fixed definition ID inside [`STARTER_LIBRARY`].
    pub id: DefinitionId,
    /// The family Find Symbol groups it in and also matches by its word (brief 034).
    pub family: Family,
    /// Its connection points that may stay unwired without an `Unconnected` check (design 034
    /// §11): spare connector pins, spare fuse outputs, a card's supply points.
    pub quiet: Quiet,
}

/// A catalogue family (design 034 §2.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    /// The 21 symbols of briefs 018, 022 and 028.
    Basic,
    Contacts,
    Protection,
    Sensors,
    Actuators,
    Connectors,
    Plc,
}

impl Family {
    /// The word a search matches (`contacts`, `protection`…); empty for [`Family::Basic`], which no
    /// word finds.
    pub fn word(self) -> &'static str {
        match self {
            Family::Basic => "",
            Family::Contacts => "contacts",
            Family::Protection => "protection",
            Family::Sensors => "sensors",
            Family::Actuators => "actuators",
            Family::Connectors => "connectors",
            Family::Plc => "plc",
        }
    }
}

/// Which connection points of a symbol are quiet (design 034 §2.2, §11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quiet {
    /// None: every point is checked.
    None,
    /// Every point.
    All,
    /// Every point except these keys.
    AllBut(&'static [&'static str]),
}

impl Starter {
    /// The definition with its origin, built and hashed once per run (the Place preview asks for it
    /// every frame).
    ///
    /// # Errors
    /// Its content hash could not be computed (never for the built-in symbols; a definition is
    /// never stored with a made-up origin hash).
    pub fn definition(&self) -> Result<&'static Definition, CmdError> {
        static BUILT: OnceLock<BTreeMap<StarterKey, Result<Definition, CmdError>>> = OnceLock::new();
        let built = BUILT.get_or_init(|| STARTERS.iter().map(|s| (s.key, s.build())).collect());
        match built.get(&self.key) {
            Some(Ok(d)) => Ok(d),
            Some(Err(e)) => Err(e.clone()),
            None => Err(CmdError::refused(format!("{} is not a starter symbol", self.key))),
        }
    }

    fn build(&self) -> Result<Definition, CmdError> {
        // Brief 044 (FP2): a connector half's bridge joins the side keys written at placement.
        let bridges = match self.kind {
            Kind::ConnectorHalf { .. } => vec![vec![SOCKET_KEY, PLUG_KEY]],
            Kind::Contact { .. } | Kind::TerminalBlock | Kind::Channel(_) | Kind::ChangeOver | Kind::Fixed { .. } | Kind::Tagless => {
                crate::terminal_strips::terminal_blocks::bridges(self.key)
            }
        };
        let mut d = Definition {
            bridges,
            name: self.name.into(),
            origin: None,
            graphics: graphics(self.key),
            connection_points: connection_points(self.key),
            device_type: self.device_type(),
            part_role: self.part_role(),
            ..Default::default()
        };
        let content_hash =
            ecad_file::definition_hash(&d).map_err(|e| CmdError::refused(format!("The symbol {} could not be prepared ({e}).", self.name)))?;
        d.origin = Some(Origin { library: STARTER_LIBRARY, definition: self.id, library_seq: self.library_seq(), content_hash });
        Ok(d)
    }

    /// A placed copy's part in its device (design 072 §4.4, F3′): the part its definition
    /// declares. The power pole sets (`power3`, `power4`) are a contactor's contacts.
    pub fn part_role(&self) -> PartRole {
        match self.kind {
            Kind::ConnectorHalf { .. } if crate::connectors::is_pin_starter(self) => PartRole::Pin,
            Kind::Fixed { exclusive: false } if !crate::references::devices::poles_of(self).is_empty() => PartRole::Contact,
            k @ (Kind::ConnectorHalf { .. }
            | Kind::Fixed { .. }
            | Kind::Contact { .. }
            | Kind::TerminalBlock
            | Kind::Channel(_)
            | Kind::ChangeOver
            | Kind::Tagless) => part_of_kind(k),
        }
    }

    /// What a device this starter begins is: the type its definition declares (design 072 §2.3).
    pub fn device_type(&self) -> DeviceType {
        let t = crate::plc::types::starter_type(self.key);
        // A protective device's kind is its symbol's (design 072 §2.2).
        if let DeviceType::Protection(_) = t {
            use ecad_model::objects::devices::ProtectionKind as P;
            let kind = if self.key.fuse_bank_ways().is_some() {
                P::FuseBank
            } else if matches!(self.key, K::Mcb1 | K::Mcb2) {
                P::Mcb
            } else if self.key == K::Rcbo2 {
                P::Rcbo
            } else if matches!(self.key, K::Rcd2 | K::Rcd4) {
                P::Rcd
            } else if self.key == K::Fuseterm {
                P::FuseTerminalBlock
            } else {
                P::Fuse
            };
            return DeviceType::Protection(kind.empty());
        }
        // What operates a switching device is its symbol's (design 072 §2.2).
        if let DeviceType::Switching(_) = t {
            use ecad_model::objects::devices::OperatorKind as O;
            let kind = if matches!(self.key, K::Ssrin | K::Ssrout) {
                O::SolidState
            } else if matches!(self.key, K::Pbno | K::Pbnc | K::Sel | K::Swco) {
                O::Manual
            } else {
                O::Coil
            };
            return DeviceType::Switching(kind.empty());
        }
        // What a sensor senses and outputs is its symbol's (design 072 §2.2).
        if let DeviceType::Sensor(_) = t {
            use ecad_model::objects::devices::{Output as O, Sensing as S, Sensor};
            let known = [
                (&[K::Prox3][..], S::Proximity, O::Pnp),
                (&[K::Floatno, K::Floatnc][..], S::Float, O::Contact),
                (&[K::Rollerno, K::Rollernc][..], S::Limit, O::Contact),
                (&[K::Interlockno, K::Interlocknc][..], S::Interlock, O::Contact),
                (&[K::Tc][..], S::Temperature, O::Thermocouple),
            ];
            if let Some((_, sensing, output)) = known.iter().find(|(keys, _, _)| keys.contains(&self.key)) {
                return DeviceType::Sensor(Sensor::empty(*sensing, *output));
            }
            return t;
        }
        // A motor's phases are its symbol's (design 072 §2.2).
        if let DeviceType::Motor(_) = t {
            use ecad_model::objects::devices::{Motor, Phases};
            return DeviceType::Motor(Motor::empty(if self.key == K::Motor1 { Phases::Single } else { Phases::Three }));
        }
        // An indicator's signal is a starter default, stored when it is created (design 072 §2.2): a
        // lamp is steady. Its colour starts empty.
        if let DeviceType::Indicator(_) = t {
            use ecad_model::objects::devices::{Indicator, SignalType};
            return DeviceType::Indicator(Indicator::empty(SignalType::Steady));
        }
        // Whether it is a power supply or a transformer is its symbol's (design 072 §2.2).
        if let DeviceType::PowerSupply(_) = t {
            use ecad_model::objects::devices::{PowerSupply, Psu, Transformer};
            return DeviceType::PowerSupply(if self.key == K::Trafo {
                PowerSupply::Transformer(Transformer::default())
            } else {
                PowerSupply::Psu(Psu::default())
            });
        }
        // A connector's family is its symbol's (a pin's: plain); a half or pin has mating halves, a
        // bulkhead is one piece.
        if let DeviceType::Connector(_) = t {
            let mut x = ecad_model::objects::devices::Connector::of_family(crate::connectors::family_of(self).unwrap_or_default());
            x.mating = matches!(self.kind, Kind::ConnectorHalf { .. });
            return DeviceType::Connector(Box::new(x));
        }
        t
    }

    /// The version of this starter's definition (`library_seq`): 2 for the 24 mating halves brief
    /// 044 changed (FP3), 1 for every other.
    pub fn library_seq(&self) -> u64 {
        match self.kind {
            Kind::ConnectorHalf { side: Side::Socket | Side::Plug } if !matches!(self.key, K::Pins | K::Pinp) => 2,
            Kind::Contact { .. }
            | Kind::TerminalBlock
            | Kind::Channel(_)
            | Kind::ChangeOver
            | Kind::Fixed { .. }
            | Kind::Tagless
            | Kind::ConnectorHalf { .. } => 1,
        }
    }

    /// The side of a connector half or pin starter.
    pub fn side(&self) -> Option<Side> {
        match self.kind {
            Kind::ConnectorHalf { side } => Some(side),
            Kind::Contact { .. } | Kind::TerminalBlock | Kind::Channel(_) | Kind::ChangeOver | Kind::Fixed { .. } | Kind::Tagless => None,
        }
    }

    /// Its built-in tag prefix (IEC 81346), without `-`, as the defaults registry holds it
    /// (`symbol.<key>.prefix`, [`crate::defaults::symbols`]); empty for a tagless symbol. A
    /// project's own prefix is [`crate::devices::prefix_for`].
    pub fn prefix(&self) -> &'static str {
        crate::defaults::symbols::prefix_entry(self.key).map_or("", crate::defaults::symbols::builtin_prefix)
    }

    /// Whether it can join an existing device (every starter symbol can, within §3.1's rules).
    pub fn joinable(&self) -> bool {
        true
    }

    /// The labels of its terminals as the definition gives them.
    pub fn labels(&self) -> Vec<String> {
        connection_points(self.key).into_iter().map(|t| t.key.to_string()).collect()
    }

    /// Its box (width, height) in millimetres.
    pub fn size(&self) -> (f64, f64) {
        let Ok(d) = self.definition() else { return (0.0, 0.0) };
        let (lo, hi) = bounds(d);
        ((hi.x - lo.x).max(0.0), (hi.y - lo.y).max(0.0))
    }
}

const fn def_id(n: u128) -> DefinitionId {
    DefinitionId(0x0000_ecad_0018_5ea1_0000_0000_0001_0000 + n)
}

/// A catalogue entry of the [`Family::Basic`] family with no quiet points; [`Starter::of`] and
/// [`Starter::quiet_at`] set the rest.
const fn st(key: StarterKey, name: &'static str, aliases: &'static [&'static str], kind: Kind, id: DefinitionId) -> Starter {
    Starter { key, name, aliases, kind, id, family: Family::Basic, quiet: Quiet::None }
}

impl Starter {
    const fn of(self, family: Family) -> Starter {
        Starter { family, ..self }
    }

    const fn quiet_at(self, quiet: Quiet) -> Starter {
        Starter { quiet, ..self }
    }
}

const HEAD: Kind = Kind::Fixed { exclusive: true };
const PART: Kind = Kind::Fixed { exclusive: false };

// Brief 034's aliases, shared by several entries.
const FUSE_BANK: &[&str] = &["fuse bank", "fuse block", "distribution", "protection"];
const SOCKET: &[&str] = &["socket", "female", "connector"];
const PLUG: &[&str] = &["plug", "male", "connector"];
const DSUB_S: &[&str] = &["d-sub", "dsub", "sub-d", "socket", "fieldbus", "profibus", "connector"];
const DSUB_P: &[&str] = &["d-sub", "dsub", "sub-d", "plug", "fieldbus", "profibus", "connector"];
const M12_S: &[&str] = &["m12", "fieldbus", "profinet", "io-link", "socket", "connector"];
const M12_P: &[&str] = &["m12", "fieldbus", "profinet", "io-link", "plug", "connector"];
const USB: &[&str] = &["usb", "bulkhead", "connector"];
// Brief 044's aliases.
const PINPAIR: &[&str] = &["pin pair", "mated pin", "plug and socket", "connector pin", "connector"];
const PIN_S: &[&str] = &["socket pin", "female pin", "connector pin", "connector"];
const PIN_P: &[&str] = &["plug pin", "male pin", "connector pin", "connector"];

/// A connector body (stage 3a): a head, every pin quiet (prefix `X`, `defaults::symbols`).
const fn conn(key: StarterKey, name: &'static str, aliases: &'static [&'static str], id: DefinitionId) -> Starter {
    st(key, name, aliases, HEAD, id).of(Family::Connectors).quiet_at(Quiet::All)
}

/// A mating connector half or pin (brief 044, design 044 §3.1): every pin quiet.
const fn half(key: StarterKey, name: &'static str, aliases: &'static [&'static str], side: Side, id: DefinitionId) -> Starter {
    st(key, name, aliases, Kind::ConnectorHalf { side }, id).of(Family::Connectors).quiet_at(Quiet::All)
}

const S: Side = Side::Socket;
const P: Side = Side::Plug;

/// Contacts and protection (brief 034 stage 1): definition IDs `0x100 + n`.
const fn cp(n: u128) -> DefinitionId {
    def_id(0x100 + n)
}

/// Sensors and actuators (stage 2): `0x200 + n`.
const fn sa(n: u128) -> DefinitionId {
    def_id(0x200 + n)
}

/// Connectors (stage 3a): `0x300 + n`.
const fn cn(n: u128) -> DefinitionId {
    def_id(0x300 + n)
}

/// PLC card bodies (stage 3b): `0x400 + n`.
const fn pc(n: u128) -> DefinitionId {
    def_id(0x400 + n)
}

/// A PLC card body (stage 3b): a head (prefix `A`, `defaults::symbols`).
const fn card(key: StarterKey, name: &'static str, aliases: &'static [&'static str], id: DefinitionId) -> Starter {
    st(key, name, aliases, HEAD, id).of(Family::Plc)
}

/// The catalogue, in table order (design 018 §4, then design 034 §3–§6). A definition ID is never
/// reused or changed once it has been on `dev`.
pub const STARTERS: &[Starter] = &[
    st(K::Coil, "Contactor / relay coil", &["coil", "relay", "contactor"], HEAD, def_id(1)),
    st(K::No, "NO contact", &["make", "normally open", "contact"], Kind::Contact { make: true }, def_id(2)),
    st(K::Nc, "NC contact", &["break", "normally closed", "contact"], Kind::Contact { make: false }, def_id(3)),
    st(K::Power3, "Power contacts, 3-pole", &["main contacts", "contactor"], PART, def_id(4)),
    st(K::Mcb1, "MCB 1-pole", &["breaker", "circuit breaker"], HEAD, def_id(5)),
    st(K::Mcb2, "MCB 2-pole", &["breaker", "circuit breaker"], HEAD, def_id(6)),
    st(K::Fuse, "Fuse", &["fuse"], HEAD, def_id(7)),
    st(K::Psu, "Power supply", &["psu", "ac/dc"], HEAD, def_id(8)),
    st(K::Pbno, "Push button, NO", &["push", "button", "start"], Kind::Contact { make: true }, def_id(9)),
    st(K::Pbnc, "Push button, NC", &["push", "button", "stop"], Kind::Contact { make: false }, def_id(10)),
    st(K::Sel, "Selector switch, 2-position", &["selector", "switch"], Kind::Contact { make: true }, def_id(11)),
    st(K::Lamp, "Indicator lamp", &["lamp", "light", "pilot"], HEAD, def_id(12)),
    st(K::Motor3, "Motor, 3-phase", &["motor"], HEAD, def_id(13)),
    st(K::Motor1, "Motor, single-phase", &["motor"], HEAD, def_id(14)),
    st(K::Trafo, "Transformer", &["control transformer"], HEAD, def_id(15)),
    st(K::TerminalBlock, "Terminal", &["terminal", "clamp"], Kind::TerminalBlock, def_id(16)),
    // Brief 028 (design 028 §2): a protective-earth terminal; it joins a strip like `connection_point`.
    st(K::TerminalBlockPe, "PE terminal", &["pe terminal", "earth terminal"], Kind::TerminalBlock, def_id(21)),
    st(K::Di, "PLC digital input", &["plc", "input", "di"], Kind::Channel(crate::plc::rack::CardType::Di), def_id(17)),
    st(K::Do, "PLC digital output", &["plc", "output", "do"], Kind::Channel(crate::plc::rack::CardType::Dq), def_id(18)),
    st(K::Continuation, "Continuation", &["continuation", "arrow", "potential", "link"], Kind::Tagless, def_id(20)),
    // ---- Brief 034 stage 1: contacts and protection (design 034 §3).
    st(K::Co, "Change-over contact", &["change-over", "changeover", "c/o", "spdt", "contact", "relay"], Kind::ChangeOver, cp(1)).of(Family::Contacts),
    st(K::Swco, "Switch, change-over", &["change-over", "switch", "toggle"], Kind::ChangeOver, cp(2)).of(Family::Contacts),
    st(K::Power4, "Power contacts, 4-pole", &["main contacts", "contactor", "4-pole"], PART, cp(3)).of(Family::Contacts),
    st(K::Rcbo2, "RCBO, 1P+N", &["rcbo", "residual current", "breaker", "protection"], HEAD, cp(4)).of(Family::Protection),
    st(K::Rcd2, "RCD, 2-pole", &["rcd", "rccb", "residual current", "protection"], HEAD, cp(5)).of(Family::Protection),
    st(K::Rcd4, "RCD, 4-pole", &["rcd", "rccb", "residual current", "protection"], HEAD, cp(6)).of(Family::Protection),
    st(K::Ssrin, "SSR input", &["ssr", "solid state relay", "relay"], HEAD, cp(7)).of(Family::Contacts),
    st(K::Ssrout, "SSR output", &["ssr", "solid state relay", "contact"], PART, cp(8)).of(Family::Contacts),
    st(K::Fuseterm, "Fuse terminal", &["fuse", "terminal", "fuse holder", "protection"], HEAD, cp(9)).of(Family::Protection),
    st(K::Fusebank4, "Fuse bank, 4-way", FUSE_BANK, HEAD, cp(10)).of(Family::Protection).quiet_at(Quiet::AllBut(&["L+"])),
    st(K::Fusebank6, "Fuse bank, 6-way", FUSE_BANK, HEAD, cp(11)).of(Family::Protection).quiet_at(Quiet::AllBut(&["L+"])),
    st(K::Fusebank8, "Fuse bank, 8-way", FUSE_BANK, HEAD, cp(12)).of(Family::Protection).quiet_at(Quiet::AllBut(&["L+"])),
    st(K::Fusebank10, "Fuse bank, 10-way", FUSE_BANK, HEAD, cp(13)).of(Family::Protection).quiet_at(Quiet::AllBut(&["L+"])),
    st(K::Fusebank12, "Fuse bank, 12-way", FUSE_BANK, HEAD, cp(14)).of(Family::Protection).quiet_at(Quiet::AllBut(&["L+"])),
    // ---- Brief 034 stage 2: sensors and field devices (design 034 §4).
    st(K::Prox3, "Proximity switch, 3-wire PNP", &["proximity", "prox", "inductive", "sensor"], HEAD, sa(1)).of(Family::Sensors),
    st(K::Floatno, "Float switch, NO", &["float", "level", "sensor"], Kind::Contact { make: true }, sa(2)).of(Family::Sensors),
    st(K::Floatnc, "Float switch, NC", &["float", "level", "sensor"], Kind::Contact { make: false }, sa(3)).of(Family::Sensors),
    st(K::Rollerno, "Position switch with roller, NO", &["roller", "limit", "position", "sensor"], Kind::Contact { make: true }, sa(4))
        .of(Family::Sensors),
    st(K::Rollernc, "Position switch with roller, NC", &["roller", "limit", "position", "sensor"], Kind::Contact { make: false }, sa(5))
        .of(Family::Sensors),
    st(K::Interlocknc, "Interlock switch, NC", &["interlock", "guard", "safety", "door switch", "sensor"], Kind::Contact { make: false }, sa(6))
        .of(Family::Sensors),
    st(K::Interlockno, "Interlock switch, NO", &["interlock", "guard", "safety", "sensor"], Kind::Contact { make: true }, sa(7)).of(Family::Sensors),
    st(K::Valve, "Solenoid valve", &["valve", "solenoid", "actuator"], HEAD, sa(8)).of(Family::Actuators),
    st(K::Valve2, "Dual solenoid valve", &["valve", "solenoid", "dual", "actuator"], HEAD, sa(9)).of(Family::Actuators),
    st(K::Tc, "Thermocouple", &["thermocouple", "temperature", "sensor"], HEAD, sa(10)).of(Family::Sensors),
    st(K::Heater, "Heater", &["heater", "heating element", "heat"], HEAD, sa(11)).of(Family::Actuators),
    // ---- Brief 034 stage 3a: connector bodies (design 034 §5); halves only, mating is M1 1.11.
    // Brief 044 (design 044 §3.1): the mating halves are connector halves, version 2.
    half(K::Socket2, "Socket, 2-pin", SOCKET, S, cn(1)),
    half(K::Socket3, "Socket, 3-pin", SOCKET, S, cn(2)),
    half(K::Socket4, "Socket, 4-pin", SOCKET, S, cn(3)),
    half(K::Socket6, "Socket, 6-pin", SOCKET, S, cn(4)),
    half(K::Socket8, "Socket, 8-pin", SOCKET, S, cn(5)),
    half(K::Socket12, "Socket, 12-pin", SOCKET, S, cn(6)),
    half(K::Plug2, "Plug, 2-pin", PLUG, P, cn(7)),
    half(K::Plug3, "Plug, 3-pin", PLUG, P, cn(8)),
    half(K::Plug4, "Plug, 4-pin", PLUG, P, cn(9)),
    half(K::Plug6, "Plug, 6-pin", PLUG, P, cn(10)),
    half(K::Plug8, "Plug, 8-pin", PLUG, P, cn(11)),
    half(K::Plug12, "Plug, 12-pin", PLUG, P, cn(12)),
    half(K::Dsub9s, "D-sub socket, 9-way", DSUB_S, S, cn(13)),
    half(K::Dsub15s, "D-sub socket, 15-way", DSUB_S, S, cn(14)),
    half(K::Dsub25s, "D-sub socket, 25-way", DSUB_S, S, cn(15)),
    half(K::Dsub9p, "D-sub plug, 9-way", DSUB_P, P, cn(16)),
    half(K::Dsub15p, "D-sub plug, 15-way", DSUB_P, P, cn(17)),
    half(K::Dsub25p, "D-sub plug, 25-way", DSUB_P, P, cn(18)),
    half(K::M12s4, "M12 socket, 4-pin", M12_S, S, cn(19)),
    half(K::M12s5, "M12 socket, 5-pin", M12_S, S, cn(20)),
    half(K::M12s8, "M12 socket, 8-pin", M12_S, S, cn(21)),
    half(K::M12p4, "M12 plug, 4-pin", M12_P, P, cn(22)),
    half(K::M12p5, "M12 plug, 5-pin", M12_P, P, cn(23)),
    half(K::M12p8, "M12 plug, 8-pin", M12_P, P, cn(24)),
    conn(K::Rj45, "RJ45 bulkhead", &["rj45", "ethernet", "profinet", "bulkhead", "connector"], cn(25)),
    conn(K::Usba, "USB-A bulkhead", USB, cn(26)),
    conn(K::Usbb, "USB-B bulkhead", USB, cn(27)),
    // ---- Brief 044: connector pins (design 044 §3.2), drawn one by one across wires.
    half(K::Pinpair, "Connector pin pair", PINPAIR, Side::Both, cn(28)),
    half(K::Pins, "Connector socket pin", PIN_S, S, cn(29)),
    half(K::Pinp, "Connector plug pin", PIN_P, P, cn(30)),
    // ---- Brief 034 stage 3b: PLC card bodies (design 034 §6); channels join as `di`/`do`.
    card(K::Plccpu, "PLC CPU", &["plc", "cpu", "controller", "card"], pc(1)),
    card(K::Plcps, "PLC power supply", &["plc", "power", "card"], pc(2)),
    card(K::Plcdi, "PLC digital input card", &["plc", "di", "input", "card"], pc(3)).quiet_at(Quiet::All),
    card(K::Plcdq, "PLC digital output card", &["plc", "dq", "do", "output", "card"], pc(4)).quiet_at(Quiet::All),
    card(K::Plcai, "PLC analogue input card", &["plc", "ai", "analogue", "card"], pc(5)).quiet_at(Quiet::All),
    card(K::Plcao, "PLC analogue output card", &["plc", "ao", "analogue", "card"], pc(6)).quiet_at(Quiet::All),
    card(K::Plcrtd, "PLC RTD card", &["plc", "rtd", "temperature", "card"], pc(7)).quiet_at(Quiet::All),
    card(K::Plciol, "PLC IO-Link master", &["plc", "io-link", "iol", "card"], pc(8)).quiet_at(Quiet::All),
    // ---- Brief 036: multi-level terminals (design 036 §3.1). Geometry, points and bridges come from
    // `terminal_block_types`; `termfuse` shares `fuseterm`'s drawing.
    lvl(K::Term2L1, "Two-level terminal, level 1", TWO, mt(1)),
    lvl(K::Term2L2, "Two-level terminal, level 2", TWO, mt(2)),
    lvl(K::Term2, "Two-level terminal", TWO, mt(3)),
    lvl(K::Term2bL1, "Two-level terminal, bridged, level 1", TWO, mt(4)),
    lvl(K::Term2bL2, "Two-level terminal, bridged, level 2", TWO, mt(5)),
    lvl(K::Term2b, "Two-level terminal, bridged", TWO, mt(6)),
    lvl(K::Term3L1, "Three-level terminal, level 1", THREE, mt(7)),
    lvl(K::Term3L2, "Three-level terminal, level 2", THREE, mt(8)),
    lvl(K::Term3L3, "Three-level terminal, level 3", THREE, mt(9)),
    lvl(K::Term3, "Three-level terminal", THREE, mt(10)),
    lvl(K::TermlnpeL1, "L/N/PE terminal, level L", LNPE, mt(11)),
    lvl(K::TermlnpeL2, "L/N/PE terminal, level N", LNPE, mt(12)),
    lvl(K::TermlnpeL3, "L/N/PE terminal, level PE", LNPE, mt(13)),
    lvl(K::Termlnpe, "L/N/PE terminal", LNPE, mt(14)),
    lvl(K::TermsensorL1, "Sensor terminal, level S", SENSOR, mt(15)),
    lvl(K::TermsensorL2, "Sensor terminal, level +", SENSOR, mt(16)),
    lvl(K::TermsensorL3, "Sensor terminal, level -", SENSOR, mt(17)),
    lvl(K::Termsensor, "Sensor terminal", SENSOR, mt(18)),
    lvl(K::Termdisc, "Disconnect terminal", &["disconnect", "knife", "isolating", "terminal"], mt(19)),
    lvl(K::Termfuse, "Fuse terminal (strip position)", &["fuse terminal", "fuse", "terminal"], mt(20)),
    // ---- Brief 045 (design 045 §3.1): a head module (a rack of its own), a power module and a
    // communication module, as 034's card bodies.
    card(K::Plcim, "PLC head module", &["plc", "im", "interface", "head", "remote", "station", "card"], pc(9)),
    card(K::Plcpm, "PLC power module", &["plc", "pm", "power", "card"], pc(10)),
    card(K::Plccm, "PLC communication module", &["plc", "cm", "communication", "fieldbus", "card"], pc(11)).quiet_at(Quiet::All),
    // ---- Brief 045 (design 045 §4.1): one channel cell per card type with channels; cells side
    // by side read as the card. Every point is quiet (a spare channel is normal).
    cell(K::Chdi, "PLC digital input channel", &["plc", "di", "input", "channel"], CT::Di, pc(12)),
    cell(K::Chdq, "PLC digital output channel", &["plc", "dq", "do", "output", "channel"], CT::Dq, pc(13)),
    cell(K::Chai, "PLC analogue input channel", &["plc", "ai", "analogue", "channel"], CT::Ai, pc(14)),
    cell(K::Chao, "PLC analogue output channel", &["plc", "ao", "analogue", "channel"], CT::Ao, pc(15)),
    cell(K::Chrtd, "PLC RTD channel", &["plc", "rtd", "temperature", "channel"], CT::Rtd, pc(16)),
    cell(K::Chiol, "PLC IO-Link port", &["plc", "io-link", "iol", "port", "channel"], CT::Iol, pc(17)),
];

use crate::plc::rack::CardType as CT;

/// A PLC channel cell (brief 045): `Kind::Channel`, every point quiet.
const fn cell(key: StarterKey, name: &'static str, aliases: &'static [&'static str], t: CT, id: DefinitionId) -> Starter {
    st(key, name, aliases, Kind::Channel(t), id).of(Family::Plc).quiet_at(Quiet::All)
}

/// A cell's width and its points (role, x) (design 045 §4.1; brief 051, design 051 §2.2: the
/// pack's pitch, 20 mm per single-point channel).
pub fn cell_shape(key: StarterKey) -> Option<(f64, &'static [(&'static str, f64)])> {
    Some(match key {
        K::Chdi => (20.0, &[("in", 0.0)]),
        K::Chdq => (20.0, &[("out", 0.0)]),
        K::Chai | K::Chao => (20.0, &[("+", -5.0), ("-", 5.0)]),
        K::Chrtd => (40.0, &[("I+", -15.0), ("S+", -5.0), ("S-", 5.0), ("I-", 15.0)]),
        K::Chiol => (30.0, &[("L+", -10.0), ("C/Q", 0.0), ("L-", 10.0)]),
        K::Coil
        | K::No
        | K::Nc
        | K::Power3
        | K::Mcb1
        | K::Mcb2
        | K::Fuse
        | K::Psu
        | K::Pbno
        | K::Pbnc
        | K::Sel
        | K::Lamp
        | K::Motor3
        | K::Motor1
        | K::Trafo
        | K::TerminalBlock
        | K::TerminalBlockPe
        | K::Di
        | K::Do
        | K::Continuation
        | K::Co
        | K::Swco
        | K::Power4
        | K::Rcbo2
        | K::Rcd2
        | K::Rcd4
        | K::Ssrin
        | K::Ssrout
        | K::Fuseterm
        | K::Fusebank4
        | K::Fusebank6
        | K::Fusebank8
        | K::Fusebank10
        | K::Fusebank12
        | K::Prox3
        | K::Floatno
        | K::Floatnc
        | K::Rollerno
        | K::Rollernc
        | K::Interlocknc
        | K::Interlockno
        | K::Valve
        | K::Valve2
        | K::Tc
        | K::Heater
        | K::Socket2
        | K::Socket3
        | K::Socket4
        | K::Socket6
        | K::Socket8
        | K::Socket12
        | K::Plug2
        | K::Plug3
        | K::Plug4
        | K::Plug6
        | K::Plug8
        | K::Plug12
        | K::Dsub9s
        | K::Dsub15s
        | K::Dsub25s
        | K::Dsub9p
        | K::Dsub15p
        | K::Dsub25p
        | K::M12s4
        | K::M12s5
        | K::M12s8
        | K::M12p4
        | K::M12p5
        | K::M12p8
        | K::Rj45
        | K::Usba
        | K::Usbb
        | K::Pinpair
        | K::Pins
        | K::Pinp
        | K::Plccpu
        | K::Plcps
        | K::Plcdi
        | K::Plcdq
        | K::Plcai
        | K::Plcao
        | K::Plcrtd
        | K::Plciol
        | K::Term2L1
        | K::Term2L2
        | K::Term2
        | K::Term2bL1
        | K::Term2bL2
        | K::Term2b
        | K::Term3L1
        | K::Term3L2
        | K::Term3L3
        | K::Term3
        | K::TermlnpeL1
        | K::TermlnpeL2
        | K::TermlnpeL3
        | K::Termlnpe
        | K::TermsensorL1
        | K::TermsensorL2
        | K::TermsensorL3
        | K::Termsensor
        | K::Termdisc
        | K::Termfuse
        | K::Plcim
        | K::Plcpm
        | K::Plccm => return None,
    })
}

/// The line of a cell's points: its top edge (design 051 §2.2), local y.
pub const CELL_TOP: f64 = 5.0;
/// The header band under the top edge (the dividers), mm.
pub const HEADER_BAND: f64 = 10.0;
/// The mark band under the header, mm; a cell's mark sits at its centre.
pub const MARK_BAND: f64 = 15.0;
/// The name band at the built-in text styles, mm (design 051 §2.5: at least 12.5).
pub const NAME_BAND: f64 = 12.5;
/// A channel text's end under the frame's bottom edge, mm.
pub const OUTSIDE_GAP: f64 = 2.0;
/// The frame's depth under the top edge at the built-in text styles: header, mark and name bands.
pub const FRAME_DEPTH: f64 = HEADER_BAND + MARK_BAND + NAME_BAND;
/// The height of a role name drawn under its pin, mm (`point_label`'s built-in).
const ROLE_H: f64 = 1.5;

/// A cell's graphics (brief 051, design 051 §2.2): its share of the card's top edge through its
/// points, its two dividers down the header band (adjacent cells draw theirs on one line), the role
/// names under the pins of a multi-point cell, and its mark at the centre of the mark band. No
/// bottom edge and no leads: the card's frame closes the box.
fn cell_graphics(key: StarterKey) -> Vec<Primitive> {
    let Some((w, points)) = cell_shape(key) else { return Vec::new() };
    let h = w / 2.0;
    let div = CELL_TOP - HEADER_BAND;
    let mut g = vec![line((-h, CELL_TOP), (h, CELL_TOP)), line((-h, CELL_TOP), (-h, div)), line((h, CELL_TOP), (h, div))];
    if points.len() > 1 {
        for (role, x) in points {
            // Under the pin, which 031's rule puts right of the point, 2.5 under the edge.
            g.push(text((*x + 1.0, CELL_TOP - 6.5), ROLE_H, role));
        }
    }
    // The marks: new drawings for ECAD (034 §2.1's rules: mm, no arcs).
    let c = CELL_TOP - HEADER_BAND - MARK_BAND / 2.0;
    let into = |dy: f64| poly(&[(-1.5, c + 1.5 + dy), (0.0, c - 1.5 + dy), (1.5, c + 1.5 + dy)], false);
    let out = |dy: f64| poly(&[(-1.5, c - 1.5 + dy), (0.0, c + 1.5 + dy), (1.5, c - 1.5 + dy)], false);
    let tilde = poly(&[(-3.0, c - 2.0), (-1.5, c - 1.0), (0.0, c - 2.0), (1.5, c - 3.0), (3.0, c - 2.0)], false);
    match key {
        K::Chdi => g.push(into(0.0)),
        K::Chdq => g.push(out(0.0)),
        K::Chai => g.extend([into(1.5), tilde]),
        K::Chao => g.extend([out(1.5), tilde]),
        // A small resistor with a slash.
        K::Chrtd => g.extend([rect((-2.0, c - 1.0), (2.0, c + 1.0)), line((-2.5, c - 1.5), (2.5, c + 1.5))]),
        // A small diamond.
        K::Chiol => g.push(poly(&[(0.0, c + 1.5), (1.5, c), (0.0, c - 1.5), (-1.5, c)], true)),
        K::Coil
        | K::No
        | K::Nc
        | K::Power3
        | K::Mcb1
        | K::Mcb2
        | K::Fuse
        | K::Psu
        | K::Pbno
        | K::Pbnc
        | K::Sel
        | K::Lamp
        | K::Motor3
        | K::Motor1
        | K::Trafo
        | K::TerminalBlock
        | K::TerminalBlockPe
        | K::Di
        | K::Do
        | K::Continuation
        | K::Co
        | K::Swco
        | K::Power4
        | K::Rcbo2
        | K::Rcd2
        | K::Rcd4
        | K::Ssrin
        | K::Ssrout
        | K::Fuseterm
        | K::Fusebank4
        | K::Fusebank6
        | K::Fusebank8
        | K::Fusebank10
        | K::Fusebank12
        | K::Prox3
        | K::Floatno
        | K::Floatnc
        | K::Rollerno
        | K::Rollernc
        | K::Interlocknc
        | K::Interlockno
        | K::Valve
        | K::Valve2
        | K::Tc
        | K::Heater
        | K::Socket2
        | K::Socket3
        | K::Socket4
        | K::Socket6
        | K::Socket8
        | K::Socket12
        | K::Plug2
        | K::Plug3
        | K::Plug4
        | K::Plug6
        | K::Plug8
        | K::Plug12
        | K::Dsub9s
        | K::Dsub15s
        | K::Dsub25s
        | K::Dsub9p
        | K::Dsub15p
        | K::Dsub25p
        | K::M12s4
        | K::M12s5
        | K::M12s8
        | K::M12p4
        | K::M12p5
        | K::M12p8
        | K::Rj45
        | K::Usba
        | K::Usbb
        | K::Pinpair
        | K::Pins
        | K::Pinp
        | K::Plccpu
        | K::Plcps
        | K::Plcdi
        | K::Plcdq
        | K::Plcai
        | K::Plcao
        | K::Plcrtd
        | K::Plciol
        | K::Term2L1
        | K::Term2L2
        | K::Term2
        | K::Term2bL1
        | K::Term2bL2
        | K::Term2b
        | K::Term3L1
        | K::Term3L2
        | K::Term3L3
        | K::Term3
        | K::TermlnpeL1
        | K::TermlnpeL2
        | K::TermlnpeL3
        | K::Termlnpe
        | K::TermsensorL1
        | K::TermsensorL2
        | K::TermsensorL3
        | K::Termsensor
        | K::Termdisc
        | K::Termfuse
        | K::Plcim
        | K::Plcpm
        | K::Plccm => {}
    }
    g
}

// Brief 036's aliases.
const TWO: &[&str] = &["two-level", "double-deck", "stacked", "terminal"];
const THREE: &[&str] = &["three-level", "stacked", "terminal"];
const LNPE: &[&str] = &["installation terminal", "l/n/pe", "stacked", "terminal"];
const SENSOR: &[&str] = &["sensor terminal", "three-level", "stacked", "terminal"];

/// Multi-level terminal starters (brief 036): `0x500 + n`.
const fn mt(n: u128) -> DefinitionId {
    def_id(0x500 + n)
}

/// A multi-level terminal starter (brief 036): joins a strip as a level.
const fn lvl(key: StarterKey, name: &'static str, aliases: &'static [&'static str], id: DefinitionId) -> Starter {
    st(key, name, aliases, Kind::TerminalBlock, id)
}

/// The starter symbol with key `key`.
///
/// # Semantics
/// [`STARTERS`] holds one row per [`StarterKey`] in variant order, checked when the crate
/// compiles ([`KEYED_IN_ORDER`]), so the row is the one at the key's index.
pub fn starter(key: StarterKey) -> &'static Starter {
    &STARTERS[key as usize]
}

/// Compile-time check that row `i` of [`STARTERS`] is keyed by variant `i`, and that there is a row
/// for every variant.
const KEYED_IN_ORDER: () = {
    assert!(STARTERS.len() == <StarterKey as strum::VariantArray>::VARIANTS.len(), "one STARTERS row per StarterKey");
    let mut i = 0;
    while i < STARTERS.len() {
        assert!(STARTERS[i].key as usize == i, "STARTERS rows follow StarterKey's order");
        i += 1;
    }
};
const _: () = KEYED_IN_ORDER;

/// The starter symbol a key word names (case-insensitive, trimmed): for a word someone typed or a
/// file holds. Code that knows which starter it means uses [`starter`] with its [`StarterKey`].
pub fn by_key(key: &str) -> Option<&'static Starter> {
    <StarterKey as ecad_model::Word>::parse(key).map(starter)
}

/// The starter symbol an embedded definition was made from, by its origin.
pub fn of_definition(d: &Definition) -> Option<&'static Starter> {
    let o = d.origin.as_ref()?;
    if o.library != STARTER_LIBRARY {
        return None;
    }
    STARTERS.iter().find(|s| s.id == o.definition)
}

/// The project's embedded copy of `s` as this catalogue builds it, found by origin: library,
/// definition **and content hash** (brief 044, FP3), so a project holding an older version of a
/// starter embeds the current one beside it for new placements.
pub fn embedded(doc: &Document, s: &Starter) -> Option<DefinitionId> {
    let hash = s.definition().ok()?.origin.as_ref()?.content_hash;
    doc.definitions
        .iter()
        .find(|(_, d)| d.origin.as_ref().is_some_and(|o| o.library == STARTER_LIBRARY && o.definition == s.id && o.content_hash == hash))
        .map(|(id, _)| *id)
}

/// The kind of a stored definition: its starter's, except that a connector half embedded before
/// brief 044 (no side bridges, version 1) is the exclusive head it was then (design 044 §3.1).
/// A project symbol (brief 048, design §5.2): an edited starter keeps its starter's kind; any
/// other is a device of its own (`join: device`) or part of a named device (`join: part`), with
/// fixed labels.
pub fn kind_of_definition(d: &Definition) -> Option<Kind> {
    if is_project_symbol(d) {
        if let Some(k) = d.derived_from.as_ref().and_then(|o| starter_of_origin(o)).map(|s| s.kind) {
            return Some(k);
        }
        return Some(Kind::Fixed { exclusive: join_of(d) != Join::Part });
    }
    let s = of_definition(d)?;
    Some(match s.kind {
        Kind::ConnectorHalf { .. } if !is_mating(d) => HEAD,
        k @ (Kind::Contact { .. }
        | Kind::TerminalBlock
        | Kind::Channel(_)
        | Kind::ChangeOver
        | Kind::Fixed { .. }
        | Kind::Tagless
        | Kind::ConnectorHalf { .. }) => k,
    })
}

// ---------------------------------------------------------------- project symbols (brief 048)

/// The library ID of symbols made in a project (design 048 §2.2, FP3): one fixed ID, so a project
/// symbol pasted into another project stays a project symbol there.
pub const PROJECT_SYMBOLS: LibraryId = LibraryId(0x0000_ecad_0048_5ea1_0000_0000_0000_0001);

/// Most characters of a project symbol's key (design 048 §2.4).
pub const MAX_KEY: usize = 16;

/// How a project symbol joins a device (design 048 §5.2).
pub use ecad_model::Join;

/// Whether `d` is a symbol made or edited in a project (its origin is [`PROJECT_SYMBOLS`]).
pub fn is_project_symbol(d: &Definition) -> bool {
    d.origin.as_ref().is_some_and(|o| o.library == PROJECT_SYMBOLS)
}

/// The part a symbol of kind `k` plays in its device (design 072 §4.4), with no poles of its own.
fn part_of_kind(k: Kind) -> PartRole {
    match k {
        Kind::Fixed { exclusive: true } => PartRole::Head,
        Kind::Contact { .. } | Kind::ChangeOver => PartRole::Contact,
        Kind::Fixed { exclusive: false } => PartRole::Pole,
        Kind::TerminalBlock => PartRole::TerminalBlock,
        Kind::Channel(_) => PartRole::Channel,
        Kind::ConnectorHalf { .. } => PartRole::Half,
        Kind::Tagless => PartRole::Other,
    }
}

/// A project symbol's part in its device (design 072 §4.4, F3′), declared when the symbol is
/// made: its derived starter's kind, else a head unless its defaults join it as a part (design 048
/// §5.2). A project symbol carries no poles of its own.
pub fn project_part_role(d: &Definition) -> PartRole {
    let kind = match d.derived_from.as_ref().and_then(starter_of_origin) {
        Some(s) => s.kind,
        None => Kind::Fixed { exclusive: join_of(d) != Join::Part },
    };
    part_of_kind(kind)
}

/// How project symbol `d` joins a device: `part` only when its defaults say so.
pub fn join_of(d: &Definition) -> Join {
    d.defaults.join.unwrap_or_default()
}

/// The starter an origin names (by library and ID).
pub fn starter_of_origin(o: &Origin) -> Option<&'static Starter> {
    (o.library == STARTER_LIBRARY).then(|| STARTERS.iter().find(|s| s.id == o.definition)).flatten()
}

/// Whether `key` is a valid symbol key (design 048 §2.4): 1 to [`MAX_KEY`] letters, digits, `-`
/// and `_`, starting with a letter or digit.
pub fn valid_key(key: &str) -> bool {
    (1..=MAX_KEY).contains(&key.len())
        && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        && key.bytes().next().is_some_and(|b| b.is_ascii_alphanumeric())
}

/// One embedded version of a project symbol.
#[derive(Debug, Clone, Copy)]
pub struct Version<'a> {
    pub id: DefinitionId,
    pub def: &'a Definition,
    /// Its lineage (`origin.definition`).
    pub lineage: DefinitionId,
    /// Its version (`origin.library_seq`).
    pub seq: u64,
}

impl<'a> Version<'a> {
    /// `def` as a project-symbol version, if it is one.
    pub fn of(id: DefinitionId, def: &'a Definition) -> Option<Self> {
        let o = def.origin.as_ref().filter(|o| o.library == PROJECT_SYMBOLS)?;
        Some(Version { id, def, lineage: o.definition, seq: o.library_seq })
    }

    /// Its key (`defaults.key`), if it has one.
    pub fn key(&self) -> Option<&'a str> {
        self.def.defaults.key.as_ref().map(|k| k.as_str())
    }
}

/// Every embedded project-symbol version (a pass over the project's definitions, which are few: the
/// symbols it uses, not its objects).
pub fn project_versions(doc: &Document) -> impl Iterator<Item = Version<'_>> {
    doc.definitions.iter().filter_map(|(id, d)| Version::of(*id, d))
}

/// The versions of `lineage`, oldest first.
pub fn versions_of(doc: &Document, lineage: DefinitionId) -> Vec<Version<'_>> {
    let mut v: Vec<Version<'_>> = project_versions(doc).filter(|x| x.lineage == lineage).collect();
    v.sort_by_key(|x| (x.seq, x.id));
    v
}

/// The newest version of `lineage` (highest version; the higher ID on a tie, from a merge).
pub fn newest(doc: &Document, lineage: DefinitionId) -> Option<Version<'_>> {
    versions_of(doc, lineage).pop()
}

/// The newest version of every lineage, by name then lineage.
pub fn project_symbols(doc: &Document) -> Vec<Version<'_>> {
    let mut by: BTreeMap<DefinitionId, Version<'_>> = BTreeMap::new();
    for v in project_versions(doc) {
        match by.get(&v.lineage) {
            Some(cur) if (cur.seq, cur.id) >= (v.seq, v.id) => {}
            _ => {
                by.insert(v.lineage, v);
            }
        }
    }
    let mut out: Vec<Version<'_>> = by.into_values().collect();
    out.sort_by(|a, b| a.def.name.cmp(&b.def.name).then(a.lineage.cmp(&b.lineage)));
    out
}

/// The project symbol whose newest version has key `key` (case-insensitive), as that version.
pub fn project_by_key<'a>(doc: &'a Document, key: &str) -> Option<Version<'a>> {
    let k = key.trim();
    project_symbols(doc).into_iter().find(|v| v.key().is_some_and(|x| x.eq_ignore_ascii_case(k)))
}

/// Whether some version of a project symbol has key `key` (case-insensitive).
pub fn has_project_key(doc: &Document, key: &str) -> bool {
    let k = key.trim();
    project_versions(doc).any(|v| v.key().is_some_and(|x| x.eq_ignore_ascii_case(k)))
}

/// The op that keeps the project's tag prefixes on project symbols (design 072 F5) true after
/// `ops`: a prefix whose key no version holds any more moves to the new key of the symbol that had
/// it (a rename), or is cleared (its versions removed). `None` when nothing changes.
///
/// # Semantics
/// Reads only the project's symbol versions and `ops`' `EmbedDefinition`/`RemoveDefinition`, so
/// it goes last in the plan. A rename onto a key that already has a prefix keeps that one.
pub fn prefix_follow(doc: &Document, ops: &[Op]) -> Option<Op> {
    use ecad_model::naming::prefix::PrefixOf;
    let removed: BTreeSet<DefinitionId> = ops.iter().filter_map(|o| if let Op::RemoveDefinition { id, .. } = o { Some(*id) } else { None }).collect();
    let embedded: Vec<Version<'_>> =
        ops.iter().filter_map(|o| if let Op::EmbedDefinition { id, definition } = o { Version::of(*id, definition) } else { None }).collect();
    let lower = |k: &str| k.trim().to_ascii_lowercase();
    let mut after: BTreeSet<String> = project_versions(doc).filter(|v| !removed.contains(&v.id)).filter_map(|v| v.key().map(lower)).collect();
    after.extend(embedded.iter().filter_map(|v| v.key().map(lower)));
    let before = doc.settings.clone();
    let mut next = before.clone();
    for (of, prefix) in &before.tag_prefixes {
        let PrefixOf::Project(k) = of else { continue };
        if after.contains(&lower(k)) {
            continue;
        }
        next.tag_prefixes.remove(of);
        let lineages: BTreeSet<DefinitionId> =
            project_versions(doc).filter(|v| v.key().is_some_and(|x| x.eq_ignore_ascii_case(k.trim()))).map(|v| v.lineage).collect();
        let renamed = embedded.iter().filter(|v| lineages.contains(&v.lineage)).max_by_key(|v| v.seq).and_then(|v| v.key());
        if let Some(new) = renamed.filter(|k| PrefixOf::project_form(k)).map(|k| PrefixOf::Project(k.into()))
            && !before.tag_prefixes.contains_key(&new)
            && !next.tag_prefixes.contains_key(&new)
        {
            next.tag_prefixes.insert(new, prefix.clone());
        }
    }
    (next != before).then_some(Op::SetProjectSettings { before, after: next })
}

/// The key a symbol's registry entries use: a starter's catalogue key, else a project symbol's own.
pub fn registry_key(d: &Definition) -> Option<String> {
    if let Some(s) = of_definition(d) {
        return Some(s.key.to_string());
    }
    d.defaults.key.as_ref().map(|k| k.to_string())
}

/// Whether `d` is a brief-044 connector half or pin: a connector starter whose bridges join the
/// side keys.
pub fn is_mating(d: &Definition) -> bool {
    of_definition(d).is_some_and(|s| matches!(s.kind, Kind::ConnectorHalf { .. }))
        && d.bridges.iter().any(|g| g.contains(&SOCKET_KEY) && g.contains(&PLUG_KEY))
}

/// Whether bridge key `k` of definition `d` is one a placement can write (overseer condition on
/// FP2): one of the definition's own terminal keys, a side key on a connector half, or a level
/// key of its multi-level terminal type.
pub fn bridge_key_ok(d: &Definition, k: &PointKey) -> bool {
    if d.connection_points.iter().any(|t| t.key == *k) {
        return true;
    }
    match of_definition(d) {
        Some(s) if matches!(s.kind, Kind::ConnectorHalf { .. }) => *k == SOCKET_KEY || *k == PLUG_KEY,
        Some(s) => crate::terminal_strips::terminal_blocks::of_starter(s.key).is_some_and(|(t, _)| t.bridges.iter().any(|g| g.contains(k))),
        None => false,
    }
}

/// Most symbols one search lists.
pub const MAX_SEARCH: usize = 500;

/// Most characters of a search query that are used.
pub const MAX_QUERY: usize = 60;

/// The symbols matching `query` (design 018 §5.1): a substring of the key, name or an alias, or the
/// tag prefix with or without `-`. An empty query lists every symbol, in table order.
pub fn search(query: &str) -> Vec<&'static Starter> {
    let q: String = query.trim().chars().take(MAX_QUERY).collect::<String>().to_lowercase();
    if q.is_empty() {
        return STARTERS.iter().collect();
    }
    let bare = q.trim_start_matches('-');
    STARTERS
        .iter()
        .filter(|s| {
            s.key.word() == q
                || s.name.to_lowercase().contains(&q)
                || (!s.family.word().is_empty() && s.family.word().contains(q.as_str()))
                || s.aliases.iter().any(|a| a.contains(q.as_str()))
                || (!bare.is_empty() && s.prefix().eq_ignore_ascii_case(bare))
        })
        .collect()
}

// ---------------------------------------------------------------- quiet points (design 034 §11)
//
// Overseer ruling (2026-11-12): labels, quiet points and anchors are catalogue code; only
// user-overridable values (the prefix) are in the defaults registry, `defaults/symbols.rs`.

/// Whether point `point` (a definition key) of `s` is quiet: left unwired, it raises no
/// `Unconnected` check.
pub fn is_quiet(s: &Starter, point: &str) -> bool {
    match s.quiet {
        Quiet::None => false,
        Quiet::All => true,
        Quiet::AllBut(loud) => !loud.contains(&point),
    }
}

// ---------------------------------------------------------------- change-over contacts

/// The labels a change-over contact gets joining a device whose labels are `used` (design 034
/// §2.3): s = 1 + the highest IEC sequence digit among labels of the form `[1-9][1-4]`, then `s1`,
/// `s2`, `s4` (common, NC, NO: definition order).
///
/// # Errors
/// s would be over 9: `<who> already has 9 contacts.`
pub fn changeover_labels(used: &BTreeSet<String>, who: &str) -> Result<Vec<String>, CmdError> {
    sequence_labels(used, &BTreeSet::new(), &['1', '2', '4'], who)
}

/// The labels a contact gets joining a device (design 018 §3.1, 034 §2.3): `suffixes` after the
/// next IEC sequence digit s, in that order (`['3', '4']` NO, `['1', '2']` NC, `['1', '2', '4']`
/// change-over).
///
/// # Semantics
/// s = 1 + the highest digit among the labels of `used` of the form `[1-9][1-4]` that are not in
/// `skip`, then the first digit from there whose labels `used` does not already hold. `skip` is a
/// fuse bank's own outputs ([`bank_outputs`]): fuse `11` or `12` is not a contact, so a fault
/// contact on a 12-way bank is still `13`/`14` (design 034 §3), and an NC skips to `21`/`22`.
///
/// # Errors
/// s would be over 9: `<who> already has 9 contacts.`
pub fn sequence_labels(used: &BTreeSet<String>, skip: &BTreeSet<String>, suffixes: &[char], who: &str) -> Result<Vec<String>, CmdError> {
    let highest = used
        .iter()
        .filter(|l| !skip.contains(*l))
        .filter_map(|l| match l.as_bytes() {
            [s @ b'1'..=b'9', b'1'..=b'4'] => Some(u32::from(s - b'0')),
            _ => None,
        })
        .max()
        .unwrap_or(0);
    let labels = |s: u32| suffixes.iter().map(|x| format!("{s}{x}")).collect::<Vec<_>>();
    // At most nine digits are tried.
    (highest + 1..=9)
        .map(labels)
        .find(|l| !l.iter().any(|x| used.contains(x)))
        .ok_or_else(|| CmdError::refused(format!("{who} already has 9 contacts.")))
}

/// The output labels of device `c`'s fuse bank (its head), which are not contact labels; empty
/// when its head is not a fuse bank.
pub fn bank_outputs(doc: &Document, c: ecad_model::DeviceId) -> BTreeSet<String> {
    doc.representations_of(Subject::Device(c))
        .filter_map(|(_, r)| crate::devices::rep_starter(doc, r))
        .find(|s| s.kind == Kind::Fixed { exclusive: true })
        .filter(|s| is_fuse_bank(s))
        .map(|s| s.labels().into_iter().filter(|l| ecad_model::naming::words::is_digits(l)).collect())
        .unwrap_or_default()
}

// ---------------------------------------------------------------- ratings (design 034 §7.3)

pub use ecad_model::objects::devices::{RATING_KEY, RATING_OF_PREFIX};

/// Most characters of `<n>` in `rating.<n>` that are read; longer is refused without parsing it
/// (a hostile key cannot make a lookup or a message long).
pub const MAX_FUSE_LABEL: usize = 12;

/// Whether `key` is `rating` or `rating.<n>`.
pub fn is_rating_key(key: &str) -> bool {
    key == RATING_KEY || key.starts_with(RATING_OF_PREFIX)
}

/// Whether `s` is a fuse bank (its fuses take `rating.<n>`).
pub fn is_fuse_bank(s: &Starter) -> bool {
    s.key.fuse_bank_ways().is_some()
}

/// Check a `rating.<n>` value set on `object` that is neither a protective device nor a terminal
/// strip (design 034 §7.3; a device's own rating, a fuse bank's fuses and a strip's fuse positions
/// are typed fields, `cmd::props::b`).
///
/// # Semantics
/// `rating.<n>` on such a device is refused, and clearing it is always allowed. Any other key, or
/// an object that is not a device: no opinion, the value comes back unchanged.
///
/// # Errors
/// `rating.<n>` set on a device that is not a fuse bank or a strip.
pub fn setprop_rule(doc: &Document, object: ObjectRef, key: &str, value: Option<&str>) -> Result<Option<String>, CmdError> {
    let ObjectRef::Device(c) = object else { return Ok(value.map(str::to_string)) };
    if !is_rating_key(key) {
        return Ok(value.map(str::to_string));
    }
    // Clearing always works (review of PR #63, 2): a `rating.<n>` left on a device that is no
    // longer a fuse bank (its bank symbol erased, or an older free property) must stay removable.
    let clearing = value.map(str::trim).is_none_or(str::is_empty);
    if let Some(n) = key.strip_prefix(RATING_OF_PREFIX).filter(|_| !clearing) {
        let who = crate::devices::tag_words(doc, c);
        // Capped: only the first 12 characters are ever shown.
        let shown: String = n.chars().take(MAX_FUSE_LABEL).collect();
        let more = if n.chars().nth(MAX_FUSE_LABEL).is_some() { "…" } else { "" };
        return Err(CmdError::refused(format!("rating.{shown}{more} is for a fuse bank's fuses; {who} is not a fuse bank.")));
    }
    crate::devices::property_value(doc, key, value)
}

// ---------------------------------------------------------------- text anchors (design 034 §10)

/// What a text anchor is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextRole {
    Tag,
    Description,
    Rating,
    /// The rating of a fuse bank's fuse, by its label (`rating.<n>`).
    RatingOf(String),
    /// Brief 045 (design 045 §7.2): a card body's caption (`-A01 slot 2`), under the body.
    Caption,
    /// Brief 045 (design 045 §7.1): a channel cell's channel number, address and description.
    ChannelNo,
    Address,
    ChannelText,
}

/// Horizontal alignment of text at its anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HAlign {
    Left,
    Centre,
    Right,
}

/// Vertical alignment of text at its anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VAlign {
    Bottom,
    Middle,
    Top,
}

/// The first place lane T tries for one text of a symbol, in the symbol's local frame (it rotates
/// and mirrors with the symbol); `rotation` in degrees.
#[derive(Debug, Clone, PartialEq)]
pub struct Anchor {
    pub role: TextRole,
    pub at: Pt,
    pub h: HAlign,
    pub v: VAlign,
    pub rotation: f64,
}

/// The gap between a symbol's box and its text, and one line's step, in millimetres (design 034
/// §10). Geometry of the anchor rule, not a text size: lane T decides sizes.
const ANCHOR_GAP: f64 = 2.0;
const ANCHOR_LINE: f64 = 4.0;

/// The text anchors of `s` (design 034 §10): the default rule from its box, with the overrides of
/// fuse banks, RCBO and RCDs, card bodies and connectors. A tagless symbol has none.
///
/// # Semantics
/// Default rule, box (lo, hi), mid = (lo.y + hi.y)/2: Tag at (lo.x − 2, mid), right, middle;
/// Description at (lo.x − 2, mid − 4), right, top; Rating at (hi.x + 2, mid), left, middle.
pub fn anchors(s: &Starter) -> Vec<Anchor> {
    if s.kind == Kind::Tagless {
        return Vec::new();
    }
    let Ok(d) = s.definition() else { return Vec::new() };
    let (lo, hi) = bounds(d);
    let mid = (lo.y + hi.y) / 2.0;
    let a = |role: TextRole, x: f64, y: f64, h: HAlign, v: VAlign| Anchor { role, at: p(x, y), h, v, rotation: 0.0 };
    let tag = a(TextRole::Tag, lo.x - ANCHOR_GAP, mid, HAlign::Right, VAlign::Middle);
    let description = a(TextRole::Description, lo.x - ANCHOR_GAP, mid - ANCHOR_LINE, HAlign::Right, VAlign::Top);
    let rating = a(TextRole::Rating, hi.x + ANCHOR_GAP, mid, HAlign::Left, VAlign::Middle);
    let mut out = match s.key {
        // Breakers carry their rating by the tag.
        K::Rcbo2 | K::Rcd2 | K::Rcd4 => vec![
            tag,
            a(TextRole::Rating, lo.x - ANCHOR_GAP, mid - ANCHOR_LINE, HAlign::Right, VAlign::Top),
            a(TextRole::Description, lo.x - ANCHOR_GAP, mid - 2.0 * ANCHOR_LINE, HAlign::Right, VAlign::Top),
        ],
        // Brief 045 (design 045 §7.1): a channel cell's texts in the cell's frame.
        K::Chdi | K::Chdq | K::Chai | K::Chao | K::Chrtd | K::Chiol => {
            let h = cell_shape(s.key).map_or(5.0, |(w, _)| w / 2.0);
            vec![
                tag,
                // Brief 051 (design 051 §2.5): the mark band's top line, and outside the frame.
                a(TextRole::ChannelNo, -h + 1.0, CELL_TOP - HEADER_BAND - 1.0, HAlign::Left, VAlign::Top),
                a(TextRole::Address, 0.0, CELL_TOP - HEADER_BAND - 1.0, HAlign::Centre, VAlign::Top),
                Anchor {
                    role: TextRole::ChannelText,
                    at: p(0.0, CELL_TOP - FRAME_DEPTH - OUTSIDE_GAP),
                    h: HAlign::Right,
                    v: VAlign::Middle,
                    rotation: 90.0,
                },
            ]
        }
        // Card bodies: the tag above, the description inside under the header.
        // Brief 045 (design 045 §7.2): and the caption under the body.
        K::Plccpu | K::Plcps | K::Plcdi | K::Plcdq | K::Plcai | K::Plcao | K::Plcrtd | K::Plciol | K::Plcim | K::Plcpm | K::Plccm => vec![
            a(TextRole::Tag, lo.x, hi.y + ANCHOR_GAP, HAlign::Left, VAlign::Bottom),
            a(TextRole::Description, -13.0, 12.0, HAlign::Left, VAlign::Top),
            rating,
            a(TextRole::Caption, lo.x, lo.y - ANCHOR_GAP, HAlign::Left, VAlign::Top),
        ],
        // Brief 044 (design 044 §3.2): a pin shows its half's tag only, beside it.
        K::Pinpair | K::Pins | K::Pinp => vec![a(TextRole::Tag, -2.5, 0.0, HAlign::Right, VAlign::Middle)],
        K::Socket2
        | K::Socket3
        | K::Socket4
        | K::Socket6
        | K::Socket8
        | K::Socket12
        | K::Plug2
        | K::Plug3
        | K::Plug4
        | K::Plug6
        | K::Plug8
        | K::Plug12
        | K::Dsub9s
        | K::Dsub15s
        | K::Dsub25s
        | K::Dsub9p
        | K::Dsub15p
        | K::Dsub25p
        | K::M12s4
        | K::M12s5
        | K::M12s8
        | K::M12p4
        | K::M12p5
        | K::M12p8
        | K::Rj45
        | K::Usba
        | K::Usbb => vec![tag, description],
        K::Coil
        | K::No
        | K::Nc
        | K::Power3
        | K::Mcb1
        | K::Mcb2
        | K::Fuse
        | K::Psu
        | K::Pbno
        | K::Pbnc
        | K::Sel
        | K::Lamp
        | K::Motor3
        | K::Motor1
        | K::Trafo
        | K::TerminalBlock
        | K::TerminalBlockPe
        | K::Di
        | K::Do
        | K::Continuation
        | K::Co
        | K::Swco
        | K::Power4
        | K::Ssrin
        | K::Ssrout
        | K::Fuseterm
        | K::Fusebank4
        | K::Fusebank6
        | K::Fusebank8
        | K::Fusebank10
        | K::Fusebank12
        | K::Prox3
        | K::Floatno
        | K::Floatnc
        | K::Rollerno
        | K::Rollernc
        | K::Interlocknc
        | K::Interlockno
        | K::Valve
        | K::Valve2
        | K::Tc
        | K::Heater
        | K::Term2L1
        | K::Term2L2
        | K::Term2
        | K::Term2bL1
        | K::Term2bL2
        | K::Term2b
        | K::Term3L1
        | K::Term3L2
        | K::Term3L3
        | K::Term3
        | K::TermlnpeL1
        | K::TermlnpeL2
        | K::TermlnpeL3
        | K::Termlnpe
        | K::TermsensorL1
        | K::TermsensorL2
        | K::TermsensorL3
        | K::Termsensor
        | K::Termdisc
        | K::Termfuse => vec![tag, description, rating],
    };
    if is_fuse_bank(s) {
        for (t, key) in d.connection_points.iter().filter_map(|t| Some((t, t.key.as_def().filter(|k| ecad_model::naming::words::is_digits(k))?))) {
            out.push(Anchor {
                role: TextRole::RatingOf(key.to_string()),
                at: p(t.at.x + 3.0, -8.0),
                h: HAlign::Left,
                v: VAlign::Middle,
                rotation: 90.0,
            });
        }
    }
    out
}

// ---------------------------------------------------------------- geometry

fn p(x: f64, y: f64) -> Pt {
    Pt::new(x, y)
}

fn line(a: (f64, f64), b: (f64, f64)) -> Primitive {
    Primitive::Line { a: p(a.0, a.1), b: p(b.0, b.1) }
}

fn poly(v: &[(f64, f64)], closed: bool) -> Primitive {
    Primitive::Polyline { vertices: v.iter().map(|(x, y)| p(*x, *y)).collect(), closed }
}

fn rect(a: (f64, f64), b: (f64, f64)) -> Primitive {
    poly(&[(a.0, a.1), (b.0, a.1), (b.0, b.1), (a.0, b.1)], true)
}

fn circle(c: (f64, f64), radius: f64) -> Primitive {
    Primitive::Circle { center: p(c.0, c.1), radius }
}

fn text(at: (f64, f64), height: f64, value: &str) -> Primitive {
    Primitive::Text { at: p(at.0, at.1), height, rotation: Radians(0.0), value: value.into(), halign: Default::default(), valign: Default::default() }
}

fn shifted(prims: Vec<Primitive>, dx: f64) -> Vec<Primitive> {
    let m = |q: &Pt| p(q.x + dx, q.y);
    prims
        .into_iter()
        .map(|g| match g {
            Primitive::Line { a, b } => Primitive::Line { a: m(&a), b: m(&b) },
            Primitive::Polyline { vertices, closed } => Primitive::Polyline { vertices: vertices.iter().map(m).collect(), closed },
            Primitive::Circle { center, radius } => Primitive::Circle { center: m(&center), radius },
            other @ (Primitive::Arc { .. } | Primitive::Text { .. } | Primitive::Point { .. }) => other,
        })
        .collect()
}

/// A mechanical link drawn dashed (definitions carry no linetype): 1.5 mm dashes, 1 mm gaps.
fn dashed(x0: f64, x1: f64, y: f64) -> Vec<Primitive> {
    let mut out = Vec::new();
    let mut x = x0;
    // At most a few dozen dashes for the widths used here.
    while x < x1 && out.len() < 64 {
        out.push(line((x, y), ((x + 1.5).min(x1), y)));
        x += 2.5;
    }
    out
}

/// A vertical mechanical link drawn dashed, as [`dashed`] along y (design 034 §2.1).
fn dashed_v(x: f64, y0: f64, y1: f64) -> Vec<Primitive> {
    let mut out = Vec::new();
    let mut y = y0;
    while y < y1 && out.len() < 64 {
        out.push(line((x, y), (x, (y + 1.5).min(y1))));
        y += 2.5;
    }
    out
}

/// A closed ellipse of `n` segments (no arcs in definitions, design 034 §2.1).
fn ellipse(c: (f64, f64), rx: f64, ry: f64, n: usize) -> Primitive {
    let v: Vec<(f64, f64)> = (0..n)
        .map(|k| {
            let a = std::f64::consts::TAU * k as f64 / n as f64;
            (c.0 + rx * a.cos(), c.1 + ry * a.sin())
        })
        .collect();
    poly(&v, true)
}

/// A change-over contact, common on top (design 034 §3).
fn contact_co() -> Vec<Primitive> {
    vec![
        line((0.0, 10.0), (0.0, 4.0)),
        line((0.0, -10.0), (0.0, -4.0)),
        line((5.0, -10.0), (5.0, -4.0)),
        line((5.0, -4.0), (3.0, -4.0)),
        line((0.0, 4.0), (4.0, -5.5)),
    ]
}

/// A residual-current toroid and release, the release `dx` right of the RCBO's (design 034 §3).
fn rcd_release(toroid_x: f64, half_width: f64, dx: f64, link_from: f64) -> Vec<Primitive> {
    let mut g =
        vec![ellipse((toroid_x, -7.0), half_width, 1.5, 12), line((13.0 + dx, -7.0), (16.0 + dx, -7.0)), rect((16.0 + dx, -8.5), (19.0 + dx, -5.5))];
    g.extend(dashed_v(17.5 + dx, -5.5, 0.5));
    g.extend(dashed(link_from, 17.5 + dx, 0.5));
    g
}

/// A key for an interlock switch, at the left of its dashed link (design 034 §4).
fn key_mark() -> Vec<Primitive> {
    vec![circle((-12.0, 0.5), 1.0), line((-11.0, 0.5), (-8.0, 0.5)), line((-9.0, 0.5), (-9.0, -0.5))]
}

/// A contact (`make`: NO) operated through a dashed link by a lever ending at `lever_to` with a
/// circle of radius `r` at `mark_y` (float and roller switches, design 034 §4).
fn levered(make: bool, lever_to: f64, r: f64, mark_y: f64) -> Vec<Primitive> {
    let x = if make { -8.0 } else { -6.0 };
    let mut g = if make { contact_no() } else { contact_nc() };
    g.extend(if make { dashed(-8.0, -2.0, 0.5) } else { dashed(-6.0, 1.9, 0.5) });
    g.push(line((x, 0.5), (x, lever_to)));
    g.push(circle((x, mark_y), r));
    g
}

/// The upper half of a circle as `n` segments, from (c.x + r, c.y) over the top to (c.x − r, c.y)
/// (a socket's cup, open downwards; no arcs in definitions).
fn half_circle_up(c: (f64, f64), r: f64, n: usize) -> Primitive {
    let v: Vec<(f64, f64)> = (0..=n)
        .map(|k| {
            let a = std::f64::consts::PI * k as f64 / n as f64;
            (c.0 + r * a.cos(), c.1 + r * a.sin())
        })
        .collect();
    poly(&v, false)
}

/// The pin pitch of connector bodies, in millimetres (design 034 §5).
const PIN_PITCH: f64 = 5.0;

/// What a connector key names: its pin count and form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Connector {
    Socket(usize),
    Plug(usize),
    DsubSocket(usize),
    DsubPlug(usize),
    M12Socket(usize),
    M12Plug(usize),
    /// A feed-through with `n` pins and a shield point.
    Bulkhead(usize),
}

impl Connector {
    /// The connector body a starter draws; `None` for any other starter.
    fn of(key: StarterKey) -> Option<Connector> {
        Some(match key {
            K::Rj45 => Connector::Bulkhead(8),
            K::Usba => Connector::Bulkhead(4),
            K::Usbb => Connector::Bulkhead(4),
            K::Socket2 => Connector::Socket(2),
            K::Plug2 => Connector::Plug(2),
            K::Socket3 => Connector::Socket(3),
            K::Plug3 => Connector::Plug(3),
            K::Socket4 => Connector::Socket(4),
            K::Plug4 => Connector::Plug(4),
            K::Socket6 => Connector::Socket(6),
            K::Plug6 => Connector::Plug(6),
            K::Socket8 => Connector::Socket(8),
            K::Plug8 => Connector::Plug(8),
            K::Socket12 => Connector::Socket(12),
            K::Plug12 => Connector::Plug(12),
            K::Dsub9s => Connector::DsubSocket(9),
            K::Dsub9p => Connector::DsubPlug(9),
            K::Dsub15s => Connector::DsubSocket(15),
            K::Dsub15p => Connector::DsubPlug(15),
            K::Dsub25s => Connector::DsubSocket(25),
            K::Dsub25p => Connector::DsubPlug(25),
            K::M12s4 => Connector::M12Socket(4),
            K::M12p4 => Connector::M12Plug(4),
            K::M12s5 => Connector::M12Socket(5),
            K::M12p5 => Connector::M12Plug(5),
            K::M12s8 => Connector::M12Socket(8),
            K::M12p8 => Connector::M12Plug(8),
            K::Coil
            | K::No
            | K::Nc
            | K::Power3
            | K::Mcb1
            | K::Mcb2
            | K::Fuse
            | K::Psu
            | K::Pbno
            | K::Pbnc
            | K::Sel
            | K::Lamp
            | K::Motor3
            | K::Motor1
            | K::Trafo
            | K::TerminalBlock
            | K::TerminalBlockPe
            | K::Di
            | K::Do
            | K::Continuation
            | K::Co
            | K::Swco
            | K::Power4
            | K::Rcbo2
            | K::Rcd2
            | K::Rcd4
            | K::Ssrin
            | K::Ssrout
            | K::Fuseterm
            | K::Fusebank4
            | K::Fusebank6
            | K::Fusebank8
            | K::Fusebank10
            | K::Fusebank12
            | K::Prox3
            | K::Floatno
            | K::Floatnc
            | K::Rollerno
            | K::Rollernc
            | K::Interlocknc
            | K::Interlockno
            | K::Valve
            | K::Valve2
            | K::Tc
            | K::Heater
            | K::Pinpair
            | K::Pins
            | K::Pinp
            | K::Plccpu
            | K::Plcps
            | K::Plcdi
            | K::Plcdq
            | K::Plcai
            | K::Plcao
            | K::Plcrtd
            | K::Plciol
            | K::Term2L1
            | K::Term2L2
            | K::Term2
            | K::Term2bL1
            | K::Term2bL2
            | K::Term2b
            | K::Term3L1
            | K::Term3L2
            | K::Term3L3
            | K::Term3
            | K::TermlnpeL1
            | K::TermlnpeL2
            | K::TermlnpeL3
            | K::Termlnpe
            | K::TermsensorL1
            | K::TermsensorL2
            | K::TermsensorL3
            | K::Termsensor
            | K::Termdisc
            | K::Termfuse
            | K::Plcim
            | K::Plcpm
            | K::Plccm
            | K::Chdi
            | K::Chdq
            | K::Chai
            | K::Chao
            | K::Chrtd
            | K::Chiol => return None,
        })
    }

    fn pins(self) -> usize {
        match self {
            Connector::Socket(n)
            | Connector::Plug(n)
            | Connector::DsubSocket(n)
            | Connector::DsubPlug(n)
            | Connector::M12Socket(n)
            | Connector::M12Plug(n)
            | Connector::Bulkhead(n) => n,
        }
    }

    /// Whether its pins point up (a socket) or down (a plug); a bulkhead's are at y = 0.
    fn socket(self) -> bool {
        matches!(self, Connector::Socket(_) | Connector::DsubSocket(_) | Connector::M12Socket(_))
    }

    fn graphics(self) -> Vec<Primitive> {
        let n = self.pins();
        let xn = pin_x(n, PIN_PITCH);
        let mut g = Vec::new();
        if let Connector::Bulkhead(_) = self {
            // Rings at every pin and the shield, one point each: a feed-through is one net.
            let xl = pin_x(n + 1, PIN_PITCH);
            g.extend((1..=n + 1).map(|i| circle((pin_x(i, PIN_PITCH), 0.0), 1.5)));
            g.push(rect((-4.0, -4.0), (xl + 4.0, 4.0)));
            g.push(line((-5.0, -5.0), (-5.0, 5.0)));
            g.push(line((xl + 5.0, -5.0), (xl + 5.0, 5.0)));
            return g;
        }
        for i in 1..=n {
            let x = pin_x(i, PIN_PITCH);
            if self.socket() {
                g.push(line((x, 5.0), (x, 1.5)));
                g.push(half_circle_up((x, 0.0), 1.5, 6));
            } else {
                g.push(line((x, -5.0), (x, -1.0)));
                g.push(rect((x - 0.5, -1.0), (x + 0.5, 1.5)));
            }
        }
        g.push(match self {
            Connector::DsubSocket(_) => poly(&[(-3.5, 3.5), (xn + 3.5, 3.5), (xn + 2.5, -2.5), (-2.5, -2.5)], true),
            Connector::DsubPlug(_) => poly(&[(-2.5, 2.5), (xn + 2.5, 2.5), (xn + 3.5, -3.5), (-3.5, -3.5)], true),
            Connector::Socket(_) | Connector::M12Socket(_) => rect((-3.5, -2.5), (xn + 3.5, 3.5)),
            Connector::Plug(_) | Connector::M12Plug(_) | Connector::Bulkhead(_) => rect((-3.5, -3.5), (xn + 3.5, 2.5)),
        });
        match self {
            Connector::M12Socket(_) => g.push(circle((-6.5, 0.5), 2.0)),
            Connector::M12Plug(_) => g.push(circle((-6.5, -0.5), 2.0)),
            Connector::Socket(_) | Connector::Plug(_) | Connector::DsubSocket(_) | Connector::DsubPlug(_) | Connector::Bulkhead(_) => {}
        }
        g
    }

    fn connection_points(self, t: &dyn Fn(&str, f64, f64) -> ConnectionPointDef) -> Vec<ConnectionPointDef> {
        let n = self.pins();
        let y = match self {
            Connector::Bulkhead(_) => 0.0,
            Connector::Socket(_) | Connector::DsubSocket(_) | Connector::M12Socket(_) => 5.0,
            Connector::Plug(_) | Connector::DsubPlug(_) | Connector::M12Plug(_) => -5.0,
        };
        let mut v: Vec<ConnectionPointDef> = (1..=n).map(|i| t(&i.to_string(), pin_x(i, PIN_PITCH), y)).collect();
        if let Connector::Bulkhead(_) = self {
            v.push(t("SH", pin_x(n + 1, PIN_PITCH), 0.0));
        }
        v
    }
}

/// A PLC card body as the card's power end (brief 051, design 051 §2.4): on each side that has
/// points, an edge through them and dividers between neighbouring points and at both ends, down
/// (or up) the header band; width the points' span plus 10, at least 20. The type word is the
/// frame's type text now, not graphics.
fn card_body(points: &[(f64, f64)]) -> Vec<Primitive> {
    let mut g = Vec::new();
    for side in [1.0_f64, -1.0] {
        let mut xs: Vec<f64> = points.iter().filter(|(_, y)| y.signum() == side).map(|(x, _)| *x).collect();
        if xs.is_empty() {
            continue;
        }
        xs.sort_by(f64::total_cmp);
        let (lo, hi) = (xs.first().copied().unwrap_or(0.0), xs.last().copied().unwrap_or(0.0));
        let pad = ((20.0 - (hi - lo)) / 2.0).max(5.0);
        let (l, r) = (lo - pad, hi + pad);
        let y = 25.0 * side;
        let d = y - HEADER_BAND * side;
        g.push(line((l, y), (r, y)));
        let mut cuts = vec![l, r];
        cuts.extend(xs.windows(2).filter_map(|w| Some((w.first()? + w.get(1)?) / 2.0)));
        g.extend(cuts.into_iter().map(|x| line((x, y), (x, d))));
    }
    g
}

/// Pin `i`'s x (1-based) in a row of pins or fuses spaced `pitch` apart.
fn pin_x(i: usize, pitch: f64) -> f64 {
    pitch * i.saturating_sub(1) as f64
}

/// A fuse bank of `n` ways (design 034 §3).
fn fuse_bank(n: usize) -> Vec<Primitive> {
    let xn = pin_x(n, 10.0);
    let mut g = vec![
        rect((-15.0, -10.0), (xn + 5.0, 15.0)),
        line((-10.0, 20.0), (-10.0, 10.0)),
        line((-10.0, 10.0), (xn, 10.0)),
        line((-10.0, -15.0), (-10.0, -5.0)),
        line((-12.0, -5.0), (-8.0, -5.0)),
    ];
    for i in 1..=n {
        let x = pin_x(i, 10.0);
        g.push(line((x, 10.0), (x, -15.0)));
        g.push(rect((x - 2.0, -3.0), (x + 2.0, 7.0)));
    }
    g
}

/// Graphics of the sized keys (fuse banks, connectors).
fn sized_graphics(key: StarterKey) -> Vec<Primitive> {
    if let Some(n) = key.fuse_bank_ways() {
        return fuse_bank(n);
    }
    Connector::of(key).map(Connector::graphics).unwrap_or_default()
}

/// Terminals of the sized keys.
fn sized_connection_points(key: StarterKey, t: &dyn Fn(&str, f64, f64) -> ConnectionPointDef) -> Vec<ConnectionPointDef> {
    if let Some(n) = key.fuse_bank_ways() {
        let mut v = vec![t("L+", -10.0, 20.0), t("M", -10.0, -15.0)];
        v.extend((1..=n).map(|i| t(&i.to_string(), pin_x(i, 10.0), -15.0)));
        return v;
    }
    Connector::of(key).map(|c| c.connection_points(t)).unwrap_or_default()
}

/// A socket pin (brief 044): its lead from (0, 5) and 034's cup, open downwards, centre (0, 0).
fn socket_pin() -> Vec<Primitive> {
    vec![line((0.0, 5.0), (0.0, 1.5)), half_circle_up((0.0, 0.0), 1.5, 6)]
}

/// A plug pin (brief 044): its lead from (0, −5) and the pin.
fn plug_pin() -> Vec<Primitive> {
    vec![line((0.0, -5.0), (0.0, -1.5)), rect((-0.5, -1.5), (0.5, 1.0))]
}

fn contact_no() -> Vec<Primitive> {
    vec![line((0.0, 10.0), (0.0, 4.0)), line((0.0, -10.0), (0.0, -4.0)), line((0.0, -4.0), (-4.0, 5.0))]
}

fn contact_nc() -> Vec<Primitive> {
    vec![line((0.0, 10.0), (0.0, 4.0)), line((0.0, -10.0), (0.0, -4.0)), line((0.0, 4.0), (3.0, 4.0)), line((0.0, -4.0), (4.0, 5.5))]
}

fn mcb() -> Vec<Primitive> {
    let mut g = contact_no();
    g.push(line((-1.5, 2.5), (1.5, 5.5)));
    g.push(line((-1.5, 5.5), (1.5, 2.5)));
    g
}

fn motor(phase: &str, leads: &[((f64, f64), (f64, f64))]) -> Vec<Primitive> {
    let mut g = vec![circle((0.0, 0.0), 10.0), text((-2.0, -0.5), 5.0, "M"), text((-2.5, -6.0), 3.0, phase)];
    g.extend(leads.iter().map(|(a, b)| line(*a, *b)));
    g.push(poly(&[(20.0, 15.0), (20.0, 0.0), (10.0, 0.0)], false));
    g
}

/// The graphics of starter `key` (brief 036: `termfuse` draws `fuseterm`'s).
pub(crate) fn graphics_of(key: StarterKey) -> Vec<Primitive> {
    graphics(key)
}

fn graphics(key: StarterKey) -> Vec<Primitive> {
    match key {
        K::Coil => vec![rect((-6.0, -3.0), (6.0, 3.0)), line((0.0, 3.0), (0.0, 10.0)), line((0.0, -3.0), (0.0, -10.0))],
        K::No => contact_no(),
        K::Nc => contact_nc(),
        K::Power3 => {
            let mut g = Vec::new();
            for x in [-10.0, 0.0, 10.0] {
                g.extend(shifted(contact_no(), x));
            }
            g.extend(dashed(-12.0, 8.0, 0.5));
            g
        }
        K::Mcb1 => mcb(),
        K::Mcb2 => {
            let mut g = mcb();
            g.extend(shifted(mcb(), 10.0));
            g.extend(dashed(-2.0, 8.0, 0.5));
            g
        }
        K::Fuse => vec![rect((-2.0, -5.0), (2.0, 5.0)), line((0.0, 10.0), (0.0, -10.0))],
        K::Psu => {
            let mut g = vec![
                rect((-15.0, -10.0), (15.0, 10.0)),
                line((-15.0, -10.0), (15.0, 10.0)),
                text((-13.0, 4.0), 3.5, "~"),
                text((8.0, -8.0), 3.5, "="),
            ];
            for x in [-10.0, 0.0, 10.0] {
                g.push(line((x, 15.0), (x, 10.0)));
            }
            for x in [-5.0, 5.0] {
                g.push(line((x, -15.0), (x, -10.0)));
            }
            g
        }
        K::Pbno => {
            let mut g = contact_no();
            g.extend(dashed(-8.0, -2.0, 0.5));
            g.push(poly(&[(-10.0, 3.0), (-8.0, 3.0), (-8.0, -2.0), (-10.0, -2.0)], false));
            g
        }
        K::Pbnc => {
            let mut g = contact_nc();
            g.extend(dashed(-6.0, 1.9, 0.5));
            g.push(poly(&[(-8.0, 3.0), (-6.0, 3.0), (-6.0, -2.0), (-8.0, -2.0)], false));
            g
        }
        K::Sel => {
            let mut g = contact_no();
            g.extend(dashed(-8.0, -2.0, 0.5));
            g.push(poly(&[(-10.0, 3.0), (-8.0, 3.0), (-8.0, -2.0), (-6.0, -2.0)], false));
            g
        }
        K::Lamp => vec![
            circle((0.0, 0.0), 4.0),
            line((-2.83, -2.83), (2.83, 2.83)),
            line((-2.83, 2.83), (2.83, -2.83)),
            line((0.0, 4.0), (0.0, 10.0)),
            line((0.0, -4.0), (0.0, -10.0)),
        ],
        K::Motor3 => motor("3~", &[((-10.0, 15.0), (-5.0, 8.66)), ((0.0, 15.0), (0.0, 10.0)), ((10.0, 15.0), (5.0, 8.66))]),
        K::Motor1 => motor("1~", &[((-5.0, 15.0), (-5.0, 8.66)), ((5.0, 15.0), (5.0, 8.66))]),
        K::Trafo => {
            // Where x = ±5 meets each circle of radius 6 around (0, ±4.5).
            let y = 4.5 + (36.0_f64 - 25.0).sqrt();
            vec![
                circle((0.0, 4.5), 6.0),
                circle((0.0, -4.5), 6.0),
                line((-5.0, 15.0), (-5.0, y)),
                line((5.0, 15.0), (5.0, y)),
                line((-5.0, -15.0), (-5.0, -y)),
                line((5.0, -15.0), (5.0, -y)),
            ]
        }
        K::TerminalBlock => vec![circle((0.0, 0.0), 1.5)],
        // Our own earth mark to the right of the ring (design 028 §2).
        K::TerminalBlockPe => {
            vec![circle((0.0, 0.0), 1.5), line((2.5, -1.2), (2.5, 1.2)), line((3.2, -0.8), (3.2, 0.8)), line((3.9, -0.4), (3.9, 0.4))]
        }
        // An arrow pointing away from its wire (design 022 §3.2).
        K::Continuation => vec![line((0.0, 0.0), (0.0, -5.0)), poly(&[(-1.5, -3.5), (0.0, -5.0), (1.5, -3.5)], false)],
        K::Di => vec![rect((-7.5, -2.5), (7.5, 2.5)), text((-2.5, -1.25), 2.5, "DI"), line((0.0, 2.5), (0.0, 10.0))],
        K::Do => vec![rect((-7.5, -2.5), (7.5, 2.5)), text((-2.5, -1.25), 2.5, "DO"), line((0.0, -2.5), (0.0, -10.0))],
        // ---- Brief 034 stage 1 (design 034 §3).
        K::Co => contact_co(),
        K::Swco => {
            let mut g = contact_co();
            g.extend(dashed(-8.0, 1.5, 0.5));
            g.push(line((-8.0, -2.5), (-8.0, 3.5)));
            g
        }
        K::Power4 => {
            let mut g = Vec::new();
            for x in [-15.0, -5.0, 5.0, 15.0] {
                g.extend(shifted(contact_no(), x));
            }
            g.extend(dashed(-17.0, 13.0, 0.5));
            g
        }
        K::Rcbo2 | K::Rcd2 => {
            let mut g = if key == K::Rcbo2 { mcb() } else { contact_no() };
            g.extend(shifted(contact_no(), 10.0));
            g.extend(dashed(-2.0, 8.0, 0.5));
            g.extend(rcd_release(5.0, 8.0, 0.0, 8.0));
            g
        }
        K::Rcd4 => {
            let mut g = Vec::new();
            for x in [0.0, 10.0, 20.0, 30.0] {
                g.extend(shifted(contact_no(), x));
            }
            g.extend(dashed(-2.0, 28.0, 0.5));
            g.extend(rcd_release(15.0, 18.0, 20.0, 28.0));
            g
        }
        K::Ssrin => vec![
            rect((-6.0, -3.0), (6.0, 3.0)),
            line((0.0, 3.0), (0.0, 10.0)),
            line((0.0, -3.0), (0.0, -10.0)),
            poly(&[(-2.0, 1.5), (2.0, 1.5), (0.0, -1.5)], true),
            line((-2.0, -1.5), (2.0, -1.5)),
        ],
        K::Ssrout => {
            let mut g = contact_no();
            g.push(poly(&[(2.0, -1.5), (5.0, -1.5), (3.5, 1.5)], true));
            g
        }
        K::Fuseterm => vec![rect((-2.0, -5.0), (2.0, 5.0)), line((0.0, 10.0), (0.0, -10.0)), rect((-3.5, -7.5), (3.5, 7.5))],
        // ---- Brief 034 stage 2 (design 034 §4).
        K::Prox3 => vec![
            rect((-10.0, -5.0), (10.0, 5.0)),
            poly(&[(-7.0, 0.0), (-5.0, 2.0), (-3.0, 0.0), (-5.0, -2.0)], true),
            line((3.0, 2.5), (3.0, 1.5)),
            line((3.0, -2.5), (3.0, -1.5)),
            line((3.0, -1.5), (1.5, 2.0)),
            line((-5.0, 10.0), (-5.0, 5.0)),
            line((-5.0, -10.0), (-5.0, -5.0)),
            line((5.0, -10.0), (5.0, -5.0)),
        ],
        K::Floatno => levered(true, -4.0, 1.5, -5.5),
        K::Floatnc => levered(false, -4.0, 1.5, -5.5),
        K::Rollerno => levered(true, 3.0, 1.2, 4.2),
        K::Rollernc => levered(false, 3.0, 1.2, 4.2),
        K::Interlocknc => {
            let mut g = contact_nc();
            g.extend(dashed(-8.0, 1.9, 0.5));
            g.push(circle((-4.0, 0.5), 1.5));
            g.push(line((-5.0, 0.5), (-3.0, 0.5)));
            g.push(poly(&[(-3.6, 1.1), (-3.0, 0.5), (-3.6, -0.1)], false));
            g.extend(key_mark());
            g
        }
        K::Interlockno => {
            let mut g = contact_no();
            g.extend(dashed(-8.0, -2.0, 0.5));
            g.extend(key_mark());
            g
        }
        K::Valve => vec![
            rect((-6.0, -3.0), (6.0, 3.0)),
            line((0.0, 3.0), (0.0, 10.0)),
            line((0.0, -3.0), (0.0, -10.0)),
            line((6.0, 0.0), (8.0, 0.0)),
            poly(&[(8.0, 3.0), (14.0, -3.0), (14.0, 3.0), (8.0, -3.0)], true),
        ],
        K::Valve2 => vec![
            rect((-16.0, -3.0), (-4.0, 3.0)),
            rect((4.0, -3.0), (16.0, 3.0)),
            line((-10.0, 3.0), (-10.0, 10.0)),
            line((-10.0, -3.0), (-10.0, -10.0)),
            line((10.0, 3.0), (10.0, 10.0)),
            line((10.0, -3.0), (10.0, -10.0)),
            poly(&[(-3.0, 3.0), (3.0, -3.0), (3.0, 3.0), (-3.0, -3.0)], true),
            line((-4.0, 0.0), (-3.0, 0.0)),
            line((3.0, 0.0), (4.0, 0.0)),
        ],
        K::Tc => vec![
            poly(&[(-5.0, 10.0), (-5.0, 0.0), (0.0, -5.0)], false),
            poly(&[(5.0, 10.0), (5.0, 0.0), (0.0, -5.0)], false),
            line((5.5, 9.0), (5.5, 0.5)),
            circle((0.0, -5.0), 0.5),
            circle((0.0, -5.0), 0.33),
            circle((0.0, -5.0), 0.17),
        ],
        K::Heater => vec![
            rect((-2.5, -6.0), (2.5, 6.0)),
            line((-2.5, -3.0), (2.5, -3.0)),
            line((-2.5, 0.0), (2.5, 0.0)),
            line((-2.5, 3.0), (2.5, 3.0)),
            line((0.0, 6.0), (0.0, 10.0)),
            line((0.0, -6.0), (0.0, -10.0)),
        ],
        // ---- Brief 034 stage 3b (design 034 §6).
        K::Plcps => card_body(&[(-10.0, 25.0), (-5.0, 25.0), (0.0, 25.0), (-5.0, -25.0), (5.0, -25.0)]),
        K::Plccm => card_body(&[(-5.0, -25.0), (5.0, -25.0)]),
        K::Plccpu | K::Plcdi | K::Plcdq | K::Plcai | K::Plcao | K::Plcrtd | K::Plciol | K::Plcim | K::Plcpm => {
            card_body(&[(-5.0, 25.0), (5.0, 25.0)])
        }
        k @ (K::Chdi | K::Chdq | K::Chai | K::Chao | K::Chrtd | K::Chiol) => cell_graphics(k),
        // ---- Brief 044 (design 044 §3.2): connector pins, 034's cup and pin, no body.
        K::Pinpair => {
            let mut g = socket_pin();
            g.extend(plug_pin());
            g
        }
        K::Pins => socket_pin(),
        K::Pinp => plug_pin(),
        K::Fusebank4
        | K::Fusebank6
        | K::Fusebank8
        | K::Fusebank10
        | K::Fusebank12
        | K::Socket2
        | K::Socket3
        | K::Socket4
        | K::Socket6
        | K::Socket8
        | K::Socket12
        | K::Plug2
        | K::Plug3
        | K::Plug4
        | K::Plug6
        | K::Plug8
        | K::Plug12
        | K::Dsub9s
        | K::Dsub15s
        | K::Dsub25s
        | K::Dsub9p
        | K::Dsub15p
        | K::Dsub25p
        | K::M12s4
        | K::M12s5
        | K::M12s8
        | K::M12p4
        | K::M12p5
        | K::M12p8
        | K::Rj45
        | K::Usba
        | K::Usbb
        | K::Term2L1
        | K::Term2L2
        | K::Term2
        | K::Term2bL1
        | K::Term2bL2
        | K::Term2b
        | K::Term3L1
        | K::Term3L2
        | K::Term3L3
        | K::Term3
        | K::TermlnpeL1
        | K::TermlnpeL2
        | K::TermlnpeL3
        | K::Termlnpe
        | K::TermsensorL1
        | K::TermsensorL2
        | K::TermsensorL3
        | K::Termsensor
        | K::Termdisc
        | K::Termfuse => crate::terminal_strips::terminal_blocks::graphics(key).unwrap_or_else(|| sized_graphics(key)),
    }
}

fn connection_points(key: StarterKey) -> Vec<ConnectionPointDef> {
    let t = |k: &str, x: f64, y: f64| ConnectionPointDef { key: PointKey::def(k), label: Some(k.into()), at: p(x, y), accepts: Default::default() };
    let side = |k: PointKey, x: f64, y: f64| ConnectionPointDef { label: Some(k.to_string()), key: k, at: p(x, y), accepts: Default::default() };
    match key {
        K::Coil => vec![t("A1", 0.0, 10.0), t("A2", 0.0, -10.0)],
        K::No | K::Pbno | K::Sel | K::Floatno | K::Rollerno | K::Interlockno => vec![t("13", 0.0, 10.0), t("14", 0.0, -10.0)],
        K::Nc | K::Pbnc | K::Floatnc | K::Rollernc | K::Interlocknc => vec![t("11", 0.0, 10.0), t("12", 0.0, -10.0)],
        K::Power3 => vec![t("1", -10.0, 10.0), t("2", -10.0, -10.0), t("3", 0.0, 10.0), t("4", 0.0, -10.0), t("5", 10.0, 10.0), t("6", 10.0, -10.0)],
        K::Mcb1 | K::Fuse => vec![t("1", 0.0, 10.0), t("2", 0.0, -10.0)],
        K::Mcb2 => vec![t("1", 0.0, 10.0), t("2", 0.0, -10.0), t("3", 10.0, 10.0), t("4", 10.0, -10.0)],
        K::Psu => {
            vec![t("L", -10.0, 15.0), t("N", 0.0, 15.0), t("PE", 10.0, 15.0), t("+", -5.0, -15.0), t("-", 5.0, -15.0)]
        }
        K::Lamp => vec![t("X1", 0.0, 10.0), t("X2", 0.0, -10.0)],
        K::Motor3 => vec![t("U1", -10.0, 15.0), t("V1", 0.0, 15.0), t("W1", 10.0, 15.0), t("PE", 20.0, 15.0)],
        K::Motor1 => vec![t("U1", -5.0, 15.0), t("U2", 5.0, 15.0), t("PE", 20.0, 15.0)],
        K::Trafo => vec![t("1U1", -5.0, 15.0), t("1U2", 5.0, 15.0), t("2U1", -5.0, -15.0), t("2U2", 5.0, -15.0)],
        K::TerminalBlock | K::TerminalBlockPe => vec![t("1", 0.0, 0.0)],
        // No label: a continuation shows no connection point name.
        K::Continuation => vec![ConnectionPointDef { key: PointKey::def("1"), label: None, at: p(0.0, 0.0), accepts: Default::default() }],
        // Design 072 §3: a channel symbol's point is keyed by its role; the label is what it shows.
        K::Di => vec![ConnectionPointDef {
            key: PointKey::Role(ecad_model::ChannelRole::In),
            label: Some("I0.0".into()),
            at: p(0.0, 10.0),
            accepts: Default::default(),
        }],
        K::Do => vec![ConnectionPointDef {
            key: PointKey::Role(ecad_model::ChannelRole::Out),
            label: Some("Q0.0".into()),
            at: p(0.0, -10.0),
            accepts: Default::default(),
        }],
        // ---- Brief 034 stage 1: definition order is common, NC, NO (design 034 §2.3).
        K::Co | K::Swco => vec![t("11", 0.0, 10.0), t("12", 5.0, -10.0), t("14", 0.0, -10.0)],
        K::Power4 => vec![
            t("1", -15.0, 10.0),
            t("2", -15.0, -10.0),
            t("3", -5.0, 10.0),
            t("4", -5.0, -10.0),
            t("5", 5.0, 10.0),
            t("6", 5.0, -10.0),
            t("7", 15.0, 10.0),
            t("8", 15.0, -10.0),
        ],
        K::Rcbo2 | K::Rcd2 => vec![t("1", 0.0, 10.0), t("2", 0.0, -10.0), t("N1", 10.0, 10.0), t("N2", 10.0, -10.0)],
        K::Rcd4 => vec![
            t("1", 0.0, 10.0),
            t("2", 0.0, -10.0),
            t("3", 10.0, 10.0),
            t("4", 10.0, -10.0),
            t("5", 20.0, 10.0),
            t("6", 20.0, -10.0),
            t("N1", 30.0, 10.0),
            t("N2", 30.0, -10.0),
        ],
        K::Ssrin => vec![t("A1", 0.0, 10.0), t("A2", 0.0, -10.0)],
        K::Ssrout | K::Fuseterm | K::Heater => vec![t("1", 0.0, 10.0), t("2", 0.0, -10.0)],
        // ---- Brief 034 stage 2: IEC 60947-5-2's numbers (1 supply +, 3 supply −, 4 output).
        K::Prox3 => vec![t("1", -5.0, 10.0), t("3", -5.0, -10.0), t("4", 5.0, -10.0)],
        K::Valve => vec![t("A1", 0.0, 10.0), t("A2", 0.0, -10.0)],
        K::Valve2 => vec![t("A1", -10.0, 10.0), t("A2", -10.0, -10.0), t("B1", 10.0, 10.0), t("B2", 10.0, -10.0)],
        K::Tc => vec![t("+", -5.0, 10.0), t("-", 5.0, 10.0)],
        // ---- Brief 034 stage 3b.
        K::Plcps => vec![t("L", -10.0, 25.0), t("N", -5.0, 25.0), t("PE", 0.0, 25.0), t("L+", -5.0, -25.0), t("M", 5.0, -25.0)],
        K::Plccpu | K::Plcdi | K::Plcdq | K::Plcai | K::Plcao | K::Plcrtd | K::Plciol => vec![t("L+", -5.0, 25.0), t("M", 5.0, 25.0)],
        // ---- Brief 045 (design 045 §3.1).
        K::Plcim | K::Plcpm => vec![t("L+", -5.0, 25.0), t("M", 5.0, 25.0)],
        K::Plccm => vec![t("P1", -5.0, -25.0), t("P2", 5.0, -25.0)],
        // Brief 045 (design 045 §4.1, FP2): a cell's points are keyed by role; the pin is the
        // label given when it joins a card.
        k @ (K::Chdi | K::Chdq | K::Chai | K::Chao | K::Chrtd | K::Chiol) => cell_shape(k)
            .map(|(_, pts)| {
                pts.iter()
                    .filter_map(|(role, x)| {
                        let role = ecad_model::ChannelRole::exact(role)?;
                        Some(ConnectionPointDef {
                            key: PointKey::Role(role),
                            label: Some(role.word().to_string()),
                            at: p(*x, 5.0),
                            accepts: Default::default(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
        // ---- Brief 044: a pin's definition keys are its side keys.
        K::Pinpair => vec![side(SOCKET_KEY, 0.0, 5.0), side(PLUG_KEY, 0.0, -5.0)],
        K::Pins => vec![side(SOCKET_KEY, 0.0, 5.0)],
        K::Pinp => vec![side(PLUG_KEY, 0.0, -5.0)],
        K::Fusebank4
        | K::Fusebank6
        | K::Fusebank8
        | K::Fusebank10
        | K::Fusebank12
        | K::Socket2
        | K::Socket3
        | K::Socket4
        | K::Socket6
        | K::Socket8
        | K::Socket12
        | K::Plug2
        | K::Plug3
        | K::Plug4
        | K::Plug6
        | K::Plug8
        | K::Plug12
        | K::Dsub9s
        | K::Dsub15s
        | K::Dsub25s
        | K::Dsub9p
        | K::Dsub15p
        | K::Dsub25p
        | K::M12s4
        | K::M12s5
        | K::M12s8
        | K::M12p4
        | K::M12p5
        | K::M12p8
        | K::Rj45
        | K::Usba
        | K::Usbb
        | K::Term2L1
        | K::Term2L2
        | K::Term2
        | K::Term2bL1
        | K::Term2bL2
        | K::Term2b
        | K::Term3L1
        | K::Term3L2
        | K::Term3L3
        | K::Term3
        | K::TermlnpeL1
        | K::TermlnpeL2
        | K::TermlnpeL3
        | K::Termlnpe
        | K::TermsensorL1
        | K::TermsensorL2
        | K::TermsensorL3
        | K::Termsensor
        | K::Termdisc
        | K::Termfuse => crate::terminal_strips::terminal_blocks::connection_points(key).unwrap_or_else(|| sized_connection_points(key, &t)),
    }
}

/// The local bounding box of a definition's graphics and terminals (text counted at its insertion
/// point and height). `(0,0)`–`(0,0)` for an empty definition.
pub fn bounds(d: &Definition) -> (Pt, Pt) {
    let mut lo = p(f64::INFINITY, f64::INFINITY);
    let mut hi = p(f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut add = |q: Pt| {
        if q.is_finite() {
            lo = p(lo.x.min(q.x), lo.y.min(q.y));
            hi = p(hi.x.max(q.x), hi.y.max(q.y));
        }
    };
    for g in &d.graphics {
        match g {
            Primitive::Line { a, b } => {
                add(*a);
                add(*b);
            }
            Primitive::Polyline { vertices, .. } => vertices.iter().for_each(|v| add(*v)),
            Primitive::Circle { center, radius } | Primitive::Arc { center, radius, .. } => {
                add(p(center.x - radius, center.y - radius));
                add(p(center.x + radius, center.y + radius));
            }
            Primitive::Text { at, height, value, .. } => {
                add(*at);
                add(p(at.x + height * 0.8 * value.chars().count() as f64, at.y + height));
            }
            Primitive::Point { at } => add(*at),
        }
    }
    for t in &d.connection_points {
        add(t.at);
    }
    if !(lo.is_finite() && hi.is_finite()) {
        return (p(0.0, 0.0), p(0.0, 0.0));
    }
    (lo, hi)
}

/// The point grid of the builder, the default of `symbol.point_grid` (design 048 §7.1, Q6).
pub const POINT_GRID_MM: f64 = 5.0;

/// Whether placed copy `rep` shows an older version than its lineage's newest (design 048 §8.4).
pub fn update_available(doc: &Document, rep: ecad_model::RepresentationId) -> Option<(DefinitionId, u64, u64)> {
    let r = doc.representations.get(&rep)?;
    let (d, _) = crate::devices::rep_definition(doc, r)?;
    let o = d.origin.as_ref().filter(|o| o.library == PROJECT_SYMBOLS)?;
    let n = newest(doc, o.definition)?;
    (n.seq > o.library_seq).then_some((n.id, o.library_seq, n.seq))
}

/// A connector side's word in the connector tool's prompts.
pub fn side_word(side: Side) -> &'static str {
    match side {
        Side::Socket => "Socket",
        Side::Plug => "Plug",
        Side::Both => "Both",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starters_are_keyed_in_order() {
        // One row per StarterKey, in variant order, and each key reads back from its word.
        assert_eq!(STARTERS.len(), StarterKey::ALL.len());
        for (i, (s, k)) in STARTERS.iter().zip(StarterKey::ALL).enumerate() {
            assert_eq!(s.key, *k, "row {i}");
            assert_eq!(starter(*k).key, *k);
            assert_eq!(StarterKey::parse(k.word()), Some(*k));
            assert_eq!(by_key(&k.word().to_uppercase()).map(|x| x.key), Some(*k));
        }
        assert!(by_key("nosuch").is_none());
    }

    #[test]
    fn every_starter_is_valid_and_on_the_grid() {
        let mut keys = std::collections::BTreeSet::new();
        let mut ids = std::collections::BTreeSet::new();
        for s in STARTERS {
            assert!(keys.insert(s.key) && ids.insert(s.id), "{} repeated", s.key);
            let d = s.definition().unwrap();
            assert!(!d.graphics.is_empty() && !d.connection_points.is_empty(), "{}", s.key);
            for t in &d.connection_points {
                let on = |v: f64| (v / SYMBOL_GRID_MM - (v / SYMBOL_GRID_MM).round()).abs() < 1e-9;
                assert!(on(t.at.x) && on(t.at.y), "{} terminal {} off the grid", s.key, t.key);
            }
            assert_eq!(of_definition(d).map(|x| x.key), Some(s.key));
            // The origin's hash is the content's.
            let o = d.origin.clone().unwrap();
            let mut bare = d.clone();
            bare.origin = None;
            assert_eq!(ecad_file::definition_hash(&bare).unwrap(), o.content_hash);
        }
        // 18 symbols (brief 018), the continuation (brief 022), the PE terminal (028),
        // brief 034's stage 1 (14), stage 2 (11), stage 3a (27) and stage 3b (8), brief 036's
        // multi-level terminals (20), brief 044's connector pins (3), brief 045's modules (3) and
        // channel cells (6).
        assert_eq!(STARTERS.len(), 20 + 14 + 11 + 27 + 8 + 20 + 3 + 3 + 6);
        // Brief 034's definition IDs sit in their family's block.
        for s in STARTERS.iter().filter(|s| s.family != Family::Basic) {
            let n = s.id.0 - def_id(0).0;
            let block = match s.family {
                Family::Contacts | Family::Protection => 0x100,
                Family::Sensors | Family::Actuators => 0x200,
                Family::Connectors => 0x300,
                Family::Plc => 0x400,
                Family::Basic => 0,
            };
            assert!(n > block && n < block + 0x100, "{} id {n:#x} outside its block", s.key);
        }
    }

    /// Design 034 §3–§6: each new symbol's box (width, height) and labels in definition order.
    const NEW: &[(&str, f64, f64, &str)] = &[
        ("co", 5.0, 20.0, "11/12/14"),
        ("swco", 13.0, 20.0, "11/12/14"),
        ("power4", 34.0, 20.0, "1/2/3/4/5/6/7/8"),
        ("rcbo2", 23.0, 20.0, "1/2/N1/N2"),
        ("rcd2", 23.0, 20.0, "1/2/N1/N2"),
        ("rcd4", 43.0, 20.0, "1/2/3/4/5/6/N1/N2"),
        ("ssrin", 12.0, 20.0, "A1/A2"),
        ("ssrout", 9.0, 20.0, "1/2"),
        ("fuseterm", 7.0, 20.0, "1/2"),
        ("fusebank4", 50.0, 35.0, "L+/M/1/2/3/4"),
        ("fusebank6", 70.0, 35.0, "L+/M/1/2/3/4/5/6"),
        ("fusebank8", 90.0, 35.0, "L+/M/1/2/3/4/5/6/7/8"),
        ("fusebank10", 110.0, 35.0, "L+/M/1/2/3/4/5/6/7/8/9/10"),
        ("fusebank12", 130.0, 35.0, "L+/M/1/2/3/4/5/6/7/8/9/10/11/12"),
        ("prox3", 20.0, 20.0, "1/3/4"),
        ("floatno", 9.5, 20.0, "13/14"),
        ("floatnc", 11.5, 20.0, "11/12"),
        ("rollerno", 9.2, 20.0, "13/14"),
        ("rollernc", 11.2, 20.0, "11/12"),
        ("interlocknc", 17.0, 20.0, "11/12"),
        ("interlockno", 13.0, 20.0, "13/14"),
        ("valve", 20.0, 20.0, "A1/A2"),
        ("valve2", 32.0, 20.0, "A1/A2/B1/B2"),
        ("tc", 10.5, 15.5, "+/-"),
        ("heater", 5.0, 20.0, "1/2"),
        ("socket2", 12.0, 7.5, "1/2"),
        ("socket3", 17.0, 7.5, "1/2/3"),
        ("socket4", 22.0, 7.5, "1/2/3/4"),
        ("socket6", 32.0, 7.5, "1/2/3/4/5/6"),
        ("socket8", 42.0, 7.5, "1/2/3/4/5/6/7/8"),
        ("socket12", 62.0, 7.5, "1/2/3/4/5/6/7/8/9/10/11/12"),
        ("plug2", 12.0, 7.5, "1/2"),
        ("plug3", 17.0, 7.5, "1/2/3"),
        ("plug4", 22.0, 7.5, "1/2/3/4"),
        ("plug6", 32.0, 7.5, "1/2/3/4/5/6"),
        ("plug8", 42.0, 7.5, "1/2/3/4/5/6/7/8"),
        ("plug12", 62.0, 7.5, "1/2/3/4/5/6/7/8/9/10/11/12"),
        ("dsub9s", 47.0, 7.5, "1/2/3/4/5/6/7/8/9"),
        ("dsub15s", 77.0, 7.5, "1/2/3/4/5/6/7/8/9/10/11/12/13/14/15"),
        ("dsub25s", 127.0, 7.5, "1/2/3/4/5/6/7/8/9/10/11/12/13/14/15/16/17/18/19/20/21/22/23/24/25"),
        ("dsub9p", 47.0, 7.5, "1/2/3/4/5/6/7/8/9"),
        ("dsub15p", 77.0, 7.5, "1/2/3/4/5/6/7/8/9/10/11/12/13/14/15"),
        ("dsub25p", 127.0, 7.5, "1/2/3/4/5/6/7/8/9/10/11/12/13/14/15/16/17/18/19/20/21/22/23/24/25"),
        ("m12s4", 27.0, 7.5, "1/2/3/4"),
        ("m12s5", 32.0, 7.5, "1/2/3/4/5"),
        ("m12s8", 47.0, 7.5, "1/2/3/4/5/6/7/8"),
        ("m12p4", 27.0, 7.5, "1/2/3/4"),
        ("m12p5", 32.0, 7.5, "1/2/3/4/5"),
        ("m12p8", 47.0, 7.5, "1/2/3/4/5/6/7/8"),
        ("rj45", 50.0, 10.0, "1/2/3/4/5/6/7/8/SH"),
        ("usba", 30.0, 10.0, "1/2/3/4/SH"),
        ("usbb", 30.0, 10.0, "1/2/3/4/SH"),
        // Brief 051 (design 051 §2.2, §2.4, FP3): cells at the pack's pitch, bodies as power ends.
        ("plccpu", 20.0, 10.0, "L+/M"),
        ("plcps", 25.0, 50.0, "L/N/PE/L+/M"),
        ("plcdi", 20.0, 10.0, "L+/M"),
        ("plcdq", 20.0, 10.0, "L+/M"),
        ("plcai", 20.0, 10.0, "L+/M"),
        ("plcao", 20.0, 10.0, "L+/M"),
        ("plcrtd", 20.0, 10.0, "L+/M"),
        ("plciol", 20.0, 10.0, "L+/M"),
        // Brief 045 (design 045 §3.1).
        ("plcim", 20.0, 10.0, "L+/M"),
        ("plcpm", 20.0, 10.0, "L+/M"),
        ("plccm", 20.0, 10.0, "P1/P2"),
        ("chdi", 20.0, 19.0, ".in"),
        ("chdq", 20.0, 19.0, ".out"),
        ("chai", 20.0, 20.5, ".+/.-"),
        ("chao", 20.0, 20.5, ".+/.-"),
        ("chrtd", 40.0, 19.0, ".I+/.S+/.S-/.I-"),
        ("chiol", 30.0, 19.0, ".L+/.C/Q/.L-"),
        // Brief 044 (design 044 §3.2): the definition keys are the side keys.
        ("pinpair", 3.0, 10.0, ".s/.p"),
        ("pins", 3.0, 5.0, ".s"),
        ("pinp", 1.0, 6.0, ".p"),
    ];

    #[test]
    fn new_symbols_have_the_designed_box_and_labels() {
        let mut wrong = Vec::new();
        for (key, w, h, _) in NEW {
            let (sw, sh) = by_key(key).unwrap().size();
            if (sw - w).abs() > 1e-6 || (sh - h).abs() > 1e-6 {
                wrong.push(format!("{key}: box {sw}×{sh}, want {w}×{h}"));
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
        for (key, _, _, labels) in NEW {
            let s = by_key(key).unwrap();
            assert_eq!(s.labels().join("/"), *labels, "{key}");
        }
        assert_eq!(STARTERS.iter().filter(|s| s.family != Family::Basic).count(), NEW.len());
    }

    /// Overseer ruling 4: the 21 starters of briefs 018, 022 and 028 keep their content hashes
    /// (a changed hash would make every old project's embedded copy a different symbol).
    #[test]
    fn the_basic_starters_keep_their_content_hashes() {
        let want: &[(&str, &str)] = &[
            ("coil", "05b6bddc45534d29fc93c421e06a13084cae82ef44285f4482d39cda4c99ea78"),
            ("no", "e6e1e24b6c7bd33d35e3d024514ce5dd6f62a105422ce42c0750183040271dab"),
            ("nc", "de13074bc1f72b98a93f83f896e4b07a25bb7f4ef541d0b3cf91c899d0501631"),
            ("power3", "82faf1478e1d9b589cc8462365a97147b4d14228e66ae81de26188d080b50032"),
            ("mcb1", "f71a787bc5231e494dd4d3b212338bc5aa9ebb2f386d080077329c081b335e9e"),
            ("mcb2", "83da96479e58451044ca0ec7df3b7eeb06ff201b964f0be2e4834ea973f8b400"),
            ("fuse", "5ad58d42b7db9a3be54f4a7e7ed9656579d2eb0d3163be839fdd37b01a6f24c6"),
            ("psu", "8533d527e794630088786d9150259f7507252d805af325b88f380e095558af0a"),
            ("pbno", "b07c02ed07473d84b2f622453bd9bfa446731eded74dc1bd622c6e410fc716eb"),
            ("pbnc", "9faea62fe300da57a72e512639a89c9c4a41b272d84b06b8aaa1fef204f4d0ab"),
            ("sel", "c60160c8e3add0fb75d9f17df4a57401226a54ae530da9a5982a560bcfaaa5bd"),
            ("lamp", "57775972955d04b3b45cfdd7875d86b6fb899c8e0194fcfaf79c7565e1b428bc"),
            ("motor3", "9455fc346ec441fff99e4c88506343755b6009e95e7ddea374ef219852a25d9c"),
            ("motor1", "6d9866b2882a1748cd5a0dc1e19c65954f5e2dbce192ff8b3006987b3b7857e9"),
            ("trafo", "b700be20cb69c8cee00ee1bea53d3c281305df697d7ef6975b0ce95f9dba7ae2"),
            ("terminal_block", "c904e326ee49fd84e63370c40f64130178894087d5e791e0b8030dbc0ce0d550"),
            ("terminal_block_pe", "7884a98a0463a90da4d3b49bb72b2929dd65a829ffd0b02027f904e4f9805da8"),
            ("di", "ff9461723031db3de576373b4a19614c8b238fdb7c2a7fc3a35ddd8d3eaacc53"),
            ("do", "ae6fece507c338ee0e3ed27f84d1b3cf5a510c5e263208b20ed579572d76e485"),
            ("continuation", "934a7f03dce20dd5bbc8697f0440c92b2b7e0e2da486b70df0327408407b25f4"),
        ];
        // Brief 036's starters are new; the hashes pinned here are the earlier basic ones.
        let basic: Vec<_> =
            STARTERS.iter().filter(|s| s.family == Family::Basic && !crate::terminal_strips::terminal_blocks::is_level_starter(s.key)).collect();
        assert_eq!(basic.len(), want.len());
        for (key, hash) in want {
            let o = by_key(key).unwrap().definition().unwrap().origin.clone().unwrap();
            assert_eq!(o.content_hash.hex(), *hash, "{key}'s content hash changed");
        }
    }

    /// Overseer ruling 4: `dashed` draws what it drew before brief 034, and `dashed_v` is the same
    /// pattern along y.
    #[test]
    fn dashed_links_are_unchanged_and_the_vertical_one_matches() {
        let ends = |g: &[Primitive]| {
            g.iter()
                .map(|x| match x {
                    Primitive::Line { a, b } => (a.x, a.y, b.x, b.y),
                    Primitive::Polyline { .. }
                    | Primitive::Arc { .. }
                    | Primitive::Circle { .. }
                    | Primitive::Text { .. }
                    | Primitive::Point { .. } => {
                        panic!("a dash is a line")
                    }
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(ends(&dashed(-2.0, 8.0, 0.5)), [(-2.0, 0.5, -0.5, 0.5), (0.5, 0.5, 2.0, 0.5), (3.0, 0.5, 4.5, 0.5), (5.5, 0.5, 7.0, 0.5)]);
        assert_eq!(ends(&dashed(-6.0, 1.9, 0.5)).last(), Some(&(1.5, 0.5, 1.9, 0.5)));
        for (a, b, at) in [(-5.5, 0.5, 17.5), (-6.0, 1.9, 0.0), (0.0, 10.0, 3.0)] {
            let h: Vec<_> = ends(&dashed(a, b, at)).into_iter().map(|(x0, _, x1, _)| (at, x0, at, x1)).collect();
            assert_eq!(ends(&dashed_v(at, a, b)), h);
        }
    }

    /// Definition IDs are unique within each block (and overall: `every_starter_is_valid…`).
    #[test]
    fn ids_are_unique_within_their_blocks() {
        let mut by_block: BTreeMap<u128, BTreeSet<u128>> = BTreeMap::new();
        for s in STARTERS {
            let n = s.id.0 - def_id(0).0;
            assert!(by_block.entry(n & !0xff).or_default().insert(n & 0xff), "{} repeats an ID in its block", s.key);
        }
    }

    #[test]
    fn changeover_labels_follow_the_sequence_digit() {
        let set = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<BTreeSet<_>>();
        assert_eq!(changeover_labels(&set(&["A1", "A2"]), "-K01").unwrap(), ["11", "12", "14"]);
        assert_eq!(changeover_labels(&set(&["A1", "11", "12", "14", "23", "24"]), "-K01").unwrap(), ["31", "32", "34"]);
        assert_eq!(changeover_labels(&set(&["91", "92"]), "-K01").unwrap_err().to_string(), "-K01 already has 9 contacts.");
        // Interplay with 018's NO/NC digits, both ways: a change-over after NO and NC takes the
        // next digit, and its `s1`/`s2`/`s4` count as that digit for later contacts.
        assert_eq!(changeover_labels(&set(&["13", "14", "21", "22"]), "-K01").unwrap(), ["31", "32", "34"]);
        assert_eq!(changeover_labels(&set(&["11", "12", "14", "23", "24"]), "-K01").unwrap(), ["31", "32", "34"]);
        // A fuse bank's outputs are skipped for the digit, and their labels are never reused.
        let bank12 = set(&["L+", "M", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12"]);
        let outs = set(&["1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12"]);
        assert_eq!(sequence_labels(&bank12, &outs, &['3', '4'], "-F01").unwrap(), ["13", "14"]);
        assert_eq!(sequence_labels(&bank12, &outs, &['1', '2'], "-F01").unwrap(), ["21", "22"]);
        assert_eq!(sequence_labels(&bank12, &outs, &['1', '2', '4'], "-F01").unwrap(), ["21", "22", "24"]);
        // Without the skip, outputs 11 and 12 would count as contact 1.
        assert_eq!(sequence_labels(&bank12, &BTreeSet::new(), &['3', '4'], "-F01").unwrap(), ["23", "24"]);
        // Labels that are not sequence labels do not count.
        assert_eq!(changeover_labels(&set(&["15", "x1", "1"]), "-K01").unwrap(), ["11", "12", "14"]);
    }

    #[test]
    fn quiet_points_table() {
        let quiet = |key: &str| by_key(key).map(|s| s.labels().into_iter().filter(|l| is_quiet(s, l)).collect::<Vec<_>>()).unwrap_or_default();
        assert_eq!(quiet("fusebank4"), ["M", "1", "2", "3", "4"]);
        assert_eq!(quiet("rj45").len(), 9);
        assert!(quiet("co").is_empty());
        assert!(quiet("coil").is_empty());
    }

    #[test]
    fn every_tagged_starter_has_anchors() {
        for s in STARTERS {
            let a = anchors(s);
            if s.kind == Kind::Tagless {
                assert!(a.is_empty(), "{}", s.key);
                continue;
            }
            assert!(a.iter().any(|x| x.role == TextRole::Tag), "{}", s.key);
            assert!(a.iter().all(|x| x.at.is_finite()), "{}", s.key);
        }
        let coil = anchors(by_key("coil").unwrap());
        assert_eq!(coil[0], Anchor { role: TextRole::Tag, at: p(-8.0, 0.0), h: HAlign::Right, v: VAlign::Middle, rotation: 0.0 });
        let bank = anchors(by_key("fusebank4").unwrap());
        let fuse3 = bank.iter().find(|x| x.role == TextRole::RatingOf("3".into())).unwrap();
        assert_eq!((fuse3.at.x, fuse3.at.y, fuse3.rotation), (23.0, -8.0, 90.0));
        assert_eq!(bank.iter().filter(|x| matches!(x.role, TextRole::RatingOf(_))).count(), 4);
        // RCBO: rating under the tag, description under that.
        let rcbo = anchors(by_key("rcbo2").unwrap());
        let rating = rcbo.iter().find(|x| x.role == TextRole::Rating).unwrap();
        let tag = rcbo.iter().find(|x| x.role == TextRole::Tag).unwrap();
        assert!(rating.at.y < tag.at.y && rating.at.x == tag.at.x);
    }

    #[test]
    fn search_table() {
        let keys = |q: &str| search(q).into_iter().map(|s| s.key.word()).collect::<Vec<_>>();
        assert_eq!(keys("-Q"), ["mcb1", "mcb2", "rcbo2", "rcd2", "rcd4"]);
        // A bare letter is also a substring of an alias (`dq`).
        assert_eq!(keys("q"), ["mcb1", "mcb2", "rcbo2", "rcd2", "rcd4", "plcdq", "chdq"]);
        assert_eq!(
            keys("plc"),
            [
                "di", "do", "plccpu", "plcps", "plcdi", "plcdq", "plcai", "plcao", "plcrtd", "plciol", "plcim", "plcpm", "plccm", "chdi", "chdq",
                "chai", "chao", "chrtd", "chiol"
            ]
        );
        assert_eq!(keys("contact"), ["coil", "no", "nc", "power3", "co", "swco", "power4", "ssrin", "ssrout"]);
        assert_eq!(keys("fuse"), ["fuse", "fuseterm", "fusebank4", "fusebank6", "fusebank8", "fusebank10", "fusebank12", "termfuse"]);
        assert_eq!(keys("protection"), ["rcbo2", "rcd2", "rcd4", "fuseterm", "fusebank4", "fusebank6", "fusebank8", "fusebank10", "fusebank12"]);
        assert!(keys("xyz").is_empty());
        assert!(keys("basic").is_empty());
        assert_eq!(keys("").len(), STARTERS.len());
        assert!(keys(&"z".repeat(10_000)).is_empty());
    }
}
