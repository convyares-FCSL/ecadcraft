//! The commands of brief 044 (design 044 §4, §5.3): `connector` (Place ▸ Connector) and `pinlabel`,
//! and `place` for a connector half or pin (Find Symbol).
//!
//! Every write is one transaction, committed through [`Project::apply_edit`]; nothing here opens a
//! dialog. The interactive form of `connector` ([`crate::interact::connector`]) commits through
//! this JSON form.
//!
//! `connector` and `pinlabel` are thin shims (brief 070): their params structs read the JSON, the
//! rules in [`ecad_electrical::cmd::connector`] decide, and the shim commits the ops and answers in
//! JSON. `place`'s connector form still reads its JSON by hand ([`run_place_half`]).
//!
//! [`Project::apply_edit`]: crate::project::Project::apply_edit

use std::collections::BTreeSet;

use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::cmd::connector::{self as rules, ConnectorInput, Made, NamesInput, Pinlabel, PinlabelInput};
use ecad_model::{ComponentId, ConductorId, Document, PageId, Placement, Pt, TerminalId};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::cmd::input::{Bounded, Command, Point, Typed};
use crate::cmd::{err, on_project, opt_typed, plain_style, with_project};
use crate::connectors::{self as cn, Names};
use crate::symbols::{Side, Starter};

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] = &[Typed::of::<ConnectorParams>("connector"), Typed::of::<PinlabelParams>("pinlabel")];

/// The commands this module adds (registered in `lanes.rs`).
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("connector", "Connector", crate::cmd::input::typed::<ConnectorParams>)
            .params(
                "{device?: id | tag (absent: a new connector), halves?: socket|plug|both, family?: plain|dsub|m12|rj45|usba|usbb, \
pins?: 1..64, names?: numbers|letters|[\"L\",\"N\",\"PE\"], layout?: row|pins, at?: [x,y], rotation?: 0|90|180|270, swap?: bool (row), \
line?: [[x,y],[x,y]] | wire?: id (pins), pin?: name (the first pin), page?: id} (design 044 §4.3; absent fields take the registry's \
connector.maker.* answers; one transaction)",
            )
            .interactive(crate::interact::connector)
            .noundo(),
        CommandSpec::new("pinlabel", "Rename Pin", crate::cmd::input::typed::<PinlabelParams>)
            .params(
                "{device?: id | tag, pin?: name, terminal?: id, to: new name | \"next\"} (renames a connector pin on both halves in one \
transaction; with terminal and to \"next\", moves that one drawn pin to its half's next free pin)",
            )
            .noundo(),
    ]
}

/// The connector a JSON call's `device` names (a hex ID or a tag): `place` still reads its JSON by
/// hand.
fn device_param(doc: &Document, params: &Value) -> Result<Option<ComponentId>, String> {
    match params.get("device") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(t)) => rules::device(doc, Some(t)).map_err(String::from),
        Some(_) => Err("`device` is a connector's id or tag".into()),
    }
}

fn out_of_range(p: Pt) -> bool {
    !(p.is_finite() && p.x.abs() <= crate::interact::MAX_COORD && p.y.abs() <= crate::interact::MAX_COORD)
}

// ---------------------------------------------------------------- connector (§4)

fn result_json(doc: &Document, m: &Made, message: &str) -> Value {
    json!({
        "component": m.component.hex(),
        "tag": m.tag,
        "new": m.new,
        "terminals": m.shown.iter().map(|x| json!({
            "terminal": x.terminal.hex(),
            "side": x.side.word(),
            "key": x.side.key(),
            "label": x.name,
            "pin": x.name,
            "name": cn::pin_name(doc, x.terminal).unwrap_or_else(|| format!("{}:{}", m.tag, x.name)),
        })).collect::<Vec<_>>(),
        "split": m.splits.iter().map(|(c, n)| json!({"conductor": c.hex(), "new": n.hex()})).collect::<Vec<_>>(),
        "dropped_wire_numbers": m.dropped.iter().map(|(c, n)| json!({"conductor": c.hex(), "number": n})).collect::<Vec<_>>(),
        "echo": message,
        "message": message,
    })
}

/// Pin names: numbers, letters, the names as text, or a list.
#[derive(Deserialize, JsonSchema)]
#[serde(untagged)]
enum NamesParam {
    /// The pin names in order.
    List(Vec<String>),
    /// `numbers`, `letters`, or the names as text (`L,N,PE`).
    Word(String),
}

/// `connector {…}` (design 044 §4.3): a row of whole halves, pins on the wires a line crosses, or a
/// bulkhead. Absent fields take the registry's `connector.maker.*` answers; one transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConnectorParams {
    /// The connector to join: an id or a tag (absent: a new connector).
    #[serde(default)]
    device: Option<String>,
    /// socket, plug or both.
    #[serde(default)]
    halves: Option<String>,
    /// plain, dsub, m12, rj45, usba or usbb.
    #[serde(default)]
    family: Option<String>,
    /// The pin count, 1 to 64.
    #[serde(default)]
    pins: Option<u64>,
    /// numbers, letters, or the pin names ["L", "N", "PE"].
    #[serde(default)]
    names: Option<NamesParam>,
    /// row (whole halves) or pins (one pin on each wire).
    #[serde(default)]
    layout: Option<String>,
    /// Where a row (or a bulkhead) goes.
    #[serde(default)]
    at: Option<Point>,
    /// 0, 90, 180 or 270.
    #[serde(default)]
    rotation: Option<f64>,
    /// A row's halves swapped.
    #[serde(default)]
    swap: Option<bool>,
    /// Pins: the line across the wires, [[x, y], [x, y]].
    #[serde(default)]
    line: Option<Bounded<Point, 2, 2>>,
    /// Pins: the one wire a pin lands on.
    #[serde(default)]
    wire: Option<ConductorId>,
    /// The first pin's name.
    #[serde(default)]
    pin: Option<String>,
    /// The page (absent: the active one).
    #[serde(default)]
    page: Option<PageId>,
}

impl Command for ConnectorParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let degrees = self.rotation.unwrap_or(0.0);
        if ![0.0, 90.0, 180.0, 270.0].contains(&degrees) {
            return Err(err("`rotation` is 0, 90, 180 or 270"));
        }
        let at = self.at.map(|p| p.0);
        let line = self.line.and_then(|l| match l.0.as_slice() {
            [a, b] => Some((a.0, b.0)),
            _ => None,
        });
        for p in at.iter().chain(line.iter().flat_map(|(a, b)| [a, b])) {
            if out_of_range(*p) {
                return Err(err(crate::interact::OUT_OF_RANGE));
            }
        }
        let names = self.names.map(|n| match n {
            NamesParam::List(v) => NamesInput::List(v),
            NamesParam::Word(w) => NamesInput::Word(w),
        });
        let input = ConnectorInput {
            page: self.page,
            device: self.device,
            halves: self.halves,
            family: self.family,
            pins: self.pins,
            names,
            layout: self.layout,
            at,
            line,
            wire: self.wire,
            degrees,
            swap: self.swap.unwrap_or(false),
            pin: self.pin.map(|p| p.trim().to_string()),
        };
        let v = on_project(s, |p, pg, active| {
            let style = plain_style(p);
            let tol = p.tolerance;
            let done = rules::plan_connector(&p.ledger, &mut p.ids, &mut p.tag_high, active, style, input, tol)?;
            // One transaction.
            p.apply_edit(pg, "connector", &done.label, done.made.ops.clone())?;
            p.last_placed = Some(("connector".to_string(), done.made.component));
            let doc = p.ledger.document();
            let message = rules::echo(doc, &done);
            Ok(result_json(doc, &done.made, &message))
        })?;
        if let Some(m) = v.get("message").and_then(Value::as_str) {
            s.echo(m.to_string());
        }
        Ok(v)
    }
}

// ---------------------------------------------------------------- place (Find Symbol)

/// `place {symbol: <connector half or pin>, at, rotation?, mirror?, device?, tag?, pin?}` (design
/// 044 §3.3): a new connector, or joining the connector named by `device` (or `tag`); a pin takes
/// the next pin (§4.4) or `pin`, and on a wire it lands there. One transaction.
pub(crate) fn run_place_half(
    s: &mut Session, params: &Value, starter: &'static Starter, at: Pt, degrees: f64, mirror: bool,
) -> cadcraft_engine::Result<Value> {
    let tag = opt_typed(params, "tag", "a tag", Value::as_str).map_err(err)?.map(str::to_string);
    let pin = opt_typed(params, "pin", "a pin name", Value::as_str).map_err(err)?.map(|p| p.trim().to_string());
    let side = starter.side().ok_or_else(|| err("That symbol is not a connector half."))?;
    let v = with_project(s, |p, pg, page| {
        let doc = p.ledger.document();
        let mut device = device_param(doc, params)?;
        if params.get("device").is_some_and(|v| !v.is_null()) && tag.is_some() {
            return Err("`tag` is for a new connector; `device` joins an existing one".into());
        }
        let mut new_tag = None;
        if let Some(t) = &tag
            && let Some(found) = crate::connectors::resolve(doc, t)
        {
            // Brief 044 (tester D5): a half's tag (`-X01.P`) names its connector and that half.
            let (c, half, _) = found?;
            if let Some(h) = half
                && side != Side::Both
                && h != side
            {
                return Err(format!(
                    "{} is the {} half of {}; this symbol is a {} half. Name -{}'s {} half, or Enter for a new connector.",
                    t.trim(),
                    h.word(),
                    crate::devices::tag_words(doc, c),
                    side.word(),
                    crate::devices::tag_words(doc, c).trim_start_matches('-'),
                    side.word()
                ));
            }
            device = Some(c);
        } else if let Some(t) = &tag {
            let t = ecad_model::tag::normalise(t, crate::defaults::print::tag_style(doc))?;
            match crate::devices::holder_of(doc, &t, None) {
                Some(c) => device = Some(c),
                None if !doc.cables_tagged(&t).is_empty() => return Err(format!("Tag {t} is a cable's; choose another tag for a device.")),
                None => new_tag = Some(t),
            }
        }
        if let Some(c) = device
            && !cn::is_connector(doc, c)
        {
            return Err(cn::not_a_connector(doc, c));
        }
        let style = plain_style(p);
        let mut ops = Vec::new();
        let def = crate::cmd::terminal::definition_for(doc, &mut p.ids, starter, &mut ops)?;
        let taken = BTreeSet::new();
        let pin_starter = cn::is_pin_starter(starter);
        let names: Vec<String> = match (device, pin_starter) {
            (Some(c), true) => vec![match &pin {
                Some(n) => {
                    cn::check_named_pin(doc, c, side, n, &taken)?;
                    n.clone()
                }
                None => cn::next_pin(doc, c, side, &taken)?,
            }],
            (None, true) => {
                let n = pin.clone().unwrap_or_else(|| "1".into());
                cn::check_name(&n)?;
                vec![n]
            }
            (Some(c), false) => {
                cn::check_whole_half(doc, c, side, &taken)?;
                rules::join_names(doc, c, cn::size_of(starter).unwrap_or(0))
            }
            (None, false) => cn::names_for(&Names::Numbers, cn::size_of(starter).unwrap_or(0))?,
        };
        let (c, tag_text, new) = match device {
            Some(c) => (c, crate::devices::tag_words(doc, c), false),
            None => {
                let pins = cn::size_of(starter);
                let (c, t) = cn::create_ops(&p.ledger, &mut p.ids, &mut p.tag_high, starter, def, new_tag, pins, None, &mut ops)?;
                (c, t, true)
            }
        };
        let landing = if pin_starter { crate::cmd::terminal::landing_point(doc, page, at) } else { None };
        let origin = landing.map_or(at, |on| on.at);
        let rotation = match landing {
            Some(on) if degrees == 0.0 => rules::along_wire(doc, page, &on),
            _ => degrees.to_radians(),
        };
        let placement = Placement { origin, rotation, mirror, scale: 1.0 };
        let (rep, _, shown) = cn::symbol_ops(&mut p.ids, page, style, def, starter, c, &names, placement, &mut ops)?;
        let land = match landing {
            Some(on) => Some(cn::land_pin(doc, &mut p.ids, page, on, &shown, placement)?),
            None => None,
        };
        if let Some(l) = &land {
            ops.extend(l.ops.clone());
        }
        let label = if new { format!("Place connector {tag_text}") } else { format!("Add symbol to {tag_text}") };
        // Brief 052d (design 052 §5.3): the place proposes; a half's conflict may answer Half.
        let proposed = crate::draw::proposals::plan_place(p, params, c, page, origin, starter.side())?;
        ops.extend(proposed.ops.iter().cloned());
        p.apply_edit(pg, "place", &label, ops)?;
        p.last_placed = Some((starter.key.to_string(), c));
        let doc = p.ledger.document();
        let made = Made {
            component: c,
            tag: tag_text.clone(),
            new,
            shown: shown.clone(),
            splits: land.iter().map(|l| (l.conductor, l.new)).collect(),
            dropped: land.iter().filter_map(|l| l.dropped_number.as_ref().map(|(n, _)| (l.new, n.clone()))).collect(),
            ops: Vec::new(),
        };
        let names: Vec<String> = shown.iter().filter_map(|x| cn::pin_name(doc, x.terminal)).collect();
        let message = format!(
            "Placed {}.",
            if names.len() > 2 {
                format!("{}…{}", names.first().cloned().unwrap_or_default(), names.last().cloned().unwrap_or_default())
            } else {
                names.join(" and ")
            }
        );
        let mut v = result_json(doc, &made, &message);
        v["symbol"] = json!(starter.key);
        v["joined"] = json!(!new);
        v["representation"] = json!(rep.hex());
        v["definition"] = json!(def.hex());
        crate::draw::proposals::finish_place(p, pg, c, !new, &proposed, &mut v);
        Ok(v)
    })?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(v)
}

// ---------------------------------------------------------------- pinlabel (§5.3)

/// `pinlabel {device | terminal, pin?, to}` (design 044 §5.3): renames a connector pin on both
/// halves, or with `terminal` and `to: "next"` moves that one drawn pin to its half's next free pin.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PinlabelParams {
    /// The connector: an id or a tag.
    #[serde(default)]
    device: Option<String>,
    /// With `device`: the pin's current name.
    #[serde(default)]
    pin: Option<String>,
    /// One drawn pin.
    #[serde(default)]
    terminal: Option<TerminalId>,
    /// The new pin name, or "next".
    to: String,
}

impl Command for PinlabelParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let input = PinlabelInput { device: self.device, pin: self.pin, terminal: self.terminal, to: self.to };
        let v = on_project(s, |p, pg, _| {
            Ok(match rules::plan_pinlabel(p.ledger.document(), input)? {
                Pinlabel::Unchanged { component, to } => json!({"changed": false, "component": component.hex(), "to": to}),
                Pinlabel::Moved { component, from, to, ops, label, message } => {
                    p.apply_edit(pg, "pinlabel", &label, ops)?;
                    json!({"changed": true, "component": component.hex(), "from": from, "to": to, "message": message})
                }
                Pinlabel::Renamed { component, tag, from, to, ops, label } => {
                    p.apply_edit(pg, "pinlabel", &label, ops)?;
                    let message = rules::renamed_message(p.ledger.document(), component, &tag, &from, &to);
                    json!({"changed": true, "component": component.hex(), "from": from, "to": to, "message": message})
                }
            })
        })?;
        if let Some(m) = v.get("message").and_then(Value::as_str) {
            s.echo(m.to_string());
        }
        Ok(v)
    }
}
