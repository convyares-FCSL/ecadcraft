//! `plcpage` ("Make its page", brief 051, design 051 §3.6): a new schematic page with every channel
//! of a card not drawn yet as cells in rows, and the card body after the last cell when the body is
//! drawn nowhere. One transaction.
//!
//! # Semantics
//! - The page goes after `after` (a page number as shown), else after the last page on which a card
//!   of the same rack (or the rack) is drawn, else after the last schematic page, else at the end.
//! - Its title is `plc.page.title` with the card's words filled in.
//! - Cells start [`LEFT_MM`] from the left of the drawing area at 60 % of its height (outputs at
//!   180°, at 40 %); a row wider than the drawing area wraps into further rows [`ROW_PITCH_MM`]
//!   apart.
//!
//! Excludes: placing anything on an existing page (that is `plcchannels`).

use cadcraft_engine::{CommandSpec, Session};
use ecad_model::book::{self, Book, FOLDER_KEY, NUMBER_KEY};
use ecad_model::{ComponentId, Document, Op, Page, PageId, Placement, Pt, Subject};
use serde_json::{Value, json};

use super::channels::{Join, channels_words, first_number, pins_for, plan_cell, points, roles_of};
use super::rack::{self, CardType, RackRef};
use crate::commands::{err, plain_style, with_any_page};

/// The first cell's distance from the left of the drawing area, mm (design 051 §3.6).
pub const LEFT_MM: f64 = 40.0;
/// From one row of cells to the next, mm: a card's frame (about 40 deep) and 30 clear under it for
/// its rotated channel texts (design 051 §3.6, "30 apart").
pub const ROW_PITCH_MM: f64 = 70.0;
/// Most characters of a page number `after` names (longer cannot be a page).
const MAX_AFTER: usize = 64;

/// Brief 051's commands: `plcpage`.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("plcpage", "Make Card Page", run_plcpage)
            .params("{card: id or tag, after?: page number (absent: after the rack's last card page)} (a new page with every channel not drawn yet, and the body)")
            .noundo(),
    ]
}

/// `plc.page.title` filled in for `card`: `{tag}`, `{type}`, `{channels}`, `{rack}`, `{slot}`,
/// `{slot:02}`.
pub fn title(doc: &Document, card: ComponentId, ty: CardType) -> String {
    let pattern = crate::defaults::resolve(doc, &crate::defaults::plc::PAGE_TITLE, crate::defaults::Scope::project()).text();
    let (rack, slot) = match (rack::rack_ref(doc, card), rack::slot_of(doc, card)) {
        (RackRef::Rack(r), s) => (crate::devices::tag_words(doc, r), s),
        (_, s) => (String::new(), s),
    };
    let channels = if ty.has_channels() { rack::channel_count(doc, card, ty).to_string() } else { String::new() };
    let t = pattern
        .replace("{tag}", &crate::devices::tag_words(doc, card))
        .replace("{type}", ty.code())
        .replace("{channels}", &channels)
        .replace("{rack}", &rack)
        .replace("{slot:02}", &slot.map(|s| format!("{s:02}")).unwrap_or_default())
        .replace("{slot}", &slot.map(|s| s.to_string()).unwrap_or_default());
    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.is_empty() { crate::devices::tag_words(doc, card) } else { t }
}

/// Whether `c` has a card body drawn anywhere.
fn body_drawn(doc: &Document, c: ComponentId) -> bool {
    doc.representations_of(Subject::Component(c))
        .any(|(_, r)| crate::devices::rep_starter(doc, r).is_some_and(|s| CardType::of_body(s.key).is_some()))
}

/// The page the new one goes after (module docs).
fn after_page(doc: &Document, book: &Book, card: ComponentId, after: Option<&str>) -> Result<Option<PageId>, String> {
    if let Some(n) = after {
        let n = n.trim();
        if n.is_empty() || n.chars().count() > MAX_AFTER {
            return Err("`after` is a page number such as \"107\".".into());
        }
        return book.order.iter().copied().find(|p| book.number(doc, *p) == n).map(Some).ok_or_else(|| format!("No page {n}."));
    }
    // The last page on which a card of the same rack (or the rack) is drawn.
    let mut mates: Vec<ComponentId> = Vec::new();
    if let RackRef::Rack(r) = rack::rack_ref(doc, card) {
        mates.push(r);
        mates.extend(rack::cards_naming(doc, r).into_iter().map(|(c, _)| c));
    }
    let pages: std::collections::BTreeSet<PageId> =
        mates.iter().flat_map(|c| doc.representations_of(Subject::Component(*c)).map(|(_, r)| r.page).collect::<Vec<_>>()).collect();
    if let Some(p) = book.order.iter().rev().find(|p| pages.contains(p)) {
        return Ok(Some(*p));
    }
    // Else the last schematic (not generated) page.
    Ok(book.order.iter().rev().find(|p| crate::generated::kind_of(doc, **p).is_none()).copied())
}

/// `plcpage {card, after?}` (design 051 §3.6). One transaction labelled `Make card page`.
pub(crate) fn run_plcpage(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let after = match params.get("after") {
        None | Some(Value::Null) => None,
        Some(Value::String(t)) => Some(t.clone()),
        Some(Value::Number(n)) => Some(n.to_string()),
        Some(_) => return Err(err("`after` is a page number such as \"107\".")),
    };
    let page = with_any_page(s, |p, pg| {
        if crate::generated::has_bom_page(p.ledger.document()) {
            p.outputs();
        }
        let doc = p.ledger.document();
        let card = rack::card_param(doc, params.get("card"))?;
        let ty = super::card_type_of(doc, card).ok_or("Not a PLC card.")?;
        let tag = crate::devices::tag_words(doc, card);
        if ty.is_rack() {
            return Err(format!("{tag} is a rack; Make its page is for a card."));
        }
        let n = if ty.has_channels() { rack::channel_count(doc, card, ty) } else { 0 };
        let pts = points(doc, card);
        let missing: Vec<u32> = (0..n).filter(|i| !pts.contains_key(i)).collect();
        let body = !body_drawn(doc, card);
        if missing.is_empty() && !body {
            let pages = super::overview::pages_of(doc, card);
            let at = pages.first().map(|p| format!(" ({})", crate::references::page_number_words(doc, *p))).unwrap_or_default();
            return Err(if ty.has_channels() { format!("All {n} channels of {tag} are drawn{at}.") } else { format!("{tag} is drawn{at}.") });
        }
        let book = Book::of(doc);
        let before = after_page(doc, &book, card, after.as_deref())?;
        let outputs = p.outputs.as_ref();
        let position = before.and_then(|b| book.position(b)).map_or(book.order.len() + 1, |x| x + 1);
        let number = crate::sheet_cmds::default_number(doc, &book, before, position, outputs);
        let title = title(doc, card, ty);
        let mut new_page = Page::new(title.clone());
        new_page.properties.insert(NUMBER_KEY.into(), number.clone());
        if let Some(f) = before.and_then(|b| book::folder_of(doc, b)).filter(|f| doc.folders.contains_key(f)) {
            new_page.properties.insert(FOLDER_KEY.into(), f.hex());
        }
        let index = before.and_then(|b| doc.page_order.iter().position(|x| *x == b)).map_or(doc.page_order.len(), |i| i + 1);
        let id: PageId = p.ids.fresh();
        let mut ops = vec![Op::AddPage { id, page: new_page, index }];
        // The drawing area, in model units.
        let spec = ecad_model::sheet::sheet_of(doc, id);
        let k = ecad_model::sheet::unit_scale(doc.settings.length_unit);
        let (l, b, r, t) = (
            ecad_model::sheet::MARGIN_MM + 5.0,
            ecad_model::sheet::MARGIN_MM + ecad_model::sheet::TITLE_BLOCK_MM + 5.0,
            f64::from(spec.width) - ecad_model::sheet::MARGIN_MM - 5.0,
            f64::from(spec.height) - ecad_model::sheet::TOP_MARGIN_MM - 5.0,
        );
        let output = super::address::shows_output(ty);
        let (y0, degrees) = if output { (b + 0.4 * (t - b), 180.0_f64) } else { (b + 0.6 * (t - b), 0.0) };
        let style = plain_style(p);
        let mut x_next = l + LEFT_MM;
        let mut y = y0;
        let mut cell_point: Option<Pt> = None;
        let mut cell_w = 0.0;
        if !missing.is_empty() {
            let starter = crate::symbols::by_key(ty.cell()).ok_or("No channel symbol for that type.")?;
            let def = crate::terminal_cmds::definition_for(doc, &mut p.ids, starter, &mut ops)?;
            let w = starter.size().0;
            cell_w = w;
            let per_row = (((r - l - LEFT_MM) / w).floor() as usize).max(1);
            let roles = roles_of(starter);
            let mut used = crate::devices::labels_of(doc, card);
            let local = starter.definition()?.terminals.first().map(|td| td.at).unwrap_or(Pt::new(0.0, 0.0));
            for (j, i) in missing.iter().enumerate() {
                let col = j % per_row;
                if j > 0 && col == 0 {
                    y -= ROW_PITCH_MM;
                }
                let x = l + LEFT_MM + w * col as f64 + w / 2.0;
                let pins = pins_for(doc, Some(card), ty, n, *i, &roles, &mut used);
                let join = Join { index: *i, pins: roles.iter().copied().zip(pins).collect() };
                let pl = Placement { origin: Pt::new(x * k, y * k), rotation: degrees.to_radians(), mirror: false, scale: 1.0 };
                plan_cell(&mut p.ids, id, style.clone(), def, starter, card, &join, pl, &mut ops)?;
                x_next = x + w / 2.0;
                cell_point = Some(pl.apply(local));
            }
        }
        if body {
            let starter = crate::symbols::by_key(ty.body()).ok_or("No body symbol for that type.")?;
            let def = starter.definition()?;
            let (bw, _) = starter.size();
            // Its points on the line of the cells' points (it is the run's power end).
            let local = def.terminals.first().map(|td| td.at).unwrap_or(Pt::new(0.0, 0.0));
            let rot = degrees.to_radians();
            let x = x_next + bw / 2.0;
            let origin = match cell_point {
                Some(q) => {
                    let lp = Placement { origin: Pt::new(0.0, 0.0), rotation: rot, mirror: false, scale: 1.0 }.apply(local);
                    Pt::new(x * k, q.y - lp.y)
                }
                None => Pt::new(x * k, y * k),
            };
            let _ = cell_w;
            let pl = Placement { origin, rotation: rot, mirror: false, scale: 1.0 };
            let plan =
                crate::devices::plan_place(&p.ledger, &mut p.ids, &mut p.tag_high, id, style, starter, pl, crate::devices::Target::Join(card))?;
            ops.extend(plan.ops);
        }
        p.apply_edit(pg, "plcpage", "Make card page", ops)?;
        p.pages.entry(id).or_default();
        let doc = p.ledger.document();
        let first = first_number(doc, card);
        let what = if missing.is_empty() { "its body".to_string() } else { format!("{tag} {}", channels_words(first, &missing)) };
        let message = format!("Made page {number} \"{title}\" with {what}.");
        pg.echo(message.clone());
        Ok(json!({
            "page": id.hex(),
            "number": number,
            "title": title,
            "card": card.hex(),
            "tag": tag,
            "channels": missing.iter().map(|i| i + first).collect::<Vec<_>>(),
            "body": body,
            "message": message,
        }))
    })?;
    Ok(page)
}
