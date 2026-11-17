//! Brief 057's commands (design 057 §8): `rating` and `ratingtable` (write) and `ratings` (read).
//!
//! # Semantics
//! - `rating {wire | cable | many, …}` sets a subject's rating properties in one transaction of
//!   `SetProperty` ops; `null` clears a key, an absent key is untouched. Every value is checked
//!   ([`crate::rating::parse`]); a refusal names the key and its range, and nothing is written.
//! - `ratingtable {name, source?, csv}` creates or replaces a rating table (one `SetStyle`);
//!   `{import: path}` reads a CSV from disk, `{export: path}` writes one (command line and CLI only;
//!   MCP refuses both and passes `csv`). An import with any bad row is refused whole.
//! - `ratings {wire | cable | {}}` reads the results the panel shows. It records nothing.
//!
//! Programmatic calls never prompt.
//!
//! Each command is a thin shim (brief 070): its params struct reads the JSON, the rules in
//! [`ecad_electrical::cmd::rating`] decide, and the shim reads or writes the CSV file, commits the
//! ops and answers in JSON.

use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::CmdError;
use ecad_electrical::cmd::rating::{self as rules, RatingItem, TableWrite};
use ecad_model::{Document, Word};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::cmd::input::{Bounded, Command, TextOrNumber, Typed, nullable};
use crate::cmd::{err, on_any_page, on_project};
use crate::rating::{self, RatedSubject, Rating, dataset};

pub use rules::{MAX_MANY, PAGE};

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] =
    &[Typed::of::<RatingParams>("rating"), Typed::of::<RatingtableParams>("ratingtable"), Typed::of::<RatingsParams>("ratings")];

/// The commands of this module, registered through [`crate::lanes`].
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("rating", "Rating", crate::cmd::input::typed::<RatingParams>)
            .params(
                "{wire: conductor id or wire number | cable: tag or id | many: [{wire|cable, …}], basis?: \"datasheet\"|\"table\"|\"manual\", \
manual?, bundling?: \"0.8\"|\"3 bunched\", load?, protected_by?: \"-Q01\"|\"-F01:3\", method?, ambient?, grouped?, voltage?, system?: \
\"ac1\"|\"ac3\"|\"dc\", vd_limit?, cable_type?: id or name} | {location: \"+CAB1\", temperature: °C | null} (a location's panel \
temperature, on its location boxes; null clears; one transaction)",
            )
            .noundo(),
        CommandSpec::new("ratingtable", "Rating Table", crate::cmd::input::typed::<RatingtableParams>)
            .params("{name, source?, csv, layer?: \"project\"|\"standard\"} (creates or replaces; one transaction) | {import: path, name?, source?} | {export: path, name?}")
            .noundo(),
        CommandSpec::new("ratings", "Ratings", crate::cmd::input::typed::<RatingsParams>)
            .params("{wire?: id or number, cable?: tag or id, from?: n} (read only)")
            .noundo(),
    ]
}

// ---------------------------------------------------------------------------------------------
// rating

/// A rating key's value: text or a number (`35` reads as `"35"`), or null to clear it.
type KeyValue = Option<Option<TextOrNumber>>;

/// The keys given, in [`rules::KEYS`] order: (JSON key, value; `None` clears).
fn sets_of(given: [(&'static str, &KeyValue); 12]) -> Vec<(&'static str, Option<String>)> {
    given.into_iter().filter_map(|(k, v)| v.as_ref().map(|v| (k, v.as_ref().map(|t| t.text().trim().to_string())))).collect()
}

/// The rating keys of a params struct, in [`rules::KEYS`] order.
macro_rules! keys_of {
    ($p:expr) => {
        [
            ("basis", &$p.basis),
            ("manual", &$p.manual),
            ("bundling", &$p.bundling),
            ("load", &$p.load),
            ("protected_by", &$p.protected_by),
            ("method", &$p.method),
            ("ambient", &$p.ambient),
            ("grouped", &$p.grouped),
            ("voltage", &$p.voltage),
            ("system", &$p.system),
            ("vd_limit", &$p.vd_limit),
            ("cable_type", &$p.cable_type),
        ]
    };
}

/// One subject of `rating {many}`: a wire or a cable, and the keys to set (null clears a key).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RatingItemParams {
    /// A panel wire: its conductor id or wire number.
    #[serde(default)]
    wire: Option<String>,
    /// A cable: its tag or id.
    #[serde(default)]
    cable: Option<String>,
    /// How the rating is found: datasheet, table or manual.
    #[serde(default, deserialize_with = "nullable")]
    basis: KeyValue,
    /// The rated current (A) when the basis is manual.
    #[serde(default, deserialize_with = "nullable")]
    manual: KeyValue,
    /// Panel wires: a factor 0.30 to 1.00, or a circuit count ("3 bunched").
    #[serde(default, deserialize_with = "nullable")]
    bundling: KeyValue,
    /// The load current (A).
    #[serde(default, deserialize_with = "nullable")]
    load: KeyValue,
    /// The protecting device: a tag such as -Q01, or -F01:3 for a fuse of a bank.
    #[serde(default, deserialize_with = "nullable")]
    protected_by: KeyValue,
    /// Cables: the IEC 60364-5-52 installation method.
    #[serde(default, deserialize_with = "nullable")]
    method: KeyValue,
    /// Cables: the ambient temperature (°C).
    #[serde(default, deserialize_with = "nullable")]
    ambient: KeyValue,
    /// Cables: the circuits grouped, 1 to 20.
    #[serde(default, deserialize_with = "nullable")]
    grouped: KeyValue,
    /// Cables: the voltage (V), for the voltage drop.
    #[serde(default, deserialize_with = "nullable")]
    voltage: KeyValue,
    /// Cables: ac1, ac3 or dc.
    #[serde(default, deserialize_with = "nullable")]
    system: KeyValue,
    /// Cables: the voltage drop limit (%).
    #[serde(default, deserialize_with = "nullable")]
    vd_limit: KeyValue,
    /// Cables: the cable type, an id or a name.
    #[serde(default, deserialize_with = "nullable")]
    cable_type: KeyValue,
}

impl RatingItemParams {
    fn item(&self) -> RatingItem {
        RatingItem { wire: self.wire.clone(), cable: self.cable.clone(), sets: sets_of(keys_of!(self)) }
    }
}

/// `rating {wire | cable | many, …}` sets a subject's rating properties in one transaction (null
/// clears a key, an absent key is untouched); `{location, temperature}` sets a location's panel
/// temperature on its location boxes.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RatingParams {
    /// A panel wire: its conductor id or wire number.
    #[serde(default)]
    wire: Option<String>,
    /// A cable: its tag or id.
    #[serde(default)]
    cable: Option<String>,
    /// Several subjects at once (at most 500), each `{wire | cable, …}`.
    #[serde(default)]
    many: Option<Bounded<RatingItemParams, 0, MAX_MANY>>,
    /// How the rating is found: datasheet, table or manual.
    #[serde(default, deserialize_with = "nullable")]
    basis: KeyValue,
    /// The rated current (A) when the basis is manual.
    #[serde(default, deserialize_with = "nullable")]
    manual: KeyValue,
    /// Panel wires: a factor 0.30 to 1.00, or a circuit count ("3 bunched").
    #[serde(default, deserialize_with = "nullable")]
    bundling: KeyValue,
    /// The load current (A).
    #[serde(default, deserialize_with = "nullable")]
    load: KeyValue,
    /// The protecting device: a tag such as -Q01, or -F01:3 for a fuse of a bank.
    #[serde(default, deserialize_with = "nullable")]
    protected_by: KeyValue,
    /// Cables: the IEC 60364-5-52 installation method.
    #[serde(default, deserialize_with = "nullable")]
    method: KeyValue,
    /// Cables: the ambient temperature (°C).
    #[serde(default, deserialize_with = "nullable")]
    ambient: KeyValue,
    /// Cables: the circuits grouped, 1 to 20.
    #[serde(default, deserialize_with = "nullable")]
    grouped: KeyValue,
    /// Cables: the voltage (V), for the voltage drop.
    #[serde(default, deserialize_with = "nullable")]
    voltage: KeyValue,
    /// Cables: ac1, ac3 or dc.
    #[serde(default, deserialize_with = "nullable")]
    system: KeyValue,
    /// Cables: the voltage drop limit (%).
    #[serde(default, deserialize_with = "nullable")]
    vd_limit: KeyValue,
    /// Cables: the cable type, an id or a name.
    #[serde(default, deserialize_with = "nullable")]
    cable_type: KeyValue,
    /// A location (such as +CAB1) whose panel temperature is set, with `temperature`.
    #[serde(default)]
    location: Option<String>,
    /// With `location`: the panel temperature (°C), or null to clear it.
    #[serde(default, deserialize_with = "nullable")]
    temperature: KeyValue,
}

impl RatingParams {
    fn single(&self) -> RatingItem {
        RatingItem { wire: self.wire.clone(), cable: self.cable.clone(), sets: sets_of(keys_of!(self)) }
    }

    /// The parameters given besides `location` and `temperature`, in name order.
    fn others(&self) -> Vec<&'static str> {
        let mut given: Vec<&'static str> = [("cable", self.cable.is_some()), ("many", self.many.is_some()), ("wire", self.wire.is_some())]
            .into_iter()
            .filter(|g| g.1)
            .map(|g| g.0)
            .collect();
        given.extend(self.single().sets.iter().map(|(k, _)| *k));
        given.sort_unstable();
        given
    }
}

impl Command for RatingParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        if let Some(loc) = &self.location {
            return run_location(s, loc, &self);
        }
        if self.temperature.is_some() {
            return Err(err(rules::not_a_key("temperature").to_string()));
        }
        let items: Vec<RatingItem> = match &self.many {
            Some(many) => many.0.iter().map(RatingItemParams::item).collect(),
            None => vec![self.single()],
        };
        on_project(s, |p, pg, _| {
            let planned = rules::plan_rating(p.ledger.document(), &items)?;
            let changed = !planned.ops.is_empty();
            if changed {
                p.apply_edit(pg, "rating", &planned.label, planned.ops)?;
            }
            let doc = p.ledger.document();
            let results: Vec<Value> = planned.subjects.iter().take(PAGE).map(|s| rating_json(doc, &rating::rating_of(doc, *s))).collect();
            Ok(json!({"changed": changed, "ratings": results}))
        })
    }
}

/// `rating {location, temperature}` (design 057 FP6; tester F1): the location boxes of
/// `location` take `rating.temperature`, written here (034's `setprop` rule reads `rating.<n>` as a
/// fuse bank's and refuses it on a box).
fn run_location(s: &mut Session, loc: &str, params: &RatingParams) -> cadcraft_engine::Result<Value> {
    let typed = loc.trim().to_string();
    if typed.is_empty() {
        return Err(err("`location` names a location, such as +CAB1."));
    }
    if let Some(k) = params.others().first() {
        return Err(err(format!("{k} is not a location rating key; give `temperature`.")));
    }
    let after = match &params.temperature {
        Some(None) => None,
        Some(Some(t)) => Some(rules::check_temperature(&t.text()).map_err(|e| err(e.to_string()))?),
        None => return Err(err("Give `temperature` (\u{b0}C), or null to clear it.")),
    };
    on_project(s, |p, pg, _| {
        let planned = rules::plan_temperature(p.ledger.document(), &typed, after.as_deref())?;
        let changed = !planned.ops.is_empty();
        if changed {
            p.apply_edit(pg, "rating", &planned.label, planned.ops)?;
        }
        Ok(json!({"changed": changed, "location": planned.location, "boxes": planned.boxes, "temperature": after}))
    })
}

// ---------------------------------------------------------------------------------------------
// ratingtable

/// `ratingtable {name, source?, csv, layer?}` creates or replaces a rating table (one
/// `SetStyle`); `{import: path, name?, source?}` reads a CSV file, `{export: path, name?}` writes
/// one.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RatingtableParams {
    /// The table's name (default: the imported file's name, or "Rating table"); with `export`, the
    /// table to write (default: the one chosen).
    #[serde(default)]
    name: Option<String>,
    /// Where the values come from (at most 200 characters).
    #[serde(default)]
    source: Option<String>,
    /// The rows, as CSV.
    #[serde(default)]
    csv: Option<String>,
    /// project (the default) or standard.
    #[serde(default)]
    layer: Option<String>,
    /// Read the rows from this CSV file.
    #[serde(default)]
    import: Option<String>,
    /// Write the table to this CSV file.
    #[serde(default)]
    export: Option<String>,
}

impl Command for RatingtableParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let trim = |v: &Option<String>| v.as_deref().map(|t| t.trim().to_string());
        let name = trim(&self.name);
        if let Some(path) = trim(&self.export) {
            return on_any_page(s, |p, _| {
                let t = rules::export_table(p.ledger.document(), name.as_deref())?;
                std::fs::write(&path, dataset::to_csv(&t.data)).map_err(|e| CmdError::refused(format!("Could not write {path}: {e}")))?;
                Ok(json!({"exported": path, "rows": t.data.rows.len()}))
            });
        }
        let (csv, from_file) = match (trim(&self.import), self.csv) {
            (Some(path), None) => {
                let meta = std::fs::metadata(&path).map_err(|e| err(format!("Could not read {path}: {e}")))?;
                if meta.len() > dataset::MAX_BYTES as u64 {
                    return Err(err(rules::too_big().to_string()));
                }
                (std::fs::read_to_string(&path).map_err(|e| err(format!("Could not read {path} as text: {e}")))?, Some(path))
            }
            (None, Some(c)) => (c, None),
            (Some(_), Some(_)) => return Err(err("Give `csv` or `import`, not both.")),
            (None, None) => return Err(err("Give `csv` (the rows), `import` (a CSV file) or `export` (a CSV file).")),
        };
        let default_name = from_file
            .as_deref()
            .and_then(|f| std::path::Path::new(f).file_stem().and_then(|s| s.to_str()))
            .map(str::to_string)
            .unwrap_or_else(|| "Rating table".to_string());
        let w = TableWrite { csv, name: name.unwrap_or(default_name), source: trim(&self.source), layer: ecad_model::style::StyleLayer::Project };
        // The rows, name and source are checked before the layer and the project (as before).
        rules::check_table(&w).map_err(|e| err(e.to_string()))?;
        let w = TableWrite { layer: rules::table_layer(self.layer.as_deref()).map_err(|e| err(e.to_string()))?, ..w };
        on_project(s, |p, pg, _| {
            let plan = rules::plan_table(p.ledger.document(), &mut p.ids, &w)?;
            let changed = !plan.ops.is_empty();
            if changed {
                p.apply_edit(pg, "ratingtable", &plan.label, plan.ops)?;
            }
            Ok(json!({"id": plan.id.text(), "name": w.name, "rows": plan.rows, "changed": changed, "odd": plan.odd}))
        })
    }
}

// ---------------------------------------------------------------------------------------------
// ratings

/// `ratings {wire? | cable?, from?}` (read only): the results the panel shows, a page at a time.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RatingsParams {
    /// Only this panel wire: its conductor id or wire number.
    #[serde(default)]
    wire: Option<String>,
    /// Only this cable: its tag or id.
    #[serde(default)]
    cable: Option<String>,
    /// Skip this many (the `next` of the last call).
    #[serde(default)]
    from: Option<u64>,
}

impl Command for RatingsParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let from = usize::try_from(self.from.unwrap_or(0)).unwrap_or(usize::MAX);
        on_any_page(s, |p, _| {
            let doc = p.ledger.document();
            let subjects: Vec<RatedSubject> = if self.wire.is_some() || self.cable.is_some() {
                rules::subjects_of(doc, self.wire.as_deref(), self.cable.as_deref())?
            } else {
                let ctx = rating::Context::new(doc);
                rating::subjects(doc, &ctx)
            };
            let total = subjects.len();
            let mut ctx = rating::Context::new(doc);
            let out: Vec<Value> = subjects
                .iter()
                .skip(from)
                .take(PAGE)
                .map(|s| {
                    let found = rating::trace::trace_subject(doc, *s);
                    rating_json(doc, &rating::evaluate(doc, &mut ctx, *s, &found))
                })
                .collect();
            let next = (from.saturating_add(PAGE) < total).then_some(from.saturating_add(PAGE));
            Ok(json!({"ratings": out, "total": total, "next": next}))
        })
    }
}

/// One result as `ratings` returns it (design 057 §8.2): the object the panel shows.
pub fn rating_json(doc: &Document, r: &Rating) -> Value {
    let subject = match r.subject {
        RatedSubject::Wire(c) => json!({"wire": r.subject.words(doc), "conductor": c.hex()}),
        RatedSubject::Cable(k) => json!({"cable": r.subject.words(doc), "id": k.hex()}),
    };
    let factors: Vec<Value> = r.factors.iter().map(|f| json!({"name": f.name, "factor": f.factor, "input": f.input, "from": f.from})).collect();
    let device = r.device.as_ref().map(|d| json!({"tag": d.tag, "in": d.amps, "how": d.how.word()}));
    let load = r.load.map(|l| json!({"amps": l.amps, "from": if l.from == rating::load::LoadFrom::Typed { "typed" } else { "power" }}));
    let vd = r.voltdrop.as_ref().map(|v| {
        json!({"volts": v.volts, "percent": v.percent, "shown": format!("{} %", rating::fmt_percent(v.percent)), "amps": v.amps, "why": v.why, "limit": v.limit})
    });
    let kinds: Vec<&str> = rating::rows(doc, r).iter().map(|c| c.kind.name()).collect();
    json!({
        "subject": subject,
        "basis": r.basis.word(),
        "rated": r.rated.map(|(a, at)| json!({"amps": a, "at": at})),
        "table_value": r.table_value.map(|(a, size, m)| json!({"amps": a, "size": size, "method": m})),
        "table": r.table.as_ref().map(|(n, src)| json!({"name": n, "source": src})),
        "factors": factors,
        "iz": r.iz,
        "iz_shown": r.iz.map(|x| format!("{} A", rating::fmt_amps(x, r.device.as_ref().and_then(|d| d.amps)))),
        "summary": r.summary(),
        "device": device,
        "load": load,
        "voltdrop": vd,
        "size": r.size,
        "checks": kinds,
    })
}
