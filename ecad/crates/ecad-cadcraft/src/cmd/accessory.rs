//! The commands of brief 030 (design 030 §2.2, §5.4, §5.5, §10): `accessory` places end stops,
//! end plates, partitions, strip labels and markers; `jumper` makes and edits links. Both write
//! ordinary strip properties, one transaction per call; a link edit's transaction also carries
//! the numbering pass's wire-number ops (FP7, through [`crate::project::Project::apply_edit`]).
//!
//! Each command is a thin shim (brief 070): its params struct reads the JSON, the rules in
//! [`ecad_electrical::cmd::accessory`] decide, and the shim commits the ops and answers in JSON.

use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::cmd::accessory::{self as rules, AccessoryInput, JumperInput, LinkRef, Words};
use ecad_model::links;
use ecad_model::{ComponentId, TerminalId};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

pub use ecad_electrical::cmd::accessory::{MAX_AT, MAX_PICKS};

use crate::cmd::input::{Bounded, Command, Typed, nullable};
use crate::cmd::on_project;

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] = &[Typed::of::<AccessoryParams>("accessory"), Typed::of::<JumperParams>("jumper")];

/// The commands this module adds.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("accessory", "Strip Accessory", crate::cmd::input::typed::<AccessoryParams>)
            .params(
                "{strip?: id, terminal?: id, at: \"start\"|\"end\"|\"all\"|label|[label…] (at most 999), on?: bool (markers on positions), \
items?: [item…]|null (replace), add?: [item…], remove?: [item…], clear?: [acc.… key]} items: end_stop, end_plate, partition, strip_label; on: marker",
            )
            .interactive(crate::interact::accessory)
            .noundo(),
        CommandSpec::new("jumper", "Jumper", crate::cmd::input::typed::<JumperParams>)
            .params(
                "make: {strip?: id, terminals?: [id…], positions?: \"1-4, 7\", skip?: bool} (ranges between consecutive picks unless skip); \
edit: {link: {strip: id, n}, add?: \"5,6\", remove?: \"5\", positions?: text, split_at?: label, drop_missing?: true, delete?: true}",
            )
            .interactive(crate::interact::jumper)
            .noundo(),
    ]
}

/// Words written as one text or as a list.
#[derive(Deserialize, JsonSchema)]
#[serde(untagged)]
enum WordsParam {
    /// A list.
    List(Vec<String>),
    /// One text (items and positions: comma-separated).
    Text(String),
}

impl WordsParam {
    fn words(self) -> Words {
        match self {
            WordsParam::List(v) => Words::List(v),
            WordsParam::Text(t) => Words::Text(t),
        }
    }
}

/// `accessory {strip | terminal, at, on?, items? | add? | remove?, clear?}` (design 030 §2.2):
/// one `SetProperty` per place, all in one transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct AccessoryParams {
    /// The strip.
    #[serde(default)]
    strip: Option<ComponentId>,
    /// A terminal of the strip (its position is the place when `at` is absent).
    #[serde(default)]
    terminal: Option<TerminalId>,
    /// "start", "end", "all", a position label, or a list of them (at most 999).
    #[serde(default)]
    at: Option<WordsParam>,
    /// Markers on positions.
    #[serde(default)]
    on: Option<bool>,
    /// The items to put there (replace; null: none): end_stop, end_plate, partition, strip_label;
    /// with `on`: marker.
    #[serde(default, deserialize_with = "nullable")]
    items: Option<Option<WordsParam>>,
    /// Items to add.
    #[serde(default, deserialize_with = "nullable")]
    add: Option<Option<WordsParam>>,
    /// Items to remove.
    #[serde(default, deserialize_with = "nullable")]
    remove: Option<Option<WordsParam>>,
    /// Placement keys (acc.…) emptied in the same transaction.
    #[serde(default)]
    clear: Option<Vec<String>>,
}

impl Command for AccessoryParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let words = |w: Option<Option<WordsParam>>| w.map(|w| w.map(WordsParam::words));
        let input = AccessoryInput {
            strip: self.strip,
            terminal: self.terminal,
            at: self.at.map(WordsParam::words),
            on: self.on.unwrap_or(false),
            items: words(self.items),
            add: words(self.add),
            remove: words(self.remove),
            clear: self.clear,
        };
        let v = on_project(s, |p, pg, _| {
            let done = rules::plan_accessory(p.ledger.document(), input)?;
            let changed = !done.ops.is_empty();
            if changed {
                p.apply_edit(pg, "accessory", &done.label, done.ops)?;
            }
            let keys: Vec<Value> =
                done.keys.iter().map(|(k, list)| json!({"key": k, "items": list.iter().map(|a| a.key()).collect::<Vec<_>>()})).collect();
            Ok(json!({"strip": done.strip.hex(), "keys": keys, "changed": changed, "message": done.message}))
        })?;
        if let Some(m) = v.get("message").and_then(Value::as_str) {
            s.echo(m.to_string());
        }
        Ok(v)
    }
}

// ---------------------------------------------------------------- jumper (§5.4, §5.5)

/// The link a `jumper` edit names.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct LinkParam {
    /// The strip.
    #[serde(default)]
    strip: Option<ComponentId>,
    /// The link's number, 1 to 999.
    #[serde(default)]
    n: Option<u64>,
}

/// `jumper` (design 030 §10): make a link (`{strip | terminals, positions?, skip?}`), or edit one
/// (`{link: {strip, n}, add?, remove?, positions?, split_at?, drop_missing?, delete?}`).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct JumperParams {
    /// Edit: the link.
    #[serde(default)]
    link: Option<LinkParam>,
    /// Make: the strip.
    #[serde(default)]
    strip: Option<ComponentId>,
    /// Make: the picked terminals (at most 999).
    #[serde(default)]
    terminals: Option<Bounded<TerminalId, 0, MAX_PICKS>>,
    /// Make: no ranges between consecutive picks.
    #[serde(default)]
    skip: Option<bool>,
    /// The positions: text like "1-4, 7" or a list of labels.
    #[serde(default)]
    positions: Option<WordsParam>,
    /// Edit: positions to add ("5,6" or a list).
    #[serde(default)]
    add: Option<WordsParam>,
    /// Edit: positions to remove ("5" or a list).
    #[serde(default)]
    remove: Option<WordsParam>,
    /// Edit: split the link after this position.
    #[serde(default)]
    split_at: Option<String>,
    /// Edit: drop the positions the strip lacks.
    #[serde(default)]
    drop_missing: Option<bool>,
    /// Edit: delete the link.
    #[serde(default)]
    delete: Option<bool>,
}

impl Command for JumperParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let text = |w: Option<WordsParam>| w.map(|w| w.words().text());
        let input = JumperInput {
            link: self.link.map(|l| LinkRef { strip: l.strip, n: l.n }),
            strip: self.strip,
            terminals: self.terminals.map(|t| t.0).unwrap_or_default(),
            skip: self.skip.unwrap_or(false),
            positions: text(self.positions),
            add: text(self.add),
            remove: text(self.remove),
            split_at: self.split_at,
            drop_missing: self.drop_missing.unwrap_or(false),
            delete: self.delete.unwrap_or(false),
        };
        let v = on_project(s, |p, pg, _| {
            let done = rules::plan_jumper(p.ledger.document(), input)?;
            let (c, n) = (done.strip, done.n);
            let changed = !done.ops.is_empty();
            if changed {
                p.apply_edit(pg, "jumper", &done.label, done.ops)?;
            }
            let doc = p.ledger.document();
            let positions = doc.components.get(&c).and_then(|x| x.properties.get(&links::key(n))).cloned();
            let bars = p.outputs().strips.get(c).and_then(|d| d.acc.links.iter().find(|l| l.n == n)).map_or(0, |l| l.bars);
            let mut v = json!({"strip": c.hex(), "link": n, "positions": positions, "bars": bars, "changed": changed, "message": done.message});
            if let Some((m, t)) = done.split {
                v["split"] = json!({"link": m, "positions": t});
            }
            Ok(v)
        })?;
        if let Some(m) = v.get("message").and_then(Value::as_str) {
            s.echo(m.to_string());
        }
        Ok(v)
    }
}
