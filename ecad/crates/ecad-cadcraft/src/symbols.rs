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
//! The catalogue reads no project; only [`setprop_rule`] reads one, to find a fuse bank's fuses.
//! Nothing here changes a project.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use ecad_model::{Definition, DefinitionId, Document, LibraryId, ObjectRef, Origin, Primitive, Pt, Subject, TerminalDef};

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
    Terminal,
    /// A PLC channel: the next address of its letter (`I0.0`…`I0.7`, `I1.0`…).
    Plc { letter: char },
    /// A change-over contact (brief 034): common, NC and NO, numbered by the next IEC sequence
    /// digit (`11`/`12`/`14`, `21`/`22`/`24`…), see [`changeover_labels`].
    ChangeOver,
    /// Fixed labels from the definition. `exclusive`: a device holds at most one of these.
    Fixed { exclusive: bool },
    /// A tagless component of its own, never part of a device: a junction or a continuation
    /// (brief 022, FP4).
    Tagless,
}

/// One starter symbol in the catalogue.
#[derive(Debug, Clone)]
pub struct Starter {
    /// The short key typed at prompts and used in JSON (`coil`, `no`…).
    pub key: &'static str,
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
    pub fn definition(&self) -> Result<&'static Definition, String> {
        static BUILT: OnceLock<BTreeMap<&'static str, Result<Definition, String>>> = OnceLock::new();
        let built = BUILT.get_or_init(|| STARTERS.iter().map(|s| (s.key, s.build())).collect());
        match built.get(self.key) {
            Some(Ok(d)) => Ok(d),
            Some(Err(e)) => Err(e.clone()),
            None => Err(format!("{} is not a starter symbol", self.key)),
        }
    }

    fn build(&self) -> Result<Definition, String> {
        let mut d =
            Definition { bridges: Vec::new(), name: self.name.into(), origin: None, graphics: graphics(self.key), terminals: terminals(self.key) };
        let content_hash = ecad_file::definition_hash(&d).map_err(|e| format!("The symbol {} could not be prepared ({e}).", self.name))?;
        d.origin = Some(Origin { library: STARTER_LIBRARY, definition: self.id, library_seq: 1, content_hash });
        Ok(d)
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
        terminals(self.key).into_iter().map(|t| t.key).collect()
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
const fn st(key: &'static str, name: &'static str, aliases: &'static [&'static str], kind: Kind, id: DefinitionId) -> Starter {
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

/// A connector body (stage 3a): a head, every pin quiet (prefix `X`, `defaults::symbols`).
const fn conn(key: &'static str, name: &'static str, aliases: &'static [&'static str], id: DefinitionId) -> Starter {
    st(key, name, aliases, HEAD, id).of(Family::Connectors).quiet_at(Quiet::All)
}

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
const fn card(key: &'static str, name: &'static str, aliases: &'static [&'static str], id: DefinitionId) -> Starter {
    st(key, name, aliases, HEAD, id).of(Family::Plc)
}

/// The catalogue, in table order (design 018 §4, then design 034 §3–§6). A definition ID is never
/// reused or changed once it has been on `dev`.
pub const STARTERS: &[Starter] = &[
    st("coil", "Contactor / relay coil", &["coil", "relay", "contactor"], HEAD, def_id(1)),
    st("no", "NO contact", &["make", "normally open", "contact"], Kind::Contact { make: true }, def_id(2)),
    st("nc", "NC contact", &["break", "normally closed", "contact"], Kind::Contact { make: false }, def_id(3)),
    st("power3", "Power contacts, 3-pole", &["main contacts", "contactor"], PART, def_id(4)),
    st("mcb1", "MCB 1-pole", &["breaker", "circuit breaker"], HEAD, def_id(5)),
    st("mcb2", "MCB 2-pole", &["breaker", "circuit breaker"], HEAD, def_id(6)),
    st("fuse", "Fuse", &["fuse"], HEAD, def_id(7)),
    st("psu", "Power supply", &["psu", "ac/dc"], HEAD, def_id(8)),
    st("pbno", "Push button, NO", &["push", "button", "start"], Kind::Contact { make: true }, def_id(9)),
    st("pbnc", "Push button, NC", &["push", "button", "stop"], Kind::Contact { make: false }, def_id(10)),
    st("sel", "Selector switch, 2-position", &["selector", "switch"], Kind::Contact { make: true }, def_id(11)),
    st("lamp", "Indicator lamp", &["lamp", "light", "pilot"], HEAD, def_id(12)),
    st("motor3", "Motor, 3-phase", &["motor"], HEAD, def_id(13)),
    st("motor1", "Motor, single-phase", &["motor"], HEAD, def_id(14)),
    st("trafo", "Transformer", &["control transformer"], HEAD, def_id(15)),
    st("terminal", "Terminal", &["terminal", "clamp"], Kind::Terminal, def_id(16)),
    // Brief 028 (design 028 §2): a protective-earth terminal; it joins a strip like `terminal`.
    st("terminalpe", "PE terminal", &["pe terminal", "earth terminal"], Kind::Terminal, def_id(21)),
    st("di", "PLC digital input", &["plc", "input", "di"], Kind::Plc { letter: 'I' }, def_id(17)),
    st("do", "PLC digital output", &["plc", "output", "do"], Kind::Plc { letter: 'Q' }, def_id(18)),
    st("junction", "Junction", &["junction", "dot", "tee"], Kind::Tagless, def_id(19)),
    st("continuation", "Continuation", &["continuation", "arrow", "potential", "link"], Kind::Tagless, def_id(20)),
    // ---- Brief 034 stage 1: contacts and protection (design 034 §3).
    st("co", "Change-over contact", &["change-over", "changeover", "c/o", "spdt", "contact", "relay"], Kind::ChangeOver, cp(1)).of(Family::Contacts),
    st("swco", "Switch, change-over", &["change-over", "switch", "toggle"], Kind::ChangeOver, cp(2)).of(Family::Contacts),
    st("power4", "Power contacts, 4-pole", &["main contacts", "contactor", "4-pole"], PART, cp(3)).of(Family::Contacts),
    st("rcbo2", "RCBO, 1P+N", &["rcbo", "residual current", "breaker", "protection"], HEAD, cp(4)).of(Family::Protection),
    st("rcd2", "RCD, 2-pole", &["rcd", "rccb", "residual current", "protection"], HEAD, cp(5)).of(Family::Protection),
    st("rcd4", "RCD, 4-pole", &["rcd", "rccb", "residual current", "protection"], HEAD, cp(6)).of(Family::Protection),
    st("ssrin", "SSR input", &["ssr", "solid state relay", "relay"], HEAD, cp(7)).of(Family::Contacts),
    st("ssrout", "SSR output", &["ssr", "solid state relay", "contact"], PART, cp(8)).of(Family::Contacts),
    st("fuseterm", "Fuse terminal", &["fuse", "terminal", "fuse holder", "protection"], HEAD, cp(9)).of(Family::Protection),
    st("fusebank4", "Fuse bank, 4-way", FUSE_BANK, HEAD, cp(10)).of(Family::Protection).quiet_at(Quiet::AllBut(&["L+"])),
    st("fusebank6", "Fuse bank, 6-way", FUSE_BANK, HEAD, cp(11)).of(Family::Protection).quiet_at(Quiet::AllBut(&["L+"])),
    st("fusebank8", "Fuse bank, 8-way", FUSE_BANK, HEAD, cp(12)).of(Family::Protection).quiet_at(Quiet::AllBut(&["L+"])),
    st("fusebank10", "Fuse bank, 10-way", FUSE_BANK, HEAD, cp(13)).of(Family::Protection).quiet_at(Quiet::AllBut(&["L+"])),
    st("fusebank12", "Fuse bank, 12-way", FUSE_BANK, HEAD, cp(14)).of(Family::Protection).quiet_at(Quiet::AllBut(&["L+"])),
    // ---- Brief 034 stage 2: sensors and field devices (design 034 §4).
    st("prox3", "Proximity switch, 3-wire PNP", &["proximity", "prox", "inductive", "sensor"], HEAD, sa(1)).of(Family::Sensors),
    st("floatno", "Float switch, NO", &["float", "level", "sensor"], Kind::Contact { make: true }, sa(2)).of(Family::Sensors),
    st("floatnc", "Float switch, NC", &["float", "level", "sensor"], Kind::Contact { make: false }, sa(3)).of(Family::Sensors),
    st("rollerno", "Position switch with roller, NO", &["roller", "limit", "position", "sensor"], Kind::Contact { make: true }, sa(4))
        .of(Family::Sensors),
    st("rollernc", "Position switch with roller, NC", &["roller", "limit", "position", "sensor"], Kind::Contact { make: false }, sa(5))
        .of(Family::Sensors),
    st("interlocknc", "Interlock switch, NC", &["interlock", "guard", "safety", "door switch", "sensor"], Kind::Contact { make: false }, sa(6))
        .of(Family::Sensors),
    st("interlockno", "Interlock switch, NO", &["interlock", "guard", "safety", "sensor"], Kind::Contact { make: true }, sa(7)).of(Family::Sensors),
    st("valve", "Solenoid valve", &["valve", "solenoid", "actuator"], HEAD, sa(8)).of(Family::Actuators),
    st("valve2", "Dual solenoid valve", &["valve", "solenoid", "dual", "actuator"], HEAD, sa(9)).of(Family::Actuators),
    st("tc", "Thermocouple", &["thermocouple", "temperature", "sensor"], HEAD, sa(10)).of(Family::Sensors),
    st("heater", "Heater", &["heater", "heating element", "heat"], HEAD, sa(11)).of(Family::Actuators),
    // ---- Brief 034 stage 3a: connector bodies (design 034 §5); halves only, mating is M1 1.11.
    conn("socket2", "Socket, 2-pin", SOCKET, cn(1)),
    conn("socket3", "Socket, 3-pin", SOCKET, cn(2)),
    conn("socket4", "Socket, 4-pin", SOCKET, cn(3)),
    conn("socket6", "Socket, 6-pin", SOCKET, cn(4)),
    conn("socket8", "Socket, 8-pin", SOCKET, cn(5)),
    conn("socket12", "Socket, 12-pin", SOCKET, cn(6)),
    conn("plug2", "Plug, 2-pin", PLUG, cn(7)),
    conn("plug3", "Plug, 3-pin", PLUG, cn(8)),
    conn("plug4", "Plug, 4-pin", PLUG, cn(9)),
    conn("plug6", "Plug, 6-pin", PLUG, cn(10)),
    conn("plug8", "Plug, 8-pin", PLUG, cn(11)),
    conn("plug12", "Plug, 12-pin", PLUG, cn(12)),
    conn("dsub9s", "D-sub socket, 9-way", DSUB_S, cn(13)),
    conn("dsub15s", "D-sub socket, 15-way", DSUB_S, cn(14)),
    conn("dsub25s", "D-sub socket, 25-way", DSUB_S, cn(15)),
    conn("dsub9p", "D-sub plug, 9-way", DSUB_P, cn(16)),
    conn("dsub15p", "D-sub plug, 15-way", DSUB_P, cn(17)),
    conn("dsub25p", "D-sub plug, 25-way", DSUB_P, cn(18)),
    conn("m12s4", "M12 socket, 4-pin", M12_S, cn(19)),
    conn("m12s5", "M12 socket, 5-pin", M12_S, cn(20)),
    conn("m12s8", "M12 socket, 8-pin", M12_S, cn(21)),
    conn("m12p4", "M12 plug, 4-pin", M12_P, cn(22)),
    conn("m12p5", "M12 plug, 5-pin", M12_P, cn(23)),
    conn("m12p8", "M12 plug, 8-pin", M12_P, cn(24)),
    conn("rj45", "RJ45 bulkhead", &["rj45", "ethernet", "profinet", "bulkhead", "connector"], cn(25)),
    conn("usba", "USB-A bulkhead", USB, cn(26)),
    conn("usbb", "USB-B bulkhead", USB, cn(27)),
    // ---- Brief 034 stage 3b: PLC card bodies (design 034 §6); channels join as `di`/`do`.
    card("plccpu", "PLC CPU", &["plc", "cpu", "controller", "card"], pc(1)),
    card("plcps", "PLC power supply", &["plc", "power", "card"], pc(2)),
    card("plcdi", "PLC digital input card", &["plc", "di", "input", "card"], pc(3)).quiet_at(Quiet::All),
    card("plcdq", "PLC digital output card", &["plc", "dq", "do", "output", "card"], pc(4)).quiet_at(Quiet::All),
    card("plcai", "PLC analogue input card", &["plc", "ai", "analogue", "card"], pc(5)).quiet_at(Quiet::All),
    card("plcao", "PLC analogue output card", &["plc", "ao", "analogue", "card"], pc(6)).quiet_at(Quiet::All),
    card("plcrtd", "PLC RTD card", &["plc", "rtd", "temperature", "card"], pc(7)).quiet_at(Quiet::All),
    card("plciol", "PLC IO-Link master", &["plc", "io-link", "iol", "card"], pc(8)).quiet_at(Quiet::All),
];

/// The starter symbol with key `key` (case-insensitive).
pub fn by_key(key: &str) -> Option<&'static Starter> {
    let k = key.trim();
    STARTERS.iter().find(|s| s.key.eq_ignore_ascii_case(k))
}

/// The starter symbol an embedded definition was made from, by its origin.
pub fn of_definition(d: &Definition) -> Option<&'static Starter> {
    let o = d.origin.as_ref()?;
    if o.library != STARTER_LIBRARY {
        return None;
    }
    STARTERS.iter().find(|s| s.id == o.definition)
}

/// The project's embedded copy of `s`, found by origin (library and definition).
pub fn embedded(doc: &Document, s: &Starter) -> Option<DefinitionId> {
    doc.definitions.iter().find(|(_, d)| d.origin.as_ref().is_some_and(|o| o.library == STARTER_LIBRARY && o.definition == s.id)).map(|(id, _)| *id)
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
            s.key == q
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
pub fn changeover_labels(used: &BTreeSet<String>, who: &str) -> Result<Vec<String>, String> {
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
pub fn sequence_labels(used: &BTreeSet<String>, skip: &BTreeSet<String>, suffixes: &[char], who: &str) -> Result<Vec<String>, String> {
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
    (highest + 1..=9).map(labels).find(|l| !l.iter().any(|x| used.contains(x))).ok_or_else(|| format!("{who} already has 9 contacts."))
}

/// The output labels of device `c`'s fuse bank (its head), which are not contact labels; empty
/// when its head is not a fuse bank.
pub fn bank_outputs(doc: &Document, c: ecad_model::ComponentId) -> BTreeSet<String> {
    doc.representations_of(Subject::Component(c))
        .filter_map(|(_, r)| crate::devices::rep_starter(doc, r))
        .find(|s| s.kind == Kind::Fixed { exclusive: true })
        .filter(|s| is_fuse_bank(s))
        .map(|s| s.labels().into_iter().filter(|l| l.bytes().all(|b| b.is_ascii_digit())).collect())
        .unwrap_or_default()
}

// ---------------------------------------------------------------- ratings (design 034 §7.3)

/// A device's rating as written on the drawing (`13A C 30mA`, `6.3A T`, `K`).
pub const RATING_KEY: &str = "rating";

/// A fuse bank's fuse rating: `rating.<n>`, `<n>` one of its fuse outputs.
pub const RATING_OF_PREFIX: &str = "rating.";

/// Most characters of `<n>` in `rating.<n>` that are read; longer is refused without parsing it
/// (a hostile key cannot make a lookup or a message long).
pub const MAX_FUSE_LABEL: usize = 12;

/// Whether `key` is `rating` or `rating.<n>`.
pub fn is_rating_key(key: &str) -> bool {
    key == RATING_KEY || key.starts_with(RATING_OF_PREFIX)
}

/// Whether `s` is a fuse bank (its fuses take `rating.<n>`).
pub fn is_fuse_bank(s: &Starter) -> bool {
    ways(s.key, "fusebank").is_some()
}

/// Check a `rating` or `rating.<n>` value set on `object` (design 034 §7.3).
///
/// # Semantics
/// `rating` on a device follows the 018 §6.1 value rules (at most 200 characters, no control
/// characters, trimmed; empty or `None` clears). `rating.<n>` follows them too, and only on a
/// device whose head is a fuse bank, with `<n>` one of its fuse outputs; clearing it is always
/// allowed, whatever the device now is. Any other key, or an
/// object that is not a device: no opinion, the value comes back unchanged.
///
/// # Errors
/// A value the 018 rules refuse; `rating.<n>` on a device that is not a fuse bank, or for a fuse
/// it does not have (`-F01 has no fuse 13.`).
pub fn setprop_rule(doc: &Document, object: ObjectRef, key: &str, value: Option<&str>) -> Result<Option<String>, String> {
    let ObjectRef::Component(c) = object else { return Ok(value.map(str::to_string)) };
    if !is_rating_key(key) {
        return Ok(value.map(str::to_string));
    }
    // Clearing always works (review of PR #63, 2): a `rating.<n>` left on a device that is no
    // longer a fuse bank (its bank symbol erased, or an older free property) must stay removable.
    let clearing = value.map(str::trim).is_none_or(str::is_empty);
    if let Some(n) = key.strip_prefix(RATING_OF_PREFIX).filter(|_| !clearing) {
        let who = crate::devices::tag_words(doc, c);
        // Capped before any lookup: only the first 12 characters are ever read.
        let shown: String = n.chars().take(MAX_FUSE_LABEL).collect();
        let long = n.chars().nth(MAX_FUSE_LABEL).is_some();
        let more = if long { "…" } else { "" };
        // The device's head (its one exclusive symbol), and only when that is a fuse bank.
        let bank = doc
            .representations_of(Subject::Component(c))
            .filter_map(|(_, r)| crate::devices::rep_starter(doc, r))
            .find(|s| s.kind == Kind::Fixed { exclusive: true })
            .filter(|s| is_fuse_bank(s));
        let Some(bank) = bank else {
            return Err(format!("rating.{shown}{more} is for a fuse bank's fuses; {who} is not a fuse bank."));
        };
        let has = !long && bank.labels().iter().any(|l| l == n && l.bytes().all(|b| b.is_ascii_digit()));
        if !has {
            return Err(format!("{who} has no fuse {shown}{more}."));
        }
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
        "rcbo2" | "rcd2" | "rcd4" => vec![
            tag,
            a(TextRole::Rating, lo.x - ANCHOR_GAP, mid - ANCHOR_LINE, HAlign::Right, VAlign::Top),
            a(TextRole::Description, lo.x - ANCHOR_GAP, mid - 2.0 * ANCHOR_LINE, HAlign::Right, VAlign::Top),
        ],
        // Card bodies: the tag above, the description inside under the header.
        _ if s.family == Family::Plc => vec![
            a(TextRole::Tag, lo.x, hi.y + ANCHOR_GAP, HAlign::Left, VAlign::Bottom),
            a(TextRole::Description, -13.0, 12.0, HAlign::Left, VAlign::Top),
            rating,
        ],
        _ if s.family == Family::Connectors => vec![tag, description],
        _ => vec![tag, description, rating],
    };
    if is_fuse_bank(s) {
        for t in d.terminals.iter().filter(|t| t.key.bytes().all(|b| b.is_ascii_digit())) {
            out.push(Anchor {
                role: TextRole::RatingOf(t.key.clone()),
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
    Primitive::Text { at: p(at.0, at.1), height, rotation: 0.0, value: value.into() }
}

fn shifted(prims: Vec<Primitive>, dx: f64) -> Vec<Primitive> {
    let m = |q: &Pt| p(q.x + dx, q.y);
    prims
        .into_iter()
        .map(|g| match g {
            Primitive::Line { a, b } => Primitive::Line { a: m(&a), b: m(&b) },
            Primitive::Polyline { vertices, closed } => Primitive::Polyline { vertices: vertices.iter().map(m).collect(), closed },
            Primitive::Circle { center, radius } => Primitive::Circle { center: m(&center), radius },
            other => other,
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
    fn of(key: &str) -> Option<Connector> {
        let dsub = |end: char| key.strip_prefix("dsub")?.strip_suffix(end)?.parse::<usize>().ok().filter(|n| (1..=MAX_WAYS).contains(n));
        match key {
            "rj45" => Some(Connector::Bulkhead(8)),
            "usba" | "usbb" => Some(Connector::Bulkhead(4)),
            _ => ways(key, "socket")
                .map(Connector::Socket)
                .or_else(|| ways(key, "plug").map(Connector::Plug))
                .or_else(|| dsub('s').map(Connector::DsubSocket))
                .or_else(|| dsub('p').map(Connector::DsubPlug))
                .or_else(|| ways(key, "m12s").map(Connector::M12Socket))
                .or_else(|| ways(key, "m12p").map(Connector::M12Plug)),
        }
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
            _ if self.socket() => rect((-3.5, -2.5), (xn + 3.5, 3.5)),
            _ => rect((-3.5, -3.5), (xn + 3.5, 2.5)),
        });
        match self {
            Connector::M12Socket(_) => g.push(circle((-6.5, 0.5), 2.0)),
            Connector::M12Plug(_) => g.push(circle((-6.5, -0.5), 2.0)),
            _ => {}
        }
        g
    }

    fn terminals(self, t: &dyn Fn(&str, f64, f64) -> TerminalDef) -> Vec<TerminalDef> {
        let n = self.pins();
        let y = match self {
            Connector::Bulkhead(_) => 0.0,
            _ if self.socket() => 5.0,
            _ => -5.0,
        };
        let mut v: Vec<TerminalDef> = (1..=n).map(|i| t(&i.to_string(), pin_x(i, PIN_PITCH), y)).collect();
        if let Connector::Bulkhead(_) = self {
            v.push(t("SH", pin_x(n + 1, PIN_PITCH), 0.0));
        }
        v
    }
}

/// A PLC card body (design 034 §6): its type text and its supply points, each with a lead to the
/// body's edge.
fn card_body(type_text: &str, points: &[(f64, f64)]) -> Vec<Primitive> {
    let mut g = vec![rect((-15.0, -20.0), (15.0, 20.0)), line((-15.0, 15.0), (15.0, 15.0)), text((-13.0, 16.5), 2.5, type_text)];
    g.extend(points.iter().map(|&(x, y)| line((x, y), (x, 20.0_f64.copysign(y)))));
    g
}

/// The type text of a card key.
fn card_type(key: &str) -> Option<&'static str> {
    Some(match key {
        "plccpu" => "CPU",
        "plcps" => "PS",
        "plcdi" => "DI",
        "plcdq" => "DQ",
        "plcai" => "AI",
        "plcao" => "AO",
        "plcrtd" => "RTD",
        "plciol" => "IOL",
        _ => return None,
    })
}

/// The most ways or pins a sized symbol key may name (guards the loops below; the catalogue's
/// largest is 25).
const MAX_WAYS: usize = 32;

/// The size a sized key names after `stem` (`fusebank8` → 8), at most [`MAX_WAYS`].
fn ways(key: &str, stem: &str) -> Option<usize> {
    key.strip_prefix(stem)?.parse::<usize>().ok().filter(|n| (1..=MAX_WAYS).contains(n))
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
fn sized_graphics(key: &str) -> Vec<Primitive> {
    if let Some(n) = ways(key, "fusebank") {
        return fuse_bank(n);
    }
    Connector::of(key).map(Connector::graphics).unwrap_or_default()
}

/// Terminals of the sized keys.
fn sized_terminals(key: &str, t: &dyn Fn(&str, f64, f64) -> TerminalDef) -> Vec<TerminalDef> {
    if let Some(n) = ways(key, "fusebank") {
        let mut v = vec![t("L+", -10.0, 20.0), t("M", -10.0, -15.0)];
        v.extend((1..=n).map(|i| t(&i.to_string(), pin_x(i, 10.0), -15.0)));
        return v;
    }
    Connector::of(key).map(|c| c.terminals(t)).unwrap_or_default()
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

fn graphics(key: &str) -> Vec<Primitive> {
    match key {
        "coil" => vec![rect((-6.0, -3.0), (6.0, 3.0)), line((0.0, 3.0), (0.0, 10.0)), line((0.0, -3.0), (0.0, -10.0))],
        "no" => contact_no(),
        "nc" => contact_nc(),
        "power3" => {
            let mut g = Vec::new();
            for x in [-10.0, 0.0, 10.0] {
                g.extend(shifted(contact_no(), x));
            }
            g.extend(dashed(-12.0, 8.0, 0.5));
            g
        }
        "mcb1" => mcb(),
        "mcb2" => {
            let mut g = mcb();
            g.extend(shifted(mcb(), 10.0));
            g.extend(dashed(-2.0, 8.0, 0.5));
            g
        }
        "fuse" => vec![rect((-2.0, -5.0), (2.0, 5.0)), line((0.0, 10.0), (0.0, -10.0))],
        "psu" => {
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
        "pbno" => {
            let mut g = contact_no();
            g.extend(dashed(-8.0, -2.0, 0.5));
            g.push(poly(&[(-10.0, 3.0), (-8.0, 3.0), (-8.0, -2.0), (-10.0, -2.0)], false));
            g
        }
        "pbnc" => {
            let mut g = contact_nc();
            g.extend(dashed(-6.0, 1.9, 0.5));
            g.push(poly(&[(-8.0, 3.0), (-6.0, 3.0), (-6.0, -2.0), (-8.0, -2.0)], false));
            g
        }
        "sel" => {
            let mut g = contact_no();
            g.extend(dashed(-8.0, -2.0, 0.5));
            g.push(poly(&[(-10.0, 3.0), (-8.0, 3.0), (-8.0, -2.0), (-6.0, -2.0)], false));
            g
        }
        "lamp" => vec![
            circle((0.0, 0.0), 4.0),
            line((-2.83, -2.83), (2.83, 2.83)),
            line((-2.83, 2.83), (2.83, -2.83)),
            line((0.0, 4.0), (0.0, 10.0)),
            line((0.0, -4.0), (0.0, -10.0)),
        ],
        "motor3" => motor("3~", &[((-10.0, 15.0), (-5.0, 8.66)), ((0.0, 15.0), (0.0, 10.0)), ((10.0, 15.0), (5.0, 8.66))]),
        "motor1" => motor("1~", &[((-5.0, 15.0), (-5.0, 8.66)), ((5.0, 15.0), (5.0, 8.66))]),
        "trafo" => {
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
        "terminal" => vec![circle((0.0, 0.0), 1.5)],
        // Our own earth mark to the right of the ring (design 028 §2).
        "terminalpe" => vec![circle((0.0, 0.0), 1.5), line((2.5, -1.2), (2.5, 1.2)), line((3.2, -0.8), (3.2, 0.8)), line((3.9, -0.4), (3.9, 0.4))],
        // A filled dot (`ecad.junction-size`): concentric circles, so it reads as filled at any
        // lineweight setting (design 022 §3.1).
        "junction" => vec![circle((0.0, 0.0), 0.5), circle((0.0, 0.0), 0.33), circle((0.0, 0.0), 0.17)],
        // An arrow pointing away from its wire (design 022 §3.2).
        "continuation" => vec![line((0.0, 0.0), (0.0, -5.0)), poly(&[(-1.5, -3.5), (0.0, -5.0), (1.5, -3.5)], false)],
        "di" => vec![rect((-7.5, -2.5), (7.5, 2.5)), text((-2.5, -1.25), 2.5, "DI"), line((0.0, 2.5), (0.0, 10.0))],
        "do" => vec![rect((-7.5, -2.5), (7.5, 2.5)), text((-2.5, -1.25), 2.5, "DO"), line((0.0, -2.5), (0.0, -10.0))],
        // ---- Brief 034 stage 1 (design 034 §3).
        "co" => contact_co(),
        "swco" => {
            let mut g = contact_co();
            g.extend(dashed(-8.0, 1.5, 0.5));
            g.push(line((-8.0, -2.5), (-8.0, 3.5)));
            g
        }
        "power4" => {
            let mut g = Vec::new();
            for x in [-15.0, -5.0, 5.0, 15.0] {
                g.extend(shifted(contact_no(), x));
            }
            g.extend(dashed(-17.0, 13.0, 0.5));
            g
        }
        "rcbo2" | "rcd2" => {
            let mut g = if key == "rcbo2" { mcb() } else { contact_no() };
            g.extend(shifted(contact_no(), 10.0));
            g.extend(dashed(-2.0, 8.0, 0.5));
            g.extend(rcd_release(5.0, 8.0, 0.0, 8.0));
            g
        }
        "rcd4" => {
            let mut g = Vec::new();
            for x in [0.0, 10.0, 20.0, 30.0] {
                g.extend(shifted(contact_no(), x));
            }
            g.extend(dashed(-2.0, 28.0, 0.5));
            g.extend(rcd_release(15.0, 18.0, 20.0, 28.0));
            g
        }
        "ssrin" => vec![
            rect((-6.0, -3.0), (6.0, 3.0)),
            line((0.0, 3.0), (0.0, 10.0)),
            line((0.0, -3.0), (0.0, -10.0)),
            poly(&[(-2.0, 1.5), (2.0, 1.5), (0.0, -1.5)], true),
            line((-2.0, -1.5), (2.0, -1.5)),
        ],
        "ssrout" => {
            let mut g = contact_no();
            g.push(poly(&[(2.0, -1.5), (5.0, -1.5), (3.5, 1.5)], true));
            g
        }
        "fuseterm" => vec![rect((-2.0, -5.0), (2.0, 5.0)), line((0.0, 10.0), (0.0, -10.0)), rect((-3.5, -7.5), (3.5, 7.5))],
        // ---- Brief 034 stage 2 (design 034 §4).
        "prox3" => vec![
            rect((-10.0, -5.0), (10.0, 5.0)),
            poly(&[(-7.0, 0.0), (-5.0, 2.0), (-3.0, 0.0), (-5.0, -2.0)], true),
            line((3.0, 2.5), (3.0, 1.5)),
            line((3.0, -2.5), (3.0, -1.5)),
            line((3.0, -1.5), (1.5, 2.0)),
            line((-5.0, 10.0), (-5.0, 5.0)),
            line((-5.0, -10.0), (-5.0, -5.0)),
            line((5.0, -10.0), (5.0, -5.0)),
        ],
        "floatno" => levered(true, -4.0, 1.5, -5.5),
        "floatnc" => levered(false, -4.0, 1.5, -5.5),
        "rollerno" => levered(true, 3.0, 1.2, 4.2),
        "rollernc" => levered(false, 3.0, 1.2, 4.2),
        "interlocknc" => {
            let mut g = contact_nc();
            g.extend(dashed(-8.0, 1.9, 0.5));
            g.push(circle((-4.0, 0.5), 1.5));
            g.push(line((-5.0, 0.5), (-3.0, 0.5)));
            g.push(poly(&[(-3.6, 1.1), (-3.0, 0.5), (-3.6, -0.1)], false));
            g.extend(key_mark());
            g
        }
        "interlockno" => {
            let mut g = contact_no();
            g.extend(dashed(-8.0, -2.0, 0.5));
            g.extend(key_mark());
            g
        }
        "valve" => vec![
            rect((-6.0, -3.0), (6.0, 3.0)),
            line((0.0, 3.0), (0.0, 10.0)),
            line((0.0, -3.0), (0.0, -10.0)),
            line((6.0, 0.0), (8.0, 0.0)),
            poly(&[(8.0, 3.0), (14.0, -3.0), (14.0, 3.0), (8.0, -3.0)], true),
        ],
        "valve2" => vec![
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
        "tc" => vec![
            poly(&[(-5.0, 10.0), (-5.0, 0.0), (0.0, -5.0)], false),
            poly(&[(5.0, 10.0), (5.0, 0.0), (0.0, -5.0)], false),
            line((5.5, 9.0), (5.5, 0.5)),
            circle((0.0, -5.0), 0.5),
            circle((0.0, -5.0), 0.33),
            circle((0.0, -5.0), 0.17),
        ],
        "heater" => vec![
            rect((-2.5, -6.0), (2.5, 6.0)),
            line((-2.5, -3.0), (2.5, -3.0)),
            line((-2.5, 0.0), (2.5, 0.0)),
            line((-2.5, 3.0), (2.5, 3.0)),
            line((0.0, 6.0), (0.0, 10.0)),
            line((0.0, -6.0), (0.0, -10.0)),
        ],
        // ---- Brief 034 stage 3b (design 034 §6).
        "plcps" => card_body("PS", &[(-10.0, 25.0), (-5.0, 25.0), (0.0, 25.0), (-5.0, -25.0), (5.0, -25.0)]),
        k if card_type(k).is_some() => card_body(card_type(k).unwrap_or_default(), &[(-5.0, 25.0), (5.0, 25.0)]),
        _ => sized_graphics(key),
    }
}

fn terminals(key: &str) -> Vec<TerminalDef> {
    let t = |k: &str, x: f64, y: f64| TerminalDef { key: k.into(), label: Some(k.into()), at: p(x, y) };
    match key {
        "coil" => vec![t("A1", 0.0, 10.0), t("A2", 0.0, -10.0)],
        "no" | "pbno" | "sel" | "floatno" | "rollerno" | "interlockno" => vec![t("13", 0.0, 10.0), t("14", 0.0, -10.0)],
        "nc" | "pbnc" | "floatnc" | "rollernc" | "interlocknc" => vec![t("11", 0.0, 10.0), t("12", 0.0, -10.0)],
        "power3" => vec![t("1", -10.0, 10.0), t("2", -10.0, -10.0), t("3", 0.0, 10.0), t("4", 0.0, -10.0), t("5", 10.0, 10.0), t("6", 10.0, -10.0)],
        "mcb1" | "fuse" => vec![t("1", 0.0, 10.0), t("2", 0.0, -10.0)],
        "mcb2" => vec![t("1", 0.0, 10.0), t("2", 0.0, -10.0), t("3", 10.0, 10.0), t("4", 10.0, -10.0)],
        "psu" => {
            vec![t("L", -10.0, 15.0), t("N", 0.0, 15.0), t("PE", 10.0, 15.0), t("+", -5.0, -15.0), t("-", 5.0, -15.0)]
        }
        "lamp" => vec![t("X1", 0.0, 10.0), t("X2", 0.0, -10.0)],
        "motor3" => vec![t("U1", -10.0, 15.0), t("V1", 0.0, 15.0), t("W1", 10.0, 15.0), t("PE", 20.0, 15.0)],
        "motor1" => vec![t("U1", -5.0, 15.0), t("U2", 5.0, 15.0), t("PE", 20.0, 15.0)],
        "trafo" => vec![t("1U1", -5.0, 15.0), t("1U2", 5.0, 15.0), t("2U1", -5.0, -15.0), t("2U2", 5.0, -15.0)],
        "terminal" | "terminalpe" => vec![t("1", 0.0, 0.0)],
        // No label: a junction or continuation shows no connection point name.
        "junction" | "continuation" => vec![TerminalDef { key: "1".into(), label: None, at: p(0.0, 0.0) }],
        "di" => vec![t("I0.0", 0.0, 10.0)],
        "do" => vec![t("Q0.0", 0.0, -10.0)],
        // ---- Brief 034 stage 1: definition order is common, NC, NO (design 034 §2.3).
        "co" | "swco" => vec![t("11", 0.0, 10.0), t("12", 5.0, -10.0), t("14", 0.0, -10.0)],
        "power4" => vec![
            t("1", -15.0, 10.0),
            t("2", -15.0, -10.0),
            t("3", -5.0, 10.0),
            t("4", -5.0, -10.0),
            t("5", 5.0, 10.0),
            t("6", 5.0, -10.0),
            t("7", 15.0, 10.0),
            t("8", 15.0, -10.0),
        ],
        "rcbo2" | "rcd2" => vec![t("1", 0.0, 10.0), t("2", 0.0, -10.0), t("N1", 10.0, 10.0), t("N2", 10.0, -10.0)],
        "rcd4" => vec![
            t("1", 0.0, 10.0),
            t("2", 0.0, -10.0),
            t("3", 10.0, 10.0),
            t("4", 10.0, -10.0),
            t("5", 20.0, 10.0),
            t("6", 20.0, -10.0),
            t("N1", 30.0, 10.0),
            t("N2", 30.0, -10.0),
        ],
        "ssrin" => vec![t("A1", 0.0, 10.0), t("A2", 0.0, -10.0)],
        "ssrout" | "fuseterm" | "heater" => vec![t("1", 0.0, 10.0), t("2", 0.0, -10.0)],
        // ---- Brief 034 stage 2: IEC 60947-5-2's numbers (1 supply +, 3 supply −, 4 output).
        "prox3" => vec![t("1", -5.0, 10.0), t("3", -5.0, -10.0), t("4", 5.0, -10.0)],
        "valve" => vec![t("A1", 0.0, 10.0), t("A2", 0.0, -10.0)],
        "valve2" => vec![t("A1", -10.0, 10.0), t("A2", -10.0, -10.0), t("B1", 10.0, 10.0), t("B2", 10.0, -10.0)],
        "tc" => vec![t("+", -5.0, 10.0), t("-", 5.0, 10.0)],
        // ---- Brief 034 stage 3b.
        "plcps" => vec![t("L", -10.0, 25.0), t("N", -5.0, 25.0), t("PE", 0.0, 25.0), t("L+", -5.0, -25.0), t("M", 5.0, -25.0)],
        "plccpu" | "plcdi" | "plcdq" | "plcai" | "plcao" | "plcrtd" | "plciol" => vec![t("L+", -5.0, 25.0), t("M", 5.0, 25.0)],
        _ => sized_terminals(key, &t),
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
    for t in &d.terminals {
        add(t.at);
    }
    if !(lo.is_finite() && hi.is_finite()) {
        return (p(0.0, 0.0), p(0.0, 0.0));
    }
    (lo, hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_starter_is_valid_and_on_the_grid() {
        let mut keys = std::collections::BTreeSet::new();
        let mut ids = std::collections::BTreeSet::new();
        for s in STARTERS {
            assert!(keys.insert(s.key) && ids.insert(s.id), "{} repeated", s.key);
            let d = s.definition().unwrap();
            assert!(!d.graphics.is_empty() && !d.terminals.is_empty(), "{}", s.key);
            for t in &d.terminals {
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
        // 18 symbols (brief 018), the junction and continuation (brief 022), the PE terminal (028),
        // brief 034's stage 1 (14), stage 2 (11), stage 3a (27) and stage 3b (8).
        assert_eq!(STARTERS.len(), 21 + 14 + 11 + 27 + 8);
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
        ("plccpu", 30.0, 45.0, "L+/M"),
        ("plcps", 30.0, 50.0, "L/N/PE/L+/M"),
        ("plcdi", 30.0, 45.0, "L+/M"),
        ("plcdq", 30.0, 45.0, "L+/M"),
        ("plcai", 30.0, 45.0, "L+/M"),
        ("plcao", 30.0, 45.0, "L+/M"),
        ("plcrtd", 30.0, 45.0, "L+/M"),
        ("plciol", 30.0, 45.0, "L+/M"),
    ];

    #[test]
    fn new_symbols_have_the_designed_box_and_labels() {
        for (key, w, h, labels) in NEW {
            let s = by_key(key).unwrap();
            let (sw, sh) = s.size();
            assert!((sw - w).abs() < 1e-6 && (sh - h).abs() < 1e-6, "{key}: box {sw}×{sh}, want {w}×{h}");
            assert_eq!(s.labels().join("/"), *labels, "{key}");
        }
        assert_eq!(STARTERS.iter().filter(|s| s.family != Family::Basic).count(), NEW.len());
    }

    /// Overseer ruling 4: the 21 starters of briefs 018, 022 and 028 keep their content hashes
    /// (a changed hash would make every old project's embedded copy a different symbol).
    #[test]
    fn the_basic_starters_keep_their_content_hashes() {
        let want: &[(&str, &str)] = &[
            ("coil", "a38469fe64864a4a602f678c8bb2fae900c64b75c555a3b58b65b98029437db3"),
            ("no", "82317c626ba389fa1deb1ecbefc1457e0b958da637780813198b4969e6d4cc31"),
            ("nc", "8861d1f37368786878b2c860ce0bec1e98f4b8fb4b264fdf6b897cd01a38717c"),
            ("power3", "7554cf8d849ccf964912b5ceef2248e342c00351cd386f88bd00f27aff9c599b"),
            ("mcb1", "60a708b75ab297e22e8ee0e3e5b99163660dbf7f941ca0eba4d7009095042ecb"),
            ("mcb2", "907da0b24254cc9520f359285988b9706420730a46e626c73abf20a3434ddd73"),
            ("fuse", "92a1eb42f8036488fa6e127903f9381488777bb1314c272c97c7fb6c565f0f93"),
            ("psu", "7ab24232dfdaa94a6b28556544b3be0b128146f6cd314f7a2e4fef86df03bc6c"),
            ("pbno", "6fcb99af92cd7290c81fb4e05291bef164c4354e358a959490d4b1dece901148"),
            ("pbnc", "eb97ba69342ae1de571c128a9086531e499581436fb5cc8c0f223c98d881479f"),
            ("sel", "ee747f4614ce340e0500223ec3f23a1463c80ef9ec42f5e425be6449dedf3023"),
            ("lamp", "654a895a70d7a93b61a1dfd279243afcd45628a7835bc7ca7baf3419e6af3e5a"),
            ("motor3", "74fba3648ea65faed6d0f2d6122333c8ddbe82593551300b8b39b84e2783db94"),
            ("motor1", "97d78f1600e761449dda417ba7a0d8818d1fc0a7576bd56f5a68bd2665714dcb"),
            ("trafo", "7546ea16e7c4b9d68527ee6e022ad642a825844ab7441c699c2200015d736016"),
            ("terminal", "140b3e17fdb18a4889a5816b1f3aa34f9c9714643d25f518470cd800310290a7"),
            ("terminalpe", "950f2596fe76913d2ac50cc894697b3ca80e10cfb3bcbe136dccbc61af844a3c"),
            ("di", "691b84067c9c64dd0ab9de2a9a6bef23fb05c71a485264cf463f245ef39fc1eb"),
            ("do", "736535bf22162769b6420f790efd350383060df36447dcee6d29cf1f86f1ff92"),
            ("junction", "8a6e36eab66ded66ec4ef470682d2d5cecf536a0d1fe6c148c632154232a4868"),
            ("continuation", "7289a1f63b563b3f0ceb6959fd7fb34763daf4c1c6e56bb2366e01129ae7afaf"),
        ];
        let basic: Vec<_> = STARTERS.iter().filter(|s| s.family == Family::Basic).collect();
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
                    _ => panic!("a dash is a line"),
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
        assert_eq!(changeover_labels(&set(&["91", "92"]), "-K01").unwrap_err(), "-K01 already has 9 contacts.");
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
        let keys = |q: &str| search(q).into_iter().map(|s| s.key).collect::<Vec<_>>();
        assert_eq!(keys("-Q"), ["mcb1", "mcb2", "rcbo2", "rcd2", "rcd4"]);
        // A bare letter is also a substring of an alias (`dq`).
        assert_eq!(keys("q"), ["mcb1", "mcb2", "rcbo2", "rcd2", "rcd4", "plcdq"]);
        assert_eq!(keys("plc"), ["di", "do", "plccpu", "plcps", "plcdi", "plcdq", "plcai", "plcao", "plcrtd", "plciol"]);
        assert_eq!(keys("contact"), ["coil", "no", "nc", "power3", "co", "swco", "power4", "ssrin", "ssrout"]);
        assert_eq!(keys("fuse"), ["fuse", "fuseterm", "fusebank4", "fusebank6", "fusebank8", "fusebank10", "fusebank12"]);
        assert_eq!(keys("protection"), ["rcbo2", "rcd2", "rcd4", "fuseterm", "fusebank4", "fusebank6", "fusebank8", "fusebank10", "fusebank12"]);
        assert!(keys("xyz").is_empty());
        assert!(keys("basic").is_empty());
        assert_eq!(keys("").len(), STARTERS.len());
        assert!(keys(&"z".repeat(10_000)).is_empty());
    }
}
