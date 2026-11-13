//! The interactive form of brief 047's `bomitem` (design 047 §6.1, Reports ▸ BOM ▸ **+ Item**):
//! description, quantity, unit, location, part number, maker; then one commit of the JSON form.
//! Enter on an optional prompt keeps its default; Esc writes nothing.

use cadcraft_engine::{Accept, Input, Interactive, Prompt, Session, Step};
use serde_json::{Map, Value, json};

use super::can_start;
use super::forms::{R, Safe, commit, other, safely};

/// `bomitem`: the prompts of design 047 §6.1.
pub(crate) fn bomitem(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    let (metres, location) = super::read(s, |p, _| {
        let doc = p.ledger.document();
        Ok((crate::bom::item_unit_metres(doc), doc.properties.get(ecad_model::book::LOCATION_KEY).cloned()))
    })
    .unwrap_or((false, None));
    Ok(Box::new(Safe(BomItem { step: 0, fields: Map::new(), metres, location })))
}

struct BomItem {
    /// 0 description, 1 quantity, 2 unit, 3 location, 4 part number, 5 maker.
    step: u8,
    fields: Map<String, Value>,
    /// The unit default (the registry's `bom.item.unit`), then the unit chosen.
    metres: bool,
    /// The location default (the project's).
    location: Option<String>,
}

impl BomItem {
    fn take(&mut self, s: &mut Session, text: Option<String>) -> R<Step> {
        let t = text.map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
        match (self.step, t) {
            (0, None) => return Err(other(crate::bom::ITEM_NEEDS_DESCRIPTION)),
            (0, Some(d)) => {
                self.fields.insert("description".into(), json!(d));
            }
            (1, Some(q)) => {
                // Checked when the unit is known; a count of pieces must be whole.
                crate::bom::parse_qty(&q, true).map_err(other)?;
                self.fields.insert("qty".into(), json!(q));
            }
            (2, Some(u)) => {
                let w = u.to_ascii_lowercase();
                self.metres = match w.as_str() {
                    "p" | "pieces" | "pcs" => false,
                    "m" | "metres" | "meters" => true,
                    _ => return Err(other("Choose Pieces or Metres.")),
                };
            }
            (2, None) => {}
            (3, Some(l)) => {
                self.fields.insert("location".into(), json!(l));
            }
            (4, Some(pn)) => {
                self.fields.insert("part_number".into(), json!(pn));
            }
            (5, Some(m)) => {
                self.fields.insert("maker".into(), json!(m));
            }
            _ => {}
        }
        if self.step == 2 {
            self.fields.insert("unit".into(), json!(if self.metres { "m" } else { "pcs" }));
            if let Some(q) = self.fields.get("qty").and_then(Value::as_str) {
                crate::bom::parse_qty(q, self.metres).map_err(|e| {
                    self.step = 1;
                    other(e)
                })?;
            }
        }
        if self.step < 5 {
            self.step += 1;
            return Ok(Step::Continue);
        }
        commit(s, "bomitem", &Value::Object(self.fields.clone()))?;
        Ok(Step::Done)
    }
}

impl Interactive for BomItem {
    fn name(&self) -> &'static str {
        "BOMITEM"
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match self.step {
            0 => Prompt::new("Item description", Accept::TEXT),
            1 => Prompt::new("Quantity <1>", Accept::TEXT),
            2 => Prompt::new(format!("Unit [Pieces/Metres] <{}>", if self.metres { "Metres" } else { "Pieces" }), Accept::TEXT)
                .kw(&["Pieces", "Metres"]),
            3 => Prompt::new(format!("Location <{}>", self.location.as_deref().unwrap_or("none")), Accept::TEXT),
            4 => Prompt::new("Part number <none>", Accept::TEXT),
            _ => Prompt::new("Maker <none>", Accept::TEXT),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
        safely(s, "bomitem", |s| match i {
            Input::Text(t) | Input::Keyword(t) => self.take(s, Some(t)),
            Input::Enter => self.take(s, None),
            _ => Ok(Step::Continue),
        })
    }
}
