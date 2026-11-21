//! Brief 010's cable reference commands (design 010 §7): `cableref`, `cablecontinue` and
//! `cablegoto` (with interactive forms in [`crate::interact`]), and `cablerefs` (read only).
//!
//! Each command is a thin shim (brief 070): its params struct reads the JSON, the rules in
//! [`ecad_electrical::cmd::reference`] decide, and the shim commits the ops and answers in JSON.
//! `cablegoto` opens the other end's page and moves the view here.

use cadcraft_engine::Session;
use ecad_electrical::CmdError;
use ecad_electrical::cmd::reference as rules;
use ecad_model::{CableId, End, RepresentationId, WireId};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::cmd::cable_draw::on_sheet;
use crate::cmd::input::{Command, Point, Typed};
use crate::cmd::{err, on_any_page, on_project, plain_style};

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] = &[
    Typed::of::<CablerefParams>("cableref"),
    Typed::of::<CablecontinueParams>("cablecontinue"),
    Typed::of::<CablegotoParams>("cablegoto"),
    Typed::of::<CablerefsParams>("cablerefs"),
];

/// The result fields every reference command reports about one end, derived after the commit.
fn reference_fields(doc: &ecad_model::Document, reference: RepresentationId) -> (Value, bool, Value) {
    match crate::references::all(doc).into_iter().find(|r| r.reference == reference) {
        Some(r) => (Value::String(r.text.clone()), r.orphan, r.partner().map_or(Value::Null, |p| Value::String(p.hex()))),
        None => (Value::String("/?".into()), true, Value::Null),
    }
}

/// `cableref {cable, at}` (design 010 §2.1, §7.1): a reference end of `cable` on the active page,
/// at `at`. One transaction: the Point item and its `reference` representation. Refused (R1 to
/// R3) when the cable already has a reference here, already has two, or has no wire drawn here.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CablerefParams {
    /// The cable.
    cable: CableId,
    /// Where the reference end goes.
    at: Point,
}

impl Command for CablerefParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let at = on_sheet(self.at, "at").map_err(|e| err(e.to_string()))?;
        on_project(s, |p, pg, page| {
            let style = plain_style(p);
            let done = rules::reference(p.ledger.document(), &mut p.ids, page, style, self.cable, at)?;
            p.apply_edit(pg, "cableref", "Cable reference", done.ops)?;
            let (text, orphan, partner) = reference_fields(p.ledger.document(), done.rep);
            Ok(json!({
                "reference": done.rep.hex(),
                "item": done.item.hex(),
                "page": page.hex(),
                "text": text,
                "orphan": orphan,
                "partner": partner,
            }))
        })
    }
}

/// `cablecontinue {cable, at, toward, members?}` (design 010 §2.2, §5, §7.2): the second drawing of
/// `cable` on the active page. One transaction: a reference end at `at`; for each continued core
/// (one on a loose line of the source page) a named point on each reference, the source line
/// landed on the one there and a stub from the one here to a free end (design 072 §4.4, F4);
/// then a slash across the stubs. Once a stub lands on a device point, one wire runs from device
/// point to device point across the pages.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CablecontinueParams {
    /// The cable.
    cable: CableId,
    /// Where the reference end goes.
    at: Point,
    /// The direction the stubs leave in.
    toward: Point,
    /// The cores that continue, in order, each by its wire or, with no wire yet, its line (default
    /// every one that can).
    #[serde(default)]
    members: Option<Vec<WireId>>,
}

impl Command for CablecontinueParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let at = on_sheet(self.at, "at").map_err(|e| err(e.to_string()))?;
        let toward = on_sheet(self.toward, "toward").map_err(|e| err(e.to_string()))?;
        on_project(s, |p, pg, page| {
            let style = plain_style(p);
            let tol = p.tolerance;
            let done = rules::continue_cable(p.ledger.document(), &mut p.ids, page, style, self.cable, self.members.as_deref(), at, toward, tol)?;
            p.apply_edit(pg, "cablecontinue", "Continue cable", done.ops)?;
            let doc = p.ledger.document();
            let rep = done.reference.rep;
            let (text, _, partner) = reference_fields(doc, rep);
            let routes: Vec<Value> = done
                .routes
                .iter()
                .map(|r| {
                    json!({
                        "core": r.core + 1,
                        "points": [r.points[0].hex(), r.points[1].hex()],
                        "route": r.route.hex(),
                        "item": r.item.hex(),
                        "free_end": if r.free_end == End::A { "A" } else { "B" },
                    })
                })
                .collect();
            let skipped: Vec<Value> = done.plan.skipped.iter().map(|(w, why)| json!({"conductor": w, "reason": why.reason()})).collect();
            Ok(json!({
                "reference": rep.hex(),
                "item": done.reference.item.hex(),
                "slash": done.slash_rep.hex(),
                "slash_item": done.slash_item.hex(),
                "routes": routes,
                "skipped": skipped,
                "text": text,
                "partner": partner,
                "messages": crate::references::continued_lines(doc, page, &done.plan, rep),
            }))
        })
    }
}

/// `cablegoto {reference}` (design 010 §4.5, §7.3): open (or front) the page of the other end of
/// `reference`, centre the view on it and select its point. Records nothing; works on a read-only
/// project.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CablegotoParams {
    /// A reference end.
    reference: RepresentationId,
}

impl Command for CablegotoParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let (uid, there, opened) = on_any_page(s, |p, pg| {
            let there = rules::other_end(p.ledger.document(), self.reference)?;
            let opened = p.pages.get(&there.page).and_then(|v| v.uid).is_none();
            let uid = p.open_page(pg, there.page).map_err(CmdError::refused)?;
            Ok((uid, there, opened))
        })?;
        if opened {
            let _ = s.zoom_extents();
        }
        if let Ok(st) = s.state_mut() {
            let height = st.view().height;
            let _ = st.set_view(cadcraft_engine::View { center: cadcraft_engine::geom::Vec2::new(there.at.x, there.at.y), height, twist: 0.0 });
        }
        let handle = on_any_page(s, |p, _| Ok(p.pages.get(&there.page).and_then(|v| v.map.item(there.item))))?;
        if let Some(h) = handle {
            s.set_selection(vec![h]);
        }
        Ok(json!({
            "page": there.page.hex(),
            "document": uid,
            "reference": there.reference.hex(),
            "item": there.item.hex(),
            "message": there.message,
        }))
    }
}

/// `cablerefs {cable?}` (design 010 §7.4): every cable reference (or `cable`'s), derived; read
/// only.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CablerefsParams {
    /// Only this cable's references.
    #[serde(default)]
    cable: Option<CableId>,
}

impl Command for CablerefsParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let cable = self.cable;
        on_any_page(s, |p, _| {
            let doc = p.ledger.document();
            if let Some(k) = cable
                && !doc.cables.contains_key(&k)
            {
                return Err(CmdError::refused("no such cable"));
            }
            let refs: Vec<_> = crate::references::all(doc).into_iter().filter(|r| cable.is_none_or(|k| r.cable == k)).collect();
            Ok(json!({ "references": ecad_inspect::references_json(doc, &refs) }))
        })
    }
}
