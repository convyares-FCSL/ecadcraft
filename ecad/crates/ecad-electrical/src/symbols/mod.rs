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

pub mod anchors;
pub mod builder;
pub mod catalogue;
pub mod cells;
pub mod commands;
pub mod graphics;
pub mod offers;
pub mod project;
pub mod proposals;
pub mod reps;
pub mod rules;
use self::{catalogue::*, cells::*, graphics::*, project::*, rules::*};
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
pub(crate) const SOCKET_KEY: PointKey = PointKey::Side(ecad_model::Side::Socket);
/// The terminal key of a plug pin (design 044 FP1).
pub(crate) const PLUG_KEY: PointKey = PointKey::Side(ecad_model::Side::Plug);

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
        Fuseterminalblock,
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
        Terminalblockfuse,
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
            | K::Fuseterminalblock
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
            | K::Terminalblockfuse => DeviceGroup::TerminalStrip,
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
            | K::Fuseterminalblock
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
            | K::Terminalblockfuse
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
            Kind::Fixed { exclusive: false } if !crate::references::devices::classify::poles_of(self).is_empty() => PartRole::Contact,
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
            } else if self.key == K::Fuseterminalblock {
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
