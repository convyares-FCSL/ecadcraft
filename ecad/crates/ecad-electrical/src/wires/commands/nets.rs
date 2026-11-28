//! The net, wire number, potential, category, wire settings, renumber and check commands: params in, a typed
//! reply out. Generic ([`crate::commands::run`]); the rules are in [`super::wires`].
//!
//! Excludes: `wire` and `joinwires`, which draw (the engine's session commands).

use std::collections::BTreeSet;

use ecad_model::connectivity::wiring;
use ecad_model::geometry::colour::ColourWords;
use ecad_model::{AidId, ConnectionPointId, Op, PageId, WireId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::wires::{self as rules, Conflict, NetChoice, Numbered, PotentialRow, WireSettingsChange, Wires};
use crate::checks::CheckRow;
use crate::commands::input::MAX_HANDLES;
use crate::commands::input::{Bounded, nullable};
use crate::commands::run::{Access, Cx, Generic, Outcome, Pages, Reply, Request};
use crate::error::CmdError;

// ---------------------------------------------------------------- nets and net

/// The project's nets, a page at a time. Read only.
#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct NetsParams {
    /// Only nets drawn on this page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<PageId>,
    /// Every net with this wire number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<String>,
    /// The net of this wire.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conductor: Option<WireId>,
    /// The net of this connection point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_point: Option<ConnectionPointId>,
    /// Also nets without a wire.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub all: Option<bool>,
    /// At most this many (default 200, at most 2000).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    /// The cursor `next` gave.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

/// One page of the project's nets.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub struct NetsReply {
    /// The nets, in listing order.
    pub nets: Vec<NetReply>,
    /// The cursor for the next page (absent: this is the last).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
}
impl Reply for NetsReply {}

/// `nets`: the project's nets, a page at a time.
pub enum Nets {}
impl Request for Nets {
    const ID: &'static str = "nets";
    type Params = NetsParams;
    type Reply = NetsReply;
}

/// One net, by a wire, a connection point or a wire number. Read only.
#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct NetParams {
    /// The net of this wire.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conductor: Option<WireId>,
    /// The net of this connection point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_point: Option<ConnectionPointId>,
    /// The (first) net with this wire number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<String>,
}

/// A wire category as a net shows it.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub struct CategoryName {
    /// Its ID.
    pub id: String,
    /// Its name.
    pub name: String,
}

/// A device connection point of a net.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub struct NetPoint {
    /// The connection point.
    pub connection_point: ConnectionPointId,
    /// It in words (`-K01:A1`).
    pub label: String,
}

/// A page a net is drawn on.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub struct NetPage {
    /// The page number as shown.
    pub page: String,
    /// The column of the net's first route start there.
    pub column: Option<String>,
}

/// One net.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub struct NetReply {
    /// Its wire number (none: not numbered yet).
    pub number: Option<String>,
    /// Whether its number is fixed.
    pub fixed: bool,
    /// Its potential.
    pub potential: Option<String>,
    /// The category every wire of the net has (none: there is none, or they differ).
    pub category: Option<CategoryName>,
    /// The categories, when the net's wires resolve to different ones or some have none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub categories: Option<Vec<CategoryName>>,
    /// Its wires.
    pub conductors: Vec<WireId>,
    /// Its device connection points (not continuations).
    pub points: Vec<NetPoint>,
    /// The names of its continuations.
    pub continuations: BTreeSet<String>,
    /// The pages it is drawn on, in page order.
    pub pages: Vec<NetPage>,
}
impl Reply for NetReply {}

/// `net`: one net.
pub enum Net {}
impl Request for Net {
    const ID: &'static str = "net";
    type Params = NetParams;
    type Reply = NetReply;
}

// ---------------------------------------------------------------- wirenumber

/// What to do when another net uses the number.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ConflictParam {
    /// That net takes this one's number.
    Swap,
    /// This net takes the next free number.
    Another,
    /// Join the nets with a continuation pair named for the number.
    Merge,
}

/// Fixes a net's number (`value`; null releases it), or fixes or releases its current one (`{conductor, fixed}`).
/// The net is named by a wire, a connection point or its number. A number another net uses needs `conflict`.
#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct WirenumberParams {
    /// A wire of the net.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conductor: Option<WireId>,
    /// A connection point of the net.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_point: Option<ConnectionPointId>,
    /// The net's current number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<String>,
    /// The number to fix, or null to release the fixed number.
    #[serde(default, deserialize_with = "nullable", skip_serializing_if = "Option::is_none")]
    pub value: Option<Option<String>>,
    /// Without `value`: true fixes the net's current number, false releases it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixed: Option<bool>,
    /// When another net uses the number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conflict: Option<ConflictParam>,
}

/// What `wirenumber` did.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub struct WirenumberReply {
    /// The net's number now.
    pub number: String,
    /// Whether it is fixed.
    pub fixed: bool,
    /// How many wires the net has (absent when nets were merged).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conductors: Option<usize>,
    /// False when nothing changed (the number was not fixed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changed: Option<bool>,
    /// The number the other net took (`conflict: "swap"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub swapped: Option<String>,
    /// True when the nets were joined (`conflict: "merge"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merged: Option<bool>,
    /// The continuation pair that joins them (`conflict: "merge"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuations: Option<Vec<AidId>>,
}
impl Reply for WirenumberReply {}

impl WirenumberReply {
    fn of(number: String, fixed: bool, conductors: usize) -> WirenumberReply {
        WirenumberReply { number, fixed, conductors: Some(conductors), changed: None, swapped: None, merged: None, continuations: None }
    }
}

/// `wirenumber`: fix or release a net's number.
pub enum Wirenumber {}
impl Request for Wirenumber {
    const ID: &'static str = "wirenumber";
    type Params = WirenumberParams;
    type Reply = WirenumberReply;
}

// ---------------------------------------------------------------- potential

/// Sets the potential of a wire's whole net, or clears it with null.
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PotentialParams {
    /// A wire of the net.
    pub conductor: WireId,
    /// The potential (24V, L1…), or null to clear it.
    #[serde(deserialize_with = "nullable", skip_serializing_if = "Option::is_none")]
    #[schemars(required, with = "crate::commands::input::OrNull<String>")]
    pub value: Option<Option<String>>,
}

/// What `potential` did.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub struct PotentialReply {
    /// The net's potential now (none: cleared).
    pub potential: Option<String>,
    /// How many wires the net has.
    pub conductors: usize,
    /// The wire's number now.
    pub number: Option<String>,
}
impl Reply for PotentialReply {}

/// `potential`: set or clear a net's potential.
pub enum Potential {}
impl Request for Potential {
    const ID: &'static str = "potential";
    type Params = PotentialParams;
    type Reply = PotentialReply;
}

// ---------------------------------------------------------------- category

/// Sets the wire category of one wire, of a wire's whole net (`net`) or of several wires, or clears the wires'
/// own with null. One transaction.
#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct CategoryParams {
    /// One wire.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conductor: Option<WireId>,
    /// A wire whose whole net takes the category.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub net: Option<WireId>,
    /// These wires.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conductors: Option<Bounded<WireId, 0, { MAX_HANDLES }>>,
    /// The wire category (its ID or name), or null to clear the wires' own.
    #[serde(deserialize_with = "nullable", skip_serializing_if = "Option::is_none")]
    #[schemars(required, with = "crate::commands::input::OrNull<String>")]
    pub value: Option<Option<String>>,
}

/// What `category` did.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub struct CategoryReply {
    /// Whether any wire changed.
    pub changed: bool,
    /// How many wires it acted on.
    pub conductors: usize,
    /// The category set, its ID (none: cleared).
    pub category: Option<String>,
    /// What to tell the person, when a net keeps its own potential.
    pub message: Option<String>,
}
impl Reply for CategoryReply {}

/// `category`: set or clear wires' category.
pub enum Category {}
impl Request for Category {
    const ID: &'static str = "category";
    type Params = CategoryParams;
    type Reply = CategoryReply;
}

// ---------------------------------------------------------------- wiresettings

/// A supply as typed text or as fields.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum SupplyInput {
    /// As typed (`"400/230 V 3~ 50 Hz"`).
    Text(String),
    /// As fields.
    Fields(ecad_model::connectivity::supply::Supply),
}

/// One row of the potentials table.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PotentialRowParam {
    /// The potential's name (24V, L1, PE…).
    #[serde(default)]
    pub name: String,
    /// The prefix of its wire numbers.
    #[serde(default)]
    pub prefix: String,
    /// Whether its wires are numbered (default true).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub numbered: Option<bool>,
    /// What feeds it, as typed (`"400/230 V 3~ 50 Hz"`, `"230 V 1~"`, `"24 V DC"`) or as fields
    /// (`{system: "ac3", line_to_line: 400, line_to_neutral?: 230, frequency?: 50}`,
    /// `{system: "ac1", line_to_neutral: 230, frequency?: 50}`, `{system: "dc", nominal: 24}`).
    /// Absent: not known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supply: Option<SupplyInput>,
    /// Refused: colours belong to wire categories now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colour: Option<Value>,
    /// Refused: cross-sections belong to wire categories now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cross_section: Option<Value>,
}

/// The project's wire numbering and label settings. No parameters reads them. Colours are IEC 60757 codes by
/// default; free text is allowed.
#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct WiresettingsParams {
    /// How wires are numbered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheme: Option<SchemeWord>,
    /// Digits of a wire number's sequence, 1 to 6.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digits: Option<u64>,
    /// The prefix of wires of no potential.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unclassified_prefix: Option<String>,
    /// The whole potentials table (colours and cross-sections are wire categories').
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub potentials: Option<Vec<PotentialRowParam>>,
    /// How colours are written: code2 (BN, BU, GN/YE), code3 (BRN, BLU, GRN/YEL) or name (Brown,
    /// Blue, Green/Yellow).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colour_style: Option<ColourWords>,
    /// Wire label height in mm, 0.5 to 10.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_height: Option<f64>,
    /// Which wires get a label: net (one label per net per page) or each (every wire).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_rule: Option<LabelRuleWord>,
    /// The shortest labelled wire in mm, 0 to 100.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_min_length: Option<f64>,
    /// What one wire number counts: net (every wire of a net shows the net's number) or
    /// conductor (every wire its own). Switching renumbers every net in the same edit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number_by: Option<wiring::NumberBy>,
}

ecad_model::word_enum! {
    /// How wires are numbered (`wiresettings`'s `scheme`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum SchemeWord {
        /// `24V-07`: one sequence per prefix, project-wide.
        Potential,
        /// `24V-105.03`: one sequence per prefix and page.
        Page,
        /// `105C.1`: one sequence per prefix, page and column.
        #[strum(serialize = "page-column")]
        PageColumn,
    }
}

impl From<SchemeWord> for wiring::Scheme {
    fn from(w: SchemeWord) -> Self {
        match w {
            SchemeWord::Potential => wiring::Scheme::Potential,
            SchemeWord::Page => wiring::Scheme::Page,
            SchemeWord::PageColumn => wiring::Scheme::PageColumn,
        }
    }
}

impl From<wiring::Scheme> for SchemeWord {
    fn from(s: wiring::Scheme) -> Self {
        match s {
            wiring::Scheme::Potential => SchemeWord::Potential,
            wiring::Scheme::Page => SchemeWord::Page,
            wiring::Scheme::PageColumn => SchemeWord::PageColumn,
        }
    }
}

ecad_model::word_enum! {
    /// Which wires get a label (`wiresettings`'s `label_rule`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum LabelRuleWord {
        /// One label per net per page.
        Net,
        /// Every wire.
        Each,
    }
}

/// One row of the potentials table, as `wiresettings` shows it.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub struct PotentialSetting {
    /// The potential's name.
    pub name: String,
    /// The prefix of its wire numbers.
    pub prefix: String,
    /// Whether its wires are numbered.
    pub numbered: bool,
    /// What feeds it (none: not known).
    pub supply: Option<ecad_model::connectivity::supply::Supply>,
}

/// The project's wire numbering and label settings (the label rule as resolved).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub struct WiresettingsReply {
    /// How wires are numbered.
    pub scheme: SchemeWord,
    /// How colours are written.
    pub colour_style: ColourWords,
    /// Wire label height in mm.
    pub label_height: f64,
    /// Which wires get a label.
    pub label_rule: LabelRuleWord,
    /// The shortest labelled wire in mm.
    pub label_min_length: f64,
    /// Digits of a wire number's sequence.
    pub digits: usize,
    /// What one wire number counts.
    pub number_by: wiring::NumberBy,
    /// The prefix of wires of no potential.
    pub unclassified_prefix: String,
    /// The potentials table.
    pub potentials: Vec<PotentialSetting>,
}
impl Reply for WiresettingsReply {}

/// `wiresettings`: read or change the wire numbering and label settings.
pub enum Wiresettings {}
impl Request for Wiresettings {
    const ID: &'static str = "wiresettings";
    type Params = WiresettingsParams;
    type Reply = WiresettingsReply;
}

// ---------------------------------------------------------------- renumberwires

/// Which wires `renumberwires` renumbers.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    /// Only wires without a number (the default).
    Unnumbered,
    /// Every wire without a fixed number.
    All,
}

/// Numbers the wires; fixed numbers never change.
#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct RenumberwiresParams {
    /// Which wires (default "unnumbered").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<Scope>,
}

/// What `renumberwires` did.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub struct RenumberwiresReply {
    /// How many nets were numbered.
    pub renumbered: usize,
}
impl Reply for RenumberwiresReply {}

/// `renumberwires`: number the wires.
pub enum Renumberwires {}
impl Request for Renumberwires {
    const ID: &'static str = "renumberwires";
    type Params = RenumberwiresParams;
    type Reply = RenumberwiresReply;
}

// ---------------------------------------------------------------- checks

/// The Checks rows, a page at a time. Read only.
#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct ChecksParams {
    /// Only these kinds of check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kinds: Option<Vec<String>>,
    /// At most this many (default 200, at most 2000).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    /// Skip this many (the `next` a previous call gave).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<usize>,
}

/// One page of the Checks rows.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub struct ChecksReply {
    /// The rows, in book order.
    pub checks: Vec<CheckRow>,
    /// How many rows there are in all.
    pub total: usize,
    /// The `after` of the next page (absent: this is the last).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<usize>,
}
impl Reply for ChecksReply {}

/// `checks`: the Checks rows.
pub enum Checks {}
impl Request for Checks {
    const ID: &'static str = "checks";
    type Params = ChecksParams;
    type Reply = ChecksReply;
}

// ---------------------------------------------------------------- the commands

impl Generic for Nets {
    const TITLE: &'static str = "List Nets";
    const PAGES: Pages = Pages::Any;
    const ACCESS: Access = Access::Reads;

    fn run(p: NetsParams, cx: &mut Cx<'_>) -> Result<Outcome<NetsReply>, CmdError> {
        let q = rules::NetsQuery {
            page: p.page,
            choice: NetChoice { conductor: p.conductor, connection_point: p.connection_point, number: p.number },
            all: p.all.unwrap_or(false),
            limit: p.limit.unwrap_or(rules::DEFAULT_LIMIT).clamp(1, rules::MAX_LIMIT),
            after: p.after,
        };
        let found = rules::nets(cx.doc, &q)?;
        Ok(Outcome::answer(NetsReply { nets: found.nets, next: found.next }))
    }
}

impl Generic for Net {
    const TITLE: &'static str = "Show Net";
    const PAGES: Pages = Pages::Any;
    const ACCESS: Access = Access::Reads;

    fn run(p: NetParams, cx: &mut Cx<'_>) -> Result<Outcome<NetReply>, CmdError> {
        let choice = NetChoice { conductor: p.conductor, connection_point: p.connection_point, number: p.number };
        Ok(Outcome::answer(rules::net(cx.doc, &choice)?))
    }
}

impl Generic for Wirenumber {
    const TITLE: &'static str = "Fix Wire Number";

    fn check(p: &WirenumberParams, _registry: &ecad_model::registry::data::Registry) -> Result<(), CmdError> {
        if p.value.is_none() && p.fixed.is_none() {
            return Err(CmdError::refused("`value` (text, or null) is required"));
        }
        if let Some(Some(v)) = &p.value {
            wiring::valid_number(v).map_err(|e| CmdError::refused(e.to_string()))?;
        }
        Ok(())
    }

    fn run(p: WirenumberParams, cx: &mut Cx<'_>) -> Result<Outcome<WirenumberReply>, CmdError> {
        Self::check(&p, cx.registry)?;
        let choice = NetChoice { conductor: p.conductor, connection_point: p.connection_point, number: p.number };
        // `fixed` alone (the Properties panel's Fixed box): fix the net's current number, or
        // release it.
        let value = match (p.value, p.fixed) {
            (Some(v), _) => Some(v),
            (None, Some(false)) => Some(None),
            _ => None,
        };
        let value = match value {
            Some(Some(v)) => Some(Some(wiring::valid_number(&v).map_err(|e| CmdError::refused(e.to_string()))?)),
            other => other,
        };
        let conflict = p.conflict.map(|c| match c {
            ConflictParam::Swap => Conflict::Swap,
            ConflictParam::Another => Conflict::Another,
            ConflictParam::Merge => Conflict::Merge,
        });
        let doc = cx.doc;
        let value = match value {
            Some(v) => v,
            None => Some(wiring::valid_number(&rules::current_number(doc, &choice)?)?),
        };
        let done = rules::wirenumber(doc, cx.ids, cx.plain.clone(), &choice, value.as_deref(), conflict)?;
        let (ops, label) = done.ops();
        let (ops, label) = (ops.to_vec(), label);
        let reply = match done {
            Numbered::Released { ops, number, conductors } if ops.is_empty() => {
                WirenumberReply { changed: Some(false), ..WirenumberReply::of(number, false, conductors) }
            }
            Numbered::Released { number, conductors, .. } => WirenumberReply::of(number, false, conductors),
            Numbered::Fixed { number, conductors, .. } | Numbered::Another { number, conductors, .. } => {
                WirenumberReply::of(number, true, conductors)
            }
            Numbered::Swapped { number, conductors, swapped, .. } => {
                WirenumberReply { swapped: Some(swapped), ..WirenumberReply::of(number, true, conductors) }
            }
            Numbered::Merged { number, continuations, .. } => WirenumberReply {
                number,
                fixed: true,
                conductors: None,
                changed: None,
                swapped: None,
                merged: Some(true),
                continuations: Some(continuations),
            },
        };
        Ok(Outcome::edit_if_any(label, ops, reply))
    }
}

impl Generic for Potential {
    const TITLE: &'static str = "Set Potential";

    fn run(p: PotentialParams, cx: &mut Cx<'_>) -> Result<Outcome<PotentialReply>, CmdError> {
        let value = match p.value.flatten() {
            Some(v) => wiring::valid_text("potential", Some(&v)).map_err(|e| CmdError::refused(e.to_string()))?,
            None => None,
        };
        let c = p.conductor;
        let (ops, conductors) = rules::potential(cx.doc, c, value.as_deref())?;
        let number = rules::number_of(cx.doc, c);
        Ok(Outcome::edit_if_any("Set potential", ops, PotentialReply { potential: value, conductors, number }))
    }
}

impl Generic for Category {
    const TITLE: &'static str = "Wire Category";

    fn run(p: CategoryParams, cx: &mut Cx<'_>) -> Result<Outcome<CategoryReply>, CmdError> {
        let value = p.value.flatten();
        let which = match (p.conductor, p.net, p.conductors.map(|b| b.0)) {
            (Some(c), None, None) => Wires::One(c),
            (None, Some(c), None) => Wires::Net(c),
            (None, None, Some(v)) => Wires::Many(v),
            _ => return Err(CmdError::input("Give one of `conductor`, `net` or `conductors`.")),
        };
        let done = rules::category(cx.doc, which, value.as_deref())?;
        let reply =
            CategoryReply { changed: !done.ops.is_empty(), conductors: done.wires, category: done.category.map(|i| i.text()), message: done.message };
        Ok(Outcome::edit_if_any(done.label, done.ops, reply))
    }
}

impl Generic for Wiresettings {
    const TITLE: &'static str = "Wire Settings";

    fn run(p: WiresettingsParams, cx: &mut Cx<'_>) -> Result<Outcome<WiresettingsReply>, CmdError> {
        let rows = p
            .potentials
            .map(|rows| {
                rows.into_iter()
                    .map(|r| {
                        let supply = match r.supply {
                            None => None,
                            Some(SupplyInput::Fields(s)) => Some(s),
                            Some(SupplyInput::Text(t)) => Some(
                                crate::devices::supply::read(&t)
                                    .map_err(|e| CmdError::refused(format!("{}: '{t}' does not read: {e}", r.name.trim())))?,
                            ),
                        };
                        Ok(PotentialRow {
                            name: r.name,
                            prefix: r.prefix,
                            numbered: r.numbered,
                            supply,
                            colours: r.colour.is_some() || r.cross_section.is_some(),
                        })
                    })
                    .collect::<Result<Vec<_>, CmdError>>()
            })
            .transpose()?;
        let change = WireSettingsChange {
            scheme: p.scheme.map(wiring::Scheme::from),
            digits: p.digits,
            colour_style: p.colour_style,
            label_height: p.label_height,
            label_rule: p.label_rule.map(|r| ecad_model::Word::word(r).to_string()),
            label_min_length: p.label_min_length,
            number_by: p.number_by,
            unclassified_prefix: p.unclassified_prefix,
            potentials: rows,
        };
        if change == WireSettingsChange::default() {
            return Ok(Outcome::answer(rules::settings(cx.doc, cx.registry)));
        }
        let before = cx.doc.settings().clone();
        let after = rules::wire_settings(&before, &change)?;
        let ops = if after != before { vec![Op::SetProjectSettings { before, after }] } else { Vec::new() };
        let registry = cx.registry;
        Ok(Outcome::edit_then("Wire settings", ops, move |doc| rules::settings(doc, registry)))
    }
}

impl Generic for Renumberwires {
    const TITLE: &'static str = "Renumber Wires";

    fn run(p: RenumberwiresParams, cx: &mut Cx<'_>) -> Result<Outcome<RenumberwiresReply>, CmdError> {
        let (ops, nets) = rules::renumber(cx.doc, p.scope == Some(Scope::All));
        if ops.is_empty() {
            return Ok(Outcome::answer(RenumberwiresReply { renumbered: 0 }));
        }
        Ok(Outcome::edit("Renumber wires", ops, RenumberwiresReply { renumbered: nets }))
    }
}

impl Generic for Checks {
    const TITLE: &'static str = "Checks";
    const PAGES: Pages = Pages::Any;
    const ACCESS: Access = Access::Reads;

    fn check(p: &ChecksParams, _registry: &ecad_model::registry::data::Registry) -> Result<(), CmdError> {
        for k in p.kinds.iter().flatten() {
            if crate::checks::Kind::parse(k).is_none() {
                return Err(CmdError::refused(format!("unknown check kind {k:?}")));
            }
        }
        Ok(())
    }

    fn run(p: ChecksParams, cx: &mut Cx<'_>) -> Result<Outcome<ChecksReply>, CmdError> {
        Self::check(&p, cx.registry)?;
        let kinds: Option<BTreeSet<_>> = p.kinds.map(|words| words.iter().filter_map(|k| crate::checks::Kind::parse(k)).collect());
        let limit = p.limit.unwrap_or(crate::checks::DEFAULT_LIMIT).clamp(1, crate::checks::MAX_LIMIT);
        let after = p.after.unwrap_or(0);
        // The project's kept report models (built on first use, then kept from each change).
        let reports = cx.derived.reports(cx.doc);
        let rows: Vec<_> = crate::checks::all_measured(cx.doc, cx.tolerance, reports, cx.look, cx.registry)
            .into_iter()
            .filter(|c| kinds.as_ref().is_none_or(|k| k.contains(&c.kind)))
            .collect();
        let checks = rows.iter().skip(after).take(limit).map(|c| c.row(cx.doc)).collect();
        let next = (rows.len() > after + limit).then_some(after + limit);
        Ok(Outcome::answer(ChecksReply { checks, total: rows.len(), next }))
    }
}
