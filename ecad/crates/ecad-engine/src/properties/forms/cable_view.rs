//! Cable View.

use super::{R, Safe, commit, not_a_point, other, paper_guard, safely};
use crate::cables::views::View;
use crate::commands::MAX_HANDLES;
use crate::interact::{can_start, in_range, pick, read};
use crate::project::Mapped;
use cadcraft_engine::{Accept, Input, Interactive, Prompt, Session, Step};
use ecad_electrical::cables::slash;
use ecad_electrical::commands::input::OUT_OF_RANGE;
use ecad_model::{CableId, Subject, WireId};
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// `cableview`: switch the selected cables at once, else click cables one by one; presentation
/// only, so Esc has nothing to drop.
pub(crate) fn cableview(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    Ok(Box::new(Safe(CableView)))
}

struct CableView;

pub(crate) const NOTHING_THERE: &str = "Nothing there. Click a cable slash, a fan or a wire in a cable.";
pub(crate) const WIRE_NOT_IN_CABLE: &str = "That wire is not in a cable.";
pub(crate) const NOT_A_CABLE: &str = "That is not a cable or a wire in a cable.";

/// The cable a mapped object names: a slash, a fan line, or a wire in a cable
/// (and, a cable reference's point or text).
/// The wire a clicked line stands for when picking cable members: the first wire it carries that
/// is in a cable, else the wire it stands for ([`ecad_electrical::wires::routes::wire_of_route`]).
pub(crate) fn cabled_wire(doc: &ecad_model::Document, r: ecad_model::RouteId) -> Option<WireId> {
    ecad_electrical::wires::routes::wires_of_route(doc, r)
        .into_iter()
        .find(|w| !doc.cables_of(*w).is_empty())
        .or_else(|| ecad_electrical::wires::routes::wire_of_route(doc, r))
}

pub(crate) fn cable_named(doc: &ecad_model::Document, m: Mapped) -> Result<CableId, &'static str> {
    match m {
        Mapped::Fan(k) => Ok(k),
        Mapped::RefText(r) => match doc.representations().get(&r).map(|r| r.subject) {
            Some(Subject::Cable(k)) => Ok(k),
            _ => Err(NOT_A_CABLE),
        },
        Mapped::Item(i) => match doc.representation_of_item(i).map(|(_, r)| r.subject) {
            Some(Subject::Cable(k)) => Ok(k),
            Some(Subject::Wire(w)) => doc.cables_of(w).first().copied().ok_or(WIRE_NOT_IN_CABLE),
            Some(Subject::Route(r)) => {
                let w = cabled_wire(doc, r).ok_or(WIRE_NOT_IN_CABLE)?;
                doc.cables_of(w).first().copied().ok_or(WIRE_NOT_IN_CABLE)
            }
            _ => Err(NOT_A_CABLE),
        },
        Mapped::Label(w) => doc.cables_of(w).first().copied().ok_or(WIRE_NOT_IN_CABLE),
        Mapped::Tee(r) => {
            let w = cabled_wire(doc, r).ok_or(WIRE_NOT_IN_CABLE)?;
            doc.cables_of(w).first().copied().ok_or(WIRE_NOT_IN_CABLE)
        }
        Mapped::Marker(..) | Mapped::Tag(_) | Mapped::Frame(_) | Mapped::DeviceReference(_) | Mapped::JumperBar(..) => Err(NOT_A_CABLE),
    }
}

impl CableView {
    /// Switch `cable` to `view` through the JSON form and say so (with why it was not remembered).
    fn switch(s: &mut Session, cable: CableId, view: Option<View>) -> R<String> {
        let mut params = json!({"cable": cable.hex()});
        if let Some(v) = view {
            params["view"] = Value::String(v.name().into());
        }
        let r = commit(s, "cableview", &params)?;
        let shown = r.get("view").and_then(Value::as_str).and_then(View::parse).unwrap_or(View::Traditional);
        let mut line = read(s, |p, _| Ok(ecad_electrical::cables::layout::switched(p.ledger.document(), cable, shown.name())))
            .unwrap_or_else(|_| format!("Cable {}… now shown in {} view.", short_hex(cable), shown.name()));
        if let Some(reason) = r.get("reason").and_then(Value::as_str) {
            line.pop();
            line.push_str(&format!(" (not remembered: {reason})."));
        }
        if let Some(notice) = r.get("notice").and_then(Value::as_str) {
            s.echo(notice);
        }
        Ok(line)
    }

    /// Step 0: the cables the selection names, switched together (all fan → traditional,
    /// otherwise all → fan). `None` when the selection names no cable.
    fn selection(s: &mut Session) -> R<Option<Step>> {
        let sel = s.selection();
        let found = read(s, |p, page| {
            let Some(map) = p.pages.get(&page).map(|v| &v.map) else { return Ok(Vec::new()) };
            let doc = p.ledger.document();
            let mut seen = BTreeSet::new();
            let mut out = Vec::new();
            for h in sel.iter().take(MAX_HANDLES) {
                if let Some(k) = map.get(*h).and_then(|m| cable_named(doc, m).ok())
                    && seen.insert(k)
                {
                    let one_wire = !ecad_electrical::cables::layout::offers_fan(doc, k);
                    out.push((k, p.view_of(k), one_wire, ecad_electrical::cables::cable_title(doc, k)));
                }
            }
            Ok(out)
        })
        .map_err(other)?;
        if found.is_empty() {
            return Ok(None);
        }
        let target = if found.iter().all(|(_, v, _, _)| *v == View::Fan) { View::Traditional } else { View::Fan };
        // Each cable's own result; one that fails never stops the others.
        let (mut switched, mut other_lines) = (Vec::new(), Vec::new());
        for (k, current, one_wire, title) in found {
            if target == View::Fan && current != View::Fan && one_wire {
                other_lines.push(format!("Cable {title} {}", crate::cables::views::ONE_WIRE));
                continue;
            }
            match Self::switch(s, k, Some(target)) {
                Ok(line) => switched.push(line),
                Err(e) => other_lines.push(format!("Cable {title} was not switched: {e}")),
            }
        }
        let shown = switched.len().min(slash::MAX_NAMES);
        for l in switched.iter().take(shown) {
            s.echo(l.clone());
        }
        if switched.len() > shown {
            s.echo(format!("and {} more cables switched.", switched.len() - shown));
        }
        let shown = other_lines.len().min(slash::MAX_NAMES);
        for l in other_lines.iter().take(shown) {
            s.echo(l.clone());
        }
        if other_lines.len() > shown {
            s.echo(format!("and {} more cables not switched.", other_lines.len() - shown));
        }
        // The selected objects may no longer be drawn.
        s.set_selection(Vec::new());
        Ok(Some(Step::Done))
    }

    fn step(&mut self, s: &mut Session, i: Input) -> R<Step> {
        match i {
            Input::Point(p) => {
                if !in_range(p) {
                    return Err(other(OUT_OF_RANGE));
                }
                let h = pick(s, p).ok_or_else(|| other(NOTHING_THERE))?;
                let (k, current, one_wire, title) = read(s, |proj, page| {
                    let m = proj.pages.get(&page).and_then(|v| v.map.get(h)).ok_or(NOT_A_CABLE)?;
                    let doc = proj.ledger.document();
                    let k = cable_named(doc, m)?;
                    Ok((k, proj.view_of(k), !ecad_electrical::cables::layout::offers_fan(doc, k), ecad_electrical::cables::cable_title(doc, k)))
                })
                .map_err(other)?;
                if current == View::Traditional && one_wire {
                    return Err(other(format!("Cable {title} {}", crate::cables::views::ONE_WIRE)));
                }
                let line = Self::switch(s, k, None)?;
                s.echo(line);
                Ok(Step::Continue)
            }
            Input::Enter => Ok(Step::Done),
            Input::Text(t) => Err(not_a_point(&t)),
            _ => Ok(Step::Continue),
        }
    }
}

fn short_hex(k: CableId) -> String {
    k.hex().get(..8).unwrap_or_default().to_string()
}

impl Interactive for CableView {
    fn name(&self) -> &'static str {
        "CABLEVIEW"
    }
    /// With cable objects selected they switch at once and the command ends.
    fn begin(&mut self, s: &mut Session) -> R<Step> {
        safely(s, "cableview", |s| Ok(Self::selection(s)?.unwrap_or(Step::Continue)))
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        Prompt::new("Select a cable to switch its view", Accept::POINT)
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
        paper_guard(s, &i)?;
        safely(s, "cableview", |s| self.step(s, i))
    }
}
