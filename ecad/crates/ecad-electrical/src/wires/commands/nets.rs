//! The net, wire number, potential, category, wire settings, renumber and check commands (brief 022,
//! design 022 §10; brief 050b): params in, a result out. Generic ([`crate::commands::run`]); the
//! rules are in [`super::wires`].
//!
//! Excludes: `wire` and `joinwires`, which draw (the engine's session commands).

use std::collections::BTreeSet;

use ecad_model::connectivity::wiring;
use ecad_model::{ConnectionPointId, Op, PageId, WireId};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::wires::{self as rules, Conflict, NetChoice, Numbered, PotentialRow, WireSettingsChange, Wires};
use crate::cables::words::MAX_HANDLES;
use crate::commands::input::{Bounded, nullable};
use crate::commands::run::{Access, Cx, Generic, Outcome, Pages};
use crate::error::CmdError;

/// `nets {page?, number?, conductor?, connection_point?, all?, limit?, after?}` (read only): the
/// project's nets, a page at a time.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NetsParams {
    /// Only nets drawn on this page.
    #[serde(default)]
    page: Option<PageId>,
    /// Every net with this wire number.
    #[serde(default)]
    number: Option<String>,
    /// The net of this wire.
    #[serde(default)]
    conductor: Option<WireId>,
    /// The net of this connection point.
    #[serde(default)]
    connection_point: Option<ConnectionPointId>,
    /// Also nets without a wire.
    #[serde(default)]
    all: Option<bool>,
    /// At most this many (default 200, at most 2000).
    #[serde(default)]
    limit: Option<usize>,
    /// The cursor `next` gave.
    #[serde(default)]
    after: Option<String>,
}

/// `net {conductor | connection_point | number}` (read only): one net.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NetParams {
    /// The net of this wire.
    #[serde(default)]
    conductor: Option<WireId>,
    /// The net of this connection point.
    #[serde(default)]
    connection_point: Option<ConnectionPointId>,
    /// The (first) net with this wire number.
    #[serde(default)]
    number: Option<String>,
}

/// What to do when another net uses the number.
#[derive(Deserialize, JsonSchema, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum ConflictParam {
    /// That net takes this one's number.
    Swap,
    /// This net takes the next free number.
    Another,
    /// Join the nets with a continuation pair named for the number.
    Merge,
}

/// `wirenumber {conductor | connection_point | number, value: string | null, conflict?}` or
/// `{conductor, fixed: bool}`: fix the net's number (null, or fixed false, releases it).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WirenumberParams {
    /// A wire of the net.
    #[serde(default)]
    conductor: Option<WireId>,
    /// A connection point of the net.
    #[serde(default)]
    connection_point: Option<ConnectionPointId>,
    /// The net's current number.
    #[serde(default)]
    number: Option<String>,
    /// The number to fix, or null to release the fixed number.
    #[serde(default, deserialize_with = "nullable")]
    value: Option<Option<String>>,
    /// Without `value`: true fixes the net's current number, false releases it.
    #[serde(default)]
    fixed: Option<bool>,
    /// When another net uses the number.
    #[serde(default)]
    conflict: Option<ConflictParam>,
}

/// `potential {conductor, value: string | null}`: the potential of the whole net.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PotentialParams {
    /// A wire of the net.
    conductor: WireId,
    /// The potential (24V, L1…), or null to clear it.
    #[serde(deserialize_with = "nullable")]
    #[schemars(required, with = "crate::commands::input::OrNull<String>")]
    value: Option<Option<String>>,
}

/// `category {conductor | net | conductors, value: string | null}` (brief 050b, design 050
/// §6.9): a wire's own category (`net`: a wire whose whole net takes it).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CategoryParams {
    /// One wire.
    #[serde(default)]
    conductor: Option<WireId>,
    /// A wire whose whole net takes the category.
    #[serde(default)]
    net: Option<WireId>,
    /// These wires.
    #[serde(default)]
    conductors: Option<Bounded<WireId, 0, { MAX_HANDLES }>>,
    /// The wire category (its ID or name), or null to clear the wires' own.
    #[serde(deserialize_with = "nullable")]
    #[schemars(required, with = "crate::commands::input::OrNull<String>")]
    value: Option<Option<String>>,
}

/// One row of the potentials table.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PotentialRowParam {
    /// The potential's name (24V, L1, PE…).
    #[serde(default)]
    name: String,
    /// The prefix of its wire numbers.
    #[serde(default)]
    prefix: String,
    /// Whether its wires are numbered (default true).
    #[serde(default)]
    numbered: Option<bool>,
    /// Refused: colours belong to wire categories now.
    #[serde(default)]
    colour: Option<Value>,
    /// Refused: cross-sections belong to wire categories now.
    #[serde(default)]
    cross_section: Option<Value>,
}

/// `wiresettings {scheme?, digits?, unclassified_prefix?, potentials?, colour_style?,
/// label_height?, label_rule?, label_min_length?, number_by?}`: no parameters reads.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WiresettingsParams {
    /// How wires are numbered: "potential", "page" or "page-column".
    #[serde(default)]
    scheme: Option<String>,
    /// Digits of a wire number's sequence, 1 to 6.
    #[serde(default)]
    digits: Option<u64>,
    /// The prefix of wires of no potential.
    #[serde(default)]
    unclassified_prefix: Option<String>,
    /// The whole potentials table (colours and cross-sections are wire categories').
    #[serde(default)]
    potentials: Option<Vec<PotentialRowParam>>,
    /// "code2" (BN, BU, GN/YE), "code3" (BRN, BLU, GRN/YEL) or "name" (Brown, Blue, Green/Yellow).
    #[serde(default)]
    colour_style: Option<String>,
    /// Wire label height in mm, 0.5 to 10.
    #[serde(default)]
    label_height: Option<f64>,
    /// "net" (one label per net per page) or "each" (every wire).
    #[serde(default)]
    label_rule: Option<String>,
    /// The shortest labelled wire in mm, 0 to 100.
    #[serde(default)]
    label_min_length: Option<f64>,
    /// What one wire number counts: "net" (every wire of a net shows the net's number) or
    /// "conductor" (every wire its own). Switching renumbers every net in the same edit.
    #[serde(default)]
    number_by: Option<String>,
}

/// Which wires `renumberwires` renumbers.
#[derive(Deserialize, JsonSchema, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Scope {
    /// Only wires without a number (the default).
    Unnumbered,
    /// Every wire without a fixed number.
    All,
}

/// `renumberwires {scope?: "unnumbered" | "all"}` (fixed numbers never change).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RenumberwiresParams {
    /// Which wires (default "unnumbered").
    #[serde(default)]
    scope: Option<Scope>,
}

/// `checks {kinds?, limit?, after?}` (read only).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChecksParams {
    /// Only these kinds of check.
    #[serde(default)]
    kinds: Option<Vec<String>>,
    /// At most this many (default 200, at most 2000).
    #[serde(default)]
    limit: Option<usize>,
    /// Skip this many (the `next` a previous call gave).
    #[serde(default)]
    after: Option<usize>,
}

// ---------------------------------------------------------------- the commands

impl Generic for NetsParams {
    const ID: &'static str = "nets";
    const TITLE: &'static str = "List Nets";
    const HELP: &'static str =
        "{page?: id, number?, conductor?: id, connection_point?: id, all?: bool, limit?: n (200, max 2000), after?: cursor} (read only)";
    const PAGES: Pages = Pages::Any;
    const ACCESS: Access = Access::Reads;

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let q = rules::NetsQuery {
            page: self.page,
            choice: NetChoice { conductor: self.conductor, connection_point: self.connection_point, number: self.number },
            all: self.all.unwrap_or(false),
            limit: self.limit.unwrap_or(rules::DEFAULT_LIMIT).clamp(1, rules::MAX_LIMIT),
            after: self.after,
        };
        let found = rules::nets(cx.doc, &q)?;
        let mut r = json!({ "nets": found.nets.iter().map(rules::net_json).collect::<Vec<_>>() });
        if let Some(n) = found.next {
            r["next"] = json!(n);
        }
        Ok(Outcome::answer(r))
    }
}

impl Generic for NetParams {
    const ID: &'static str = "net";
    const TITLE: &'static str = "Show Net";
    const HELP: &'static str = "{conductor: id} | {connection_point: id} | {number} (read only)";
    const PAGES: Pages = Pages::Any;
    const ACCESS: Access = Access::Reads;

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let choice = NetChoice { conductor: self.conductor, connection_point: self.connection_point, number: self.number };
        Ok(Outcome::answer(rules::net_json(&rules::net(cx.doc, &choice)?)))
    }
}

impl Generic for WirenumberParams {
    const ID: &'static str = "wirenumber";
    const TITLE: &'static str = "Fix Wire Number";
    const HELP: &'static str = "{conductor: id | number, value: string|null, conflict?: \"swap\"|\"another\"|\"merge\"} | {conductor: id, fixed: bool} (fixes the net's number; null or fixed false releases it; a number another net uses needs `conflict`)";

    fn check(&self) -> Result<(), CmdError> {
        if self.value.is_none() && self.fixed.is_none() {
            return Err(CmdError::refused("`value` (text, or null) is required"));
        }
        if let Some(Some(v)) = &self.value {
            wiring::valid_number(v).map_err(|e| CmdError::refused(e.to_string()))?;
        }
        Ok(())
    }

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        self.check()?;
        let choice = NetChoice { conductor: self.conductor, connection_point: self.connection_point, number: self.number };
        // `fixed` alone (the Properties panel's Fixed box): fix the net's current number, or
        // release it.
        let value = match (self.value, self.fixed) {
            (Some(v), _) => Some(v),
            (None, Some(false)) => Some(None),
            _ => None,
        };
        let value = match value {
            Some(Some(v)) => Some(Some(wiring::valid_number(&v).map_err(|e| CmdError::refused(e.to_string()))?)),
            other => other,
        };
        let conflict = self.conflict.map(|c| match c {
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
        let answer = match done {
            Numbered::Released { ops, number, conductors } if ops.is_empty() => {
                json!({"number": number, "fixed": false, "conductors": conductors, "changed": false})
            }
            Numbered::Released { number, conductors, .. } => json!({"number": number, "fixed": false, "conductors": conductors}),
            Numbered::Fixed { number, conductors, .. } | Numbered::Another { number, conductors, .. } => {
                json!({"number": number, "fixed": true, "conductors": conductors})
            }
            Numbered::Swapped { number, conductors, swapped, .. } => {
                json!({"number": number, "fixed": true, "conductors": conductors, "swapped": swapped})
            }
            Numbered::Merged { number, continuations, .. } => json!({
                "number": number, "fixed": true, "merged": true,
                "continuations": continuations.iter().map(|c| c.hex()).collect::<Vec<_>>(),
            }),
        };
        Ok(Outcome::edit_if_any(label, ops, answer))
    }
}

impl Generic for PotentialParams {
    const ID: &'static str = "potential";
    const TITLE: &'static str = "Set Potential";
    const HELP: &'static str = "{conductor: id, value: string|null} (the potential of the whole net)";

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let value = match self.value.flatten() {
            Some(v) => wiring::valid_text("potential", Some(&v)).map_err(CmdError::refused)?,
            None => None,
        };
        let c = self.conductor;
        let (ops, n) = rules::potential(cx.doc, c, value.as_deref())?;
        let number = rules::number_of(cx.doc, c);
        Ok(Outcome::edit_if_any("Set potential", ops, json!({"potential": value, "conductors": n, "number": number})))
    }
}

impl Generic for CategoryParams {
    const ID: &'static str = "category";
    const TITLE: &'static str = "Wire Category";
    const HELP: &'static str =
        "{conductor: id | net: id (a conductor of the net) | conductors: [id], value: category id or name | null} (one transaction)";

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let value = self.value.flatten();
        let which = match (self.conductor, self.net, self.conductors.map(|b| b.0)) {
            (Some(c), None, None) => Wires::One(c),
            (None, Some(c), None) => Wires::Net(c),
            (None, None, Some(v)) => Wires::Many(v),
            _ => return Err(CmdError::input("Give one of `conductor`, `net` or `conductors`.")),
        };
        let done = rules::category(cx.doc, which, value.as_deref())?;
        let changed = !done.ops.is_empty();
        let answer = json!({"changed": changed, "conductors": done.wires, "category": done.category.map(|i| i.text()), "message": done.message});
        Ok(Outcome::edit_if_any(done.label, done.ops, answer))
    }
}

impl Generic for WiresettingsParams {
    const ID: &'static str = "wiresettings";
    const TITLE: &'static str = "Wire Settings";
    const HELP: &'static str = "{scheme?: \"potential\"|\"page\"|\"page-column\", digits?: 1..6, unclassified_prefix?, potentials?: [{name, prefix, numbered}] (colours and cross-sections: wire categories), colour_style?: \"code2\"|\"code3\"|\"name\", label_height?: mm} (no parameters reads; colours are IEC 60757 codes by default, free text allowed)";

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let rows = self.potentials.map(|rows| {
            rows.into_iter()
                .map(|r| PotentialRow {
                    name: r.name,
                    prefix: r.prefix,
                    numbered: r.numbered,
                    colours: r.colour.is_some() || r.cross_section.is_some(),
                })
                .collect()
        });
        let change = WireSettingsChange {
            scheme: self.scheme,
            digits: self.digits,
            colour_style: self.colour_style,
            label_height: self.label_height,
            label_rule: self.label_rule,
            label_min_length: self.label_min_length,
            number_by: self.number_by,
            unclassified_prefix: self.unclassified_prefix,
            potentials: rows,
        };
        if change == WireSettingsChange::default() {
            return Ok(Outcome::answer(rules::settings_json(cx.doc)));
        }
        let before = cx.doc.settings.clone();
        let after = rules::wire_settings(&before, &change)?;
        let ops = if after != before { vec![Op::SetProjectSettings { before, after }] } else { Vec::new() };
        Ok(Outcome::edit_then("Wire settings", ops, rules::settings_json))
    }
}

impl Generic for RenumberwiresParams {
    const ID: &'static str = "renumberwires";
    const TITLE: &'static str = "Renumber Wires";
    const HELP: &'static str = "{scope?: \"unnumbered\"|\"all\"} (fixed numbers never change)";

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let (ops, nets) = rules::renumber(cx.doc, self.scope == Some(Scope::All));
        if ops.is_empty() {
            return Ok(Outcome::answer(json!({"renumbered": 0})));
        }
        Ok(Outcome::edit("Renumber wires", ops, json!({"renumbered": nets})))
    }
}

impl Generic for ChecksParams {
    const ID: &'static str = "checks";
    const TITLE: &'static str = "Checks";
    const HELP: &'static str = "{kinds?: [kind], limit?: n (200, max 2000), after?: n} (read only)";
    const PAGES: Pages = Pages::Any;
    const ACCESS: Access = Access::Reads;

    fn check(&self) -> Result<(), CmdError> {
        for k in self.kinds.iter().flatten() {
            if crate::checks::Kind::parse(k).is_none() {
                return Err(CmdError::refused(format!("unknown check kind {k:?}")));
            }
        }
        Ok(())
    }

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        self.check()?;
        let kinds: Option<BTreeSet<_>> = self.kinds.map(|words| words.iter().filter_map(|k| crate::checks::Kind::parse(k)).collect());
        let limit = self.limit.unwrap_or(crate::checks::DEFAULT_LIMIT).clamp(1, crate::checks::MAX_LIMIT);
        let after = self.after.unwrap_or(0);
        // The project's kept report models (built on first use, then kept from each change).
        let reports = cx.derived.reports(cx.doc);
        let rows: Vec<_> = crate::checks::all_measured(cx.doc, cx.tolerance, reports, cx.metrics)
            .into_iter()
            .filter(|c| kinds.as_ref().is_none_or(|k| k.contains(&c.kind)))
            .collect();
        let shown: Vec<Value> = rows.iter().skip(after).take(limit).map(|c| c.to_json(cx.doc)).collect();
        let mut r = json!({"checks": shown, "total": rows.len()});
        if rows.len() > after + limit {
            r["next"] = json!(after + limit);
        }
        Ok(Outcome::answer(r))
    }
}
