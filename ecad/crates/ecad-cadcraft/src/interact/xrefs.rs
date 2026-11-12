//! The interactive form of Move Mirror Table (brief 025, design 025 §4.4) and the device side of
//! Go to Reference (design 025 §5.3). Like every ECAD form they only collect and resolve values;
//! each commit runs the command's JSON form (`mirrormove`, `refgoto`).

use cadcraft_engine::doc::{EntityKind, Handle};
use cadcraft_engine::geom::Vec2;
use cadcraft_engine::{Accept, Input, Interactive, Prompt, Session, Step};
use ecad_model::{ComponentId, RepresentationId, Subject};
use serde_json::{Value, json};

use super::forms::{R, Safe, commit, not_a_point, other, paper_guard, safely};
use super::{OUT_OF_RANGE, can_start, in_range, line_kind, pick, pt, read};
use crate::project::{Mapped, Project};
use crate::xref;

const NOT_A_TABLE: &str = "That is not a contact table or a coil. Click a coil or its table.";
const NOTHING_THERE: &str = "Nothing there. Click a coil or its contact table.";
const PLACEMENTS: [&str; 4] = ["Auto", "Below", "Right", "Off"];

/// The device whose table a mapped object names: a table part, or a head symbol (its item or tag).
fn table_named(p: &Project, m: Mapped) -> Option<ComponentId> {
    let doc = p.ledger.document();
    let rep = match m {
        Mapped::Xref(r) | Mapped::Tag(r) | Mapped::Marker(r, _) => r,
        Mapped::Item(i) => doc.representation_of_item(i)?.0,
        _ => return None,
    };
    let Subject::Component(c) = doc.representations.get(&rep)?.subject else { return None };
    (xref::device(doc, c).head == Some(rep)).then_some(c)
}

/// The reference source and target a mapped object names for Go to Reference (design 025
/// §5.3): a contact's text or symbol goes to its coil; a table row to its contact; a coil (or its
/// table's bar) to its first contact.
pub(crate) fn device_source(p: &Project, map: &crate::project::HandleMap, h: Handle) -> Option<(RepresentationId, Option<RepresentationId>)> {
    let doc = p.ledger.document();
    let m = map.get(h)?;
    let rep = match m {
        Mapped::Xref(r) | Mapped::Tag(r) | Mapped::Marker(r, _) => r,
        Mapped::Item(i) => doc.representation_of_item(i)?.0,
        _ => return None,
    };
    let (_, class) = p.xref.member(rep)?;
    let target = match (m, class) {
        (Mapped::Xref(_), xref::Class::Head) => {
            let (_, i) = map.xref_part(h)?;
            let Subject::Component(c) = doc.representations.get(&rep)?.subject else { return None };
            let dev = xref::device(doc, c);
            let mut places = xref::Places::new(doc);
            let t = xref::table(doc, &mut places, &dev)?;
            let row = t.parts.get(usize::try_from(i).ok()?)?.row;
            row.and_then(|r| t.rows.get(r)).and_then(|r| r.rep)
        }
        _ => None,
    };
    Some((rep, target))
}

/// Go to the place a device reference shows (the JSON form `refgoto`).
pub(crate) fn jump(s: &mut Session, source: RepresentationId, target: Option<RepresentationId>) -> R<Step> {
    let mut params = json!({"source": source.hex()});
    if let Some(t) = target {
        params["target"] = json!(t.hex());
    }
    let v = commit(s, "refgoto", &params)?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(Step::Done)
}

/// `mirrormove`: pick a coil or its table, then click where the table goes (or type a word).
pub(crate) fn mirrormove(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    Ok(Box::new(Safe(MirrorMove::default())))
}

#[derive(Default)]
struct MirrorMove {
    device: Option<ComponentId>,
}

impl MirrorMove {
    fn choose(s: &Session, h: Handle) -> Result<ComponentId, String> {
        read(s, |p, page| {
            let m = p.pages.get(&page).and_then(|v| v.map.get(h)).ok_or(NOT_A_TABLE)?;
            table_named(p, m).ok_or_else(|| NOT_A_TABLE.to_string())
        })
    }

    fn commit(s: &mut Session, c: ComponentId, params: Value) -> R<Step> {
        let mut params = params;
        params["component"] = json!(c.hex());
        let v = commit(s, "mirrormove", &params)?;
        if let Some(m) = v.get("message").and_then(Value::as_str) {
            s.echo(m.to_string());
        } else if v.get("changed").and_then(Value::as_bool) == Some(false) {
            s.echo("The contact table is already there.");
        }
        Ok(Step::Done)
    }

    fn step(&mut self, s: &mut Session, i: Input) -> R<Step> {
        match (i, self.device) {
            (Input::Point(p), _) if !in_range(p) => Err(other(OUT_OF_RANGE)),
            (Input::Point(p), None) => {
                let h = pick(s, p).ok_or_else(|| other(NOTHING_THERE))?;
                self.device = Some(Self::choose(s, h).map_err(other)?);
                Ok(Step::Continue)
            }
            (Input::Point(p), Some(c)) => Self::commit(s, c, json!({"at": [p.x, p.y]})),
            (Input::Keyword(k) | Input::Text(k), Some(c)) => {
                let w = k.trim().to_ascii_lowercase();
                match PLACEMENTS.iter().find(|x| x.to_ascii_lowercase() == w || (!w.is_empty() && x.to_ascii_lowercase().starts_with(&w))) {
                    Some(word) => Self::commit(s, c, json!({"placement": word.to_ascii_lowercase()})),
                    None => Err(not_a_point(&k)),
                }
            }
            (Input::Enter, _) => Ok(Step::Done),
            (Input::Text(t), None) => Err(not_a_point(&t)),
            _ => Ok(Step::Continue),
        }
    }
}

impl Interactive for MirrorMove {
    fn name(&self) -> &'static str {
        "MIRRORMOVE"
    }
    /// One selected coil or table part chooses the device at once.
    fn begin(&mut self, s: &mut Session) -> R<Step> {
        safely(s, "mirrormove", |s| {
            let found: std::collections::BTreeSet<ComponentId> =
                s.selection().iter().take(crate::commands::MAX_HANDLES).filter_map(|h| Self::choose(s, *h).ok()).collect();
            if found.len() == 1 {
                self.device = found.into_iter().next();
            }
            Ok(Step::Continue)
        })
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match self.device {
            None => Prompt::new("Select contact table or coil", Accept::POINT),
            Some(_) => Prompt::new("Place the table", Accept::POINT).kw(&PLACEMENTS),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
        paper_guard(s, &i)?;
        safely(s, "mirrormove", |s| self.step(s, i))
    }
    /// The table follows the cursor, as it will be drawn.
    fn preview(&self, s: &Session, cursor: Vec2) -> Vec<EntityKind> {
        let Some(c) = self.device else { return Vec::new() };
        if !in_range(cursor) {
            return Vec::new();
        }
        read(s, |p, _| {
            let doc = p.ledger.document();
            let dev = xref::device(doc, c);
            let Some(head) = dev.head else { return Ok(Vec::new()) };
            let mut places = xref::Places::new(doc);
            let offer = xref::offer_of(doc, c);
            let rows = xref::rows(&mut places, &dev, offer.as_ref().map(|(_, o)| o.as_slice()));
            let parts = xref::table_parts(doc, head, pt(cursor), &rows);
            Ok(parts
                .iter()
                .filter_map(|part| match &part.shape {
                    xref::Shape::Line(a, b) => Some(line_kind(*a, *b)),
                    xref::Shape::Text { .. } => None,
                })
                .collect())
        })
        .unwrap_or_default()
    }
}
