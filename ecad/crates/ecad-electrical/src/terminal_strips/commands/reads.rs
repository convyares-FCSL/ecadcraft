//! The reads of the terminal strips: `stripplan` and `cablecores`. Both are
//! generic ([`crate::commands::run`]): they read the report models and record nothing.

use ecad_model::{CableId, DeviceId, Noun, WireId};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::commands::run::{Access, Cx, Generic, Outcome, Pages};
use crate::error::CmdError;
use crate::terminal_strips::commands::terminal_strips as rules;
use crate::terminal_strips::strip_model::StripData;

/// Every strip's terminal blocks, drawn and spare, in tag order, paged by rows: `after` rows are skipped and at
/// most `limit` returned; a strip split across pages carries `first_row`, `total` counts the rows and `next` is
/// the next `after`. Read only.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StripplanParams {
    /// Only this strip.
    #[serde(default)]
    strip: Option<DeviceId>,
    /// At most this many rows (500, at most 5000).
    #[serde(default)]
    limit: Option<u64>,
    /// Skip this many rows (the `next` of the last call).
    #[serde(default)]
    after: Option<u64>,
    /// Gap rows, markers and jumper bars (default true).
    #[serde(default)]
    accessories: Option<bool>,
}

impl Generic for StripplanParams {
    const ID: &'static str = "stripplan";
    const TITLE: &'static str = "Terminal Plan";
    const PAGES: Pages = Pages::Any;
    const ACCESS: Access = Access::Reads;

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let limit =
            self.limit.map_or(crate::reports::DEFAULT_LIMIT, |n| usize::try_from(n).unwrap_or(usize::MAX)).clamp(1, crate::reports::MAX_LIMIT);
        let after = self.after.map_or(0, |n| usize::try_from(n).unwrap_or(usize::MAX));
        let accessories = self.accessories.unwrap_or(true);
        let doc = cx.doc;
        let style = doc.settings().reference_style();
        let o = cx.derived.reports(doc);
        let shown: Vec<&StripData> = match self.strip {
            Some(c) => vec![o.strips.get(c).ok_or(CmdError::NotFound(Noun::TerminalStrip))?],
            None => o.sorted_strips(),
        };
        let lens: Vec<usize> = shown.iter().map(|d| if accessories { d.rows.len() + d.acc.gap_rows() } else { d.rows.len() }).collect();
        let page = rules::plan_page(&lens, after, limit);
        let out: Vec<Value> =
            page.parts.iter().filter_map(|(i, first, end)| shown.get(*i).map(|d| rules::strip_json(d, style, *first, *end, accessories))).collect();
        let mut v = json!({"strips": out, "total": page.total, "fields": rules::plan_fields_json()});
        if let Some(n) = page.next {
            v["next"] = json!(n);
        }
        Ok(Outcome::answer(v))
    }
}

/// A cable's offered cores: used, spare and over. Read only.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CablecoresParams {
    /// The cable.
    cable: CableId,
}

impl Generic for CablecoresParams {
    const ID: &'static str = "cablecores";
    const TITLE: &'static str = "Cable Cores";
    const PAGES: Pages = Pages::Any;
    const ACCESS: Access = Access::Reads;

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let k = self.cable;
        let doc = cx.doc;
        let o = cx.derived.reports(doc);
        let row = o.cable_row(k).cloned().ok_or(CmdError::NotFound(ecad_model::Noun::Cable))?;
        let wire = |w: WireId| o.wire(w).cloned();
        let style = doc.settings().reference_style();
        let o_chain = |i: usize| o.chain_ends(&row, i, style);
        let count = &row.count;
        let member_json = |i: usize, label: Option<String>, state: &str| {
            let w = row.members.get(i).copied();
            let r = w.and_then(wire);
            let mut o = serde_json::Map::new();
            rules::put(&mut o, "label", label);
            o.insert("state".into(), json!(state));
            rules::put(&mut o, "conductor", w.map(|w| w.hex()));
            rules::put(&mut o, "wire", r.as_ref().and_then(|r| r.wire_number.clone()));
            // A core is a chain of members through continuations, From to To.
            let (from, to) = o_chain(i);
            rules::put(&mut o, "from", from);
            rules::put(&mut o, "to", to);
            Value::Object(o)
        };
        // The rows are the cable's core chains, each with its stored label.
        let stored = |i: usize| row.member_cores.get(i).cloned().flatten();
        let mut cores = Vec::new();
        match &count.offer {
            Some(offer) => {
                for label in offer {
                    match count.matched.iter().position(|m| m.as_deref() == Some(label.as_str())) {
                        Some(i) => cores.push(member_json(i, Some(label.clone()), "used")),
                        // A spare core has no ends.
                        None => cores.push(json!({"label": label, "state": "spare"})),
                    }
                }
            }
            None => {
                for i in 0..row.members.len() {
                    cores.push(member_json(i, stored(i), "used"));
                }
            }
        }
        let over: Vec<Value> = count.over.iter().map(|i| member_json(*i, stored(*i), "over")).collect();
        let mut v = serde_json::Map::new();
        v.insert("cable".into(), json!(k.hex()));
        rules::put(&mut v, "tag", row.tag.as_ref());
        rules::put(&mut v, "offer", count.offer_text.as_ref());
        rules::put(&mut v, "source", count.source.map(|s| s.word()));
        v.insert("used".into(), json!(count.used()));
        rules::put(&mut v, "spare", count.offer.as_ref().map(|_| count.spare.clone()));
        v.insert("cores".into(), Value::Array(cores));
        v.insert("over".into(), Value::Array(over));
        Ok(Outcome::answer(Value::Object(v)))
    }
}
