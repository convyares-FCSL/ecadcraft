//! The interactive forms of the cable reference commands (brief 010, design 010 §4): Cable
//! Reference, Continue Cable and Go to Reference. Like every ECAD form they only collect and
//! resolve values; each commit runs the command's JSON form (design 008 §2 rule 3), so the
//! interactive and JSON forms record identical transactions.

use std::collections::BTreeSet;

use cadcraft_engine::doc::{Entity, EntityKind, Handle};
use cadcraft_engine::geom::Vec2;
use cadcraft_engine::{Accept, Input, Interactive, Prompt, Session, Step};
use ecad_model::derive::CableReference;
use ecad_model::{CableId, Document, Op, PageId, Pt, RepresentationId};
use serde_json::{Value, json};

use super::forms::{NOTHING_THERE, R, Safe, cable_named, commit, not_a_point, other, paper_guard, safely};
use super::{
    OUT_OF_RANGE, can_start, in_range, line_kind, pick, pixel, preview_entity, preview_text, pt, read, ring_kind, v2,
};
use crate::project::{Mapped, Project};
use crate::references::{self as refs, ORPHAN_ACI, REFERENCE_ACI};

/// The ring of a reference in a preview, in screen pixels (`ecad.ref-size`).
const RING: f64 = refs::RING_PX as f64;
/// `ecad.attach-preview` (ACI 3) for the preview text that says what will happen.
const PREVIEW_ACI: u8 = super::ATTACHED_ACI;

/// The cables the current selection names (slashes, fan lines, member wires, reference ends).
fn selected_cables(s: &Session) -> Vec<CableId> {
    let sel = s.selection();
    read(s, |p, page| {
        let Some(map) = p.pages.get(&page).map(|v| &v.map) else { return Ok(Vec::new()) };
        let doc = p.ledger.document();
        let mut seen = BTreeSet::new();
        Ok(sel
            .iter()
            .take(crate::commands::MAX_HANDLES)
            .filter_map(|h| map.get(*h).and_then(|m| cable_named(doc, m).ok()))
            .filter(|k| seen.insert(*k))
            .collect())
    })
    .unwrap_or_default()
}

/// The reference a mapped object is part of: its point item or its derived text.
fn reference_named(p: &Project, m: Mapped) -> Option<RepresentationId> {
    let doc = p.ledger.document();
    match m {
        Mapped::RefText(r) => Some(r),
        Mapped::Item(i) => doc.representation_of_item(i).filter(|(_, r)| refs::is_reference(r)).map(|(id, _)| id),
        _ => None,
    }
}

/// The ring and text a reference preview shows at `at`.
fn ring_and_text(s: &Session, at: Pt, text: &str, orphan: bool) -> Vec<Entity> {
    let Some(px) = pixel(s) else { return Vec::new() };
    let aci = if orphan { ORPHAN_ACI } else { REFERENCE_ACI };
    let mut out: Vec<Entity> =
        [RING - 1.0, RING, RING + 1.0].into_iter().map(|d| preview_entity(s, ring_kind(at, d, px), aci)).collect();
    out.push(preview_text(s, Pt::new(at.x + RING * px, at.y - 4.0 * px), px, text.to_string(), aci));
    out
}

/// The text the cable's next reference would show: its existing reference's place, or `/?`.
fn next_text(doc: &Document, cable: CableId) -> (String, bool) {
    match refs::all(doc).into_iter().find(|r| r.cable == cable) {
        Some(r) => (r.locator(), false),
        None => ("/?".into(), true),
    }
}

// ---------------------------------------------------------------- Cable Reference (§4.1)

/// `cableref`: pick a cable (or take the selected one), then click where it leaves this page.
pub(crate) fn cableref(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    Ok(Box::new(Safe(CableRef::default())))
}

#[derive(Default)]
struct CableRef {
    cable: Option<CableId>,
    /// Step 1 was shown (so `U` goes back to it).
    asked: bool,
}

impl CableRef {
    fn choose(s: &Session, k: CableId) -> Result<(), String> {
        read(s, |p, page| refs::check_reference(p.ledger.document(), page, k))
    }

    fn place(&mut self, s: &mut Session, k: CableId, at: Pt) -> R<Step> {
        let r = commit(s, "cableref", &json!({"cable": k.hex(), "at": [at.x, at.y]}))?;
        let orphan = r.get("orphan").and_then(Value::as_bool).unwrap_or(true);
        let line = read(s, |p, page| {
            let doc = p.ledger.document();
            Ok(format!(
                "Reference for cable {} placed on {}.",
                crate::views::cable_title(doc, k),
                refs::page_number_words(doc, page)
            ))
        })
        .unwrap_or_else(|_| "Reference placed.".into());
        s.echo(line);
        if orphan {
            s.echo("It has no other end yet: open the other page and click Continue Cable.");
        }
        Ok(Step::Done)
    }

    fn step(&mut self, s: &mut Session, i: Input) -> R<Step> {
        match (i, self.cable) {
            (Input::Point(p), _) if !in_range(p) => Err(other(OUT_OF_RANGE)),
            (Input::Point(p), None) => {
                let h = pick(s, p).ok_or_else(|| other(NOTHING_THERE))?;
                let k = read(s, |proj, page| {
                    let m = proj.pages.get(&page).and_then(|v| v.map.get(h)).ok_or(super::forms::NOT_A_CABLE)?;
                    cable_named(proj.ledger.document(), m).map_err(str::to_string)
                })
                .map_err(other)?;
                Self::choose(s, k).map_err(other)?;
                self.cable = Some(k);
                Ok(Step::Continue)
            }
            (Input::Point(p), Some(k)) => self.place(s, k, pt(p)),
            (Input::Keyword(k), Some(_)) if k == "Undo" && self.asked => {
                self.cable = None;
                Ok(Step::Continue)
            }
            (Input::Enter, _) => Ok(Step::Done),
            (Input::Text(t), _) => Err(not_a_point(&t)),
            _ => Ok(Step::Continue),
        }
    }
}

impl Interactive for CableRef {
    fn name(&self) -> &'static str {
        "CABLEREF"
    }
    /// Step 0: one selected cable goes straight to the reference point.
    fn begin(&mut self, s: &mut Session) -> R<Step> {
        safely(s, "cableref", |s| {
            self.asked = true;
            match selected_cables(s).as_slice() {
                [] => {}
                [k] => match Self::choose(s, *k) {
                    Ok(()) => {
                        self.cable = Some(*k);
                        self.asked = false;
                    }
                    Err(e) => s.echo(e),
                },
                _ => s.echo("Select one cable only."),
            }
            Ok(Step::Continue)
        })
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match self.cable {
            None => Prompt::new("Select a cable to reference", Accept::POINT),
            Some(_) if self.asked => Prompt::new("Specify reference point", Accept::POINT).kw(&["Undo"]),
            Some(_) => Prompt::new("Specify reference point", Accept::POINT),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
        paper_guard(s, &i)?;
        safely(s, "cableref", |s| self.step(s, i))
    }
    /// Step 2: the ring at the cursor and the text the reference will show.
    fn preview_objects(&self, s: &Session, cursor: Vec2) -> Vec<Entity> {
        let Some(k) = self.cable else { return Vec::new() };
        if !in_range(cursor) {
            return Vec::new();
        }
        let Ok((text, orphan)) = read(s, |p, _| Ok(next_text(p.ledger.document(), k))) else { return Vec::new() };
        ring_and_text(s, pt(cursor), &text, orphan)
    }
}

// ---------------------------------------------------------------- Continue Cable (§4.2)

/// `cablecontinue`: name the cable (Enter takes the one waiting most recently), click where it
/// arrives, then which way its wires run.
pub(crate) fn cablecontinue(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    let default = read(s, |p, _| Ok(latest_orphan(p))).ok().flatten();
    Ok(Box::new(Safe(Continue { default, cable: None, at: None })))
}

struct Continue {
    /// The cable of the orphan reference made most recently, with its name.
    default: Option<(CableId, String)>,
    cable: Option<CableId>,
    at: Option<Pt>,
}

/// The cable of the orphan reference linked most recently (design 010 §4.2 step 1, Q12): the
/// latest transaction in the history that linked a reference which is an orphan now.
fn latest_orphan(p: &Project) -> Option<(CableId, String)> {
    let doc = p.ledger.document();
    let orphans: Vec<CableReference> = refs::all(doc).into_iter().filter(|r| r.orphan).collect();
    if orphans.is_empty() {
        return None;
    }
    let ids: BTreeSet<RepresentationId> = orphans.iter().map(|r| r.reference).collect();
    let latest = p.ledger.log().iter().rev().find_map(|tx| {
        tx.ops.iter().rev().find_map(|op| match op {
            Op::LinkRepresentation { id, .. } if ids.contains(id) => Some(*id),
            _ => None,
        })
    });
    let r = match latest {
        Some(id) => orphans.iter().find(|r| r.reference == id),
        None => orphans.last(),
    }?;
    Some((r.cable, crate::views::cable_title(doc, r.cable)))
}

/// Most orphan references `List` shows.
const MAX_LISTED: usize = 20;

/// The cable typed at step 1 (design 010 §4.2 1b): a tag (case-insensitive, exact), else a full ID
/// or a leading hex prefix of at least 8 digits naming one cable.
fn cable_typed(doc: &Document, typed: &str) -> Result<CableId, String> {
    let t = typed.trim();
    let tagged: Vec<CableId> = doc
        .cables
        .iter()
        .filter(|(_, c)| c.properties.get("tag").is_some_and(|x| x.eq_ignore_ascii_case(t)))
        .map(|(k, _)| *k)
        .collect();
    let found = if tagged.is_empty() && t.len() >= 8 && t.chars().all(|c| c.is_ascii_hexdigit()) {
        let lower = t.to_ascii_lowercase();
        doc.cables.keys().copied().filter(|k| k.hex().starts_with(&lower)).collect()
    } else {
        tagged
    };
    match found.as_slice() {
        [] => Err(format!("No cable named \"{t}\".")),
        [k] => Ok(*k),
        many => Err(format!("\"{t}\" names {} cables; type more of it.", many.len())),
    }
}

/// The checks of step 1 (R2, R4 to R6) for `cable` on `page`, without any geometry.
fn check_continue(doc: &Document, page: PageId, cable: CableId) -> Result<(), String> {
    refs::plan_continue(doc, page, cable, None, Pt::new(0.0, 0.0), Pt::new(1.0, 0.0), 0.0).map(|_| ())
}

impl Continue {
    fn list(s: &mut Session) {
        let lines = read(s, |p, _| {
            let doc = p.ledger.document();
            let orphans: Vec<CableReference> = refs::all(doc).into_iter().filter(|r| r.orphan).collect();
            let mut out: Vec<String> = orphans
                .iter()
                .take(MAX_LISTED)
                .map(|r| {
                    format!(
                        "{} on {} ({}): {}",
                        crate::views::cable_title(doc, r.cable),
                        refs::page_number_words(doc, r.page),
                        r.locator(),
                        crate::slash::wires(r.wires_here)
                    )
                })
                .collect();
            if orphans.len() > MAX_LISTED {
                out.push(format!("and {} more.", orphans.len() - MAX_LISTED));
            }
            if orphans.is_empty() {
                out.push("None waiting.".into());
            }
            Ok(out)
        })
        .unwrap_or_default();
        for l in lines {
            s.echo(l);
        }
    }

    fn pick_cable(&mut self, s: &mut Session, k: CableId) -> R<Step> {
        read(s, |p, page| check_continue(p.ledger.document(), page, k)).map_err(other)?;
        self.cable = Some(k);
        Ok(Step::Continue)
    }

    fn finish(&mut self, s: &mut Session, k: CableId, at: Pt, toward: Pt) -> R<Step> {
        let r =
            commit(s, "cablecontinue", &json!({"cable": k.hex(), "at": [at.x, at.y], "toward": [toward.x, toward.y]}))?;
        for l in r.get("messages").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
            s.echo(l.to_string());
        }
        Ok(Step::Done)
    }

    fn tolerance(s: &Session) -> f64 {
        read(s, |p, _| Ok(p.tolerance)).unwrap_or(1e-6)
    }

    fn step(&mut self, s: &mut Session, i: Input) -> R<Step> {
        match (self.cable, self.at) {
            (None, _) => {
                match i {
                    Input::Enter => match self.default {
                        Some((k, _)) => self.pick_cable(s, k),
                        None => {
                            s.echo("No cable reference is waiting for its other end; place one with Cable Reference first.");
                            Ok(Step::Done)
                        }
                    },
                    Input::Keyword(k) if k == "List" => {
                        Self::list(s);
                        Ok(Step::Continue)
                    }
                    Input::Text(t) | Input::Keyword(t) => {
                        if matches!(t.trim().to_ascii_lowercase().as_str(), "l" | "list") {
                            Self::list(s);
                            return Ok(Step::Continue);
                        }
                        let k = read(s, |p, _| cable_typed(p.ledger.document(), &t)).map_err(other)?;
                        self.pick_cable(s, k)
                    }
                    _ => Ok(Step::Continue),
                }
            }
            (Some(_), None) => match i {
                Input::Point(p) if !in_range(p) => Err(other(OUT_OF_RANGE)),
                Input::Point(p) => {
                    self.at = Some(pt(p));
                    Ok(Step::Continue)
                }
                Input::Enter => Ok(Step::Done),
                Input::Text(t) => Err(not_a_point(&t)),
                _ => Ok(Step::Continue),
            },
            (Some(k), Some(at)) => match i {
                Input::Point(p) if !in_range(p) => Err(other(OUT_OF_RANGE)),
                Input::Point(p) if pt(p).near(at, Self::tolerance(s)) => Ok(Step::Continue),
                Input::Point(p) => self.finish(s, k, at, pt(p)),
                Input::Keyword(w) if w == "Undo" => {
                    self.at = None;
                    Ok(Step::Continue)
                }
                Input::Enter => Ok(Step::Done),
                Input::Text(t) => Err(not_a_point(&t)),
                _ => Ok(Step::Continue),
            },
        }
    }

    /// The plan a click at `toward` would commit (the JSON form's own planning).
    fn plan(s: &Session, k: CableId, at: Pt, toward: Pt) -> Option<refs::Plan> {
        read(s, |p, page| Ok(refs::plan_continue(p.ledger.document(), page, k, None, at, toward, p.tolerance).ok()))
            .ok()
            .flatten()
    }
}

impl Interactive for Continue {
    fn name(&self) -> &'static str {
        "CABLECONTINUE"
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match (self.cable, self.at) {
            (None, _) => {
                let p = Prompt::new("Enter cable to continue", Accept::TEXT).kw(&["List"]);
                match &self.default {
                    Some((_, name)) => p.default(name.clone()),
                    None => p,
                }
            }
            (Some(_), None) => Prompt::new("Specify reference point", Accept::POINT),
            (Some(_), Some(at)) => {
                Prompt::new("Specify direction and length of the wires", Accept::POINT).kw(&["Undo"]).base(v2(at))
            }
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
        paper_guard(s, &i)?;
        safely(s, "cablecontinue", |s| self.step(s, i))
    }
    /// Step 3: the stubs and the slash as they will be drawn.
    fn preview(&self, s: &Session, cursor: Vec2) -> Vec<EntityKind> {
        let (Some(k), Some(at)) = (self.cable, self.at) else { return Vec::new() };
        if !super::model_space(s) || !in_range(cursor) {
            return Vec::new();
        }
        let Some(plan) = Self::plan(s, k, at, pt(cursor)) else { return vec![line_kind(at, pt(cursor))] };
        let mut out: Vec<EntityKind> = plan.stubs.iter().take(500).map(|st| line_kind(st.a, st.b)).collect();
        out.push(line_kind(plan.slash.0, plan.slash.1));
        out
    }
    /// Step 2: the ring and the live text this end will show; step 3: the ring at the point and
    /// how many wires continue, beside the cursor.
    fn preview_objects(&self, s: &Session, cursor: Vec2) -> Vec<Entity> {
        let Some(k) = self.cable else { return Vec::new() };
        if !in_range(cursor) {
            return Vec::new();
        }
        let Ok((text, orphan)) = read(s, |p, _| Ok(next_text(p.ledger.document(), k))) else { return Vec::new() };
        match self.at {
            None => ring_and_text(s, pt(cursor), &text, orphan),
            Some(at) => {
                let mut out = ring_and_text(s, at, &text, orphan);
                let Some(px) = pixel(s) else { return out };
                let words = match Self::plan(s, k, at, pt(cursor)) {
                    Some(plan) => {
                        let n = plan.stubs.len();
                        let mut w =
                            format!("-> {} continue", if n == 1 { "1 wire".to_string() } else { format!("{n} wires") });
                        if !plan.skipped.is_empty() {
                            w.push_str(&format!(", {} skipped", plan.skipped.len()));
                        }
                        w
                    }
                    None => "->".to_string(),
                };
                out.push(preview_text(s, Pt::new(cursor.x + 10.0 * px, cursor.y + 10.0 * px), px, words, PREVIEW_ACI));
                out
            }
        }
    }
}

// ---------------------------------------------------------------- Go to Reference (§4.5)

/// `cablegoto`: jump at once for a selected reference, else click one.
pub(crate) fn cablegoto(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    Ok(Box::new(Safe(Goto)))
}

struct Goto;

const NOT_A_REFERENCE: &str = "That is not a cable reference.";
const NOTHING_TO_GO: &str = "Nothing there. Click a cable reference.";
const ORPHAN: &str =
    "This reference has no other end (orphan); continue the cable on another page with Continue Cable.";

impl Goto {
    /// The reference under a handle and whether it has another end.
    fn named(s: &Session, h: Handle) -> Result<(RepresentationId, bool), String> {
        read(s, |p, page| {
            let m = p.pages.get(&page).and_then(|v| v.map.get(h)).ok_or(NOT_A_REFERENCE)?;
            let r = reference_named(p, m).ok_or(NOT_A_REFERENCE)?;
            let orphan = refs::all(p.ledger.document()).iter().find(|x| x.reference == r).is_none_or(|x| x.orphan);
            Ok((r, orphan))
        })
    }

    fn jump(s: &mut Session, r: RepresentationId) -> R<Step> {
        let v = commit(s, "cablegoto", &json!({"reference": r.hex()}))?;
        if let Some(m) = v.get("message").and_then(Value::as_str) {
            s.echo(m.to_string());
        }
        Ok(Step::Done)
    }

    fn step(&mut self, s: &mut Session, i: Input) -> R<Step> {
        match i {
            Input::Point(p) if !in_range(p) => Err(other(OUT_OF_RANGE)),
            Input::Point(p) => {
                let h = pick(s, p).ok_or_else(|| other(NOTHING_TO_GO))?;
                let (r, orphan) = Self::named(s, h).map_err(other)?;
                if orphan {
                    return Err(other(ORPHAN));
                }
                Self::jump(s, r)
            }
            Input::Enter => Ok(Step::Done),
            Input::Text(t) => Err(not_a_point(&t)),
            _ => Ok(Step::Continue),
        }
    }
}

impl Interactive for Goto {
    fn name(&self) -> &'static str {
        "CABLEGOTO"
    }
    /// Step 0: one selected reference (its point or its text) jumps at once.
    fn begin(&mut self, s: &mut Session) -> R<Step> {
        safely(s, "cablegoto", |s| {
            let sel = s.selection();
            let found: BTreeSet<(RepresentationId, bool)> =
                sel.iter().take(crate::commands::MAX_HANDLES).filter_map(|h| Self::named(s, *h).ok()).collect();
            match found.iter().next() {
                Some((r, false)) if found.len() == 1 => Self::jump(s, *r),
                Some((_, true)) if found.len() == 1 => {
                    s.echo(ORPHAN);
                    Ok(Step::Continue)
                }
                _ => Ok(Step::Continue),
            }
        })
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        Prompt::new("Select a cable reference", Accept::POINT)
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
        paper_guard(s, &i)?;
        safely(s, "cablegoto", |s| self.step(s, i))
    }
}
