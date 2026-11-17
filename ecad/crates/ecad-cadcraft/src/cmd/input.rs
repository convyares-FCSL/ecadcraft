//! Reading a command's JSON parameters into its params struct (brief 070).
//!
//! Each command has a `#[derive(Deserialize, JsonSchema)] #[serde(deny_unknown_fields)]` struct
//! that implements [`Command`]; [`typed`] is its run function. Fields are ordinary types: typed IDs
//! read their hex form themselves, [`Point`] reads `[x, y]`, [`Bounded`] a list of bounded length,
//! [`nullable`] tells `null` from absent and [`lenient`] keeps the few legacy switches that ignored
//! a wrong type.
//!
//! A parameter that does not read is refused in one uniform wording, built from its name and the
//! kind its schema gives ([`parse`]): "`conductor` (hex id) is required", "`at` must be a point
//! [x, y]", "`colour` is not a parameter of this command; it takes …". The JSON Schema comes from
//! the same struct ([`schema`]), so what an agent reads cannot drift from what the command takes.

use cadcraft_engine::Session;
use cadcraft_engine::cmd::Run;
use ecad_electrical::CmdError;
use ecad_model::Pt;
use schemars::JsonSchema;
use serde::de::{DeserializeOwned, Error as _};
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};

use crate::cmd::err;

/// A command with a typed input.
pub(crate) trait Command: DeserializeOwned + JsonSchema {
    /// Run the command with its input read.
    ///
    /// # Errors
    /// The command's refusal, in words.
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value>;
}

/// The run function of command `P`: read the parameters, then run it.
///
/// # Errors
/// Parameters that do not read ([`parse`]), or the command's own refusal.
pub(crate) fn typed<P: Command>(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let p = parse::<P>(params).map_err(|e| err(e.to_string()))?;
    p.run(s)
}

/// Read `params` as `P`. Anything but an object reads as no parameters.
///
/// # Errors
/// [`CmdError::Input`] naming the parameter and the kind it must be.
pub(crate) fn parse<P: Command>(params: &Value) -> Result<P, CmdError> {
    let empty = Map::new();
    let obj = params.as_object().unwrap_or(&empty);
    P::deserialize(&Value::Object(obj.clone())).map_err(|e| CmdError::input(explain::<P>(obj, &e.to_string())))
}

/// The marker a reader puts where the parameter's name goes ([`Bounded`]'s refusals).
const FIELD: &str = "{field}";

/// The uniform wording of serde's refusal `text` of `obj` as `P`.
fn explain<P: Command>(obj: &Map<String, Value>, text: &str) -> String {
    let schema = schema::<P>();
    if let Some(f) = quoted_after(text, "missing field `") {
        return format!("`{f}` ({}) is required", kind(&schema, f).0);
    }
    if let Some(f) = quoted_after(text, "unknown field `") {
        let takes: Vec<String> = schema["properties"].as_object().into_iter().flatten().map(|(k, _)| format!("`{k}`")).collect();
        return format!("`{f}` is not a parameter of this command; it takes {}", if takes.is_empty() { "none".into() } else { takes.join(", ") });
    }
    // The parameter at fault: the first whose removal changes the refusal (serde stops at the first
    // bad entry, before it looks for missing ones, so removing any other leaves the same text).
    let culprit = obj.keys().find(|k| {
        let mut rest = obj.clone();
        rest.remove(*k);
        match P::deserialize(&Value::Object(rest)) {
            Ok(_) => true,
            Err(e) => e.to_string() != text,
        }
    });
    match culprit {
        Some(k) if text.contains(FIELD) => text.replace(FIELD, &format!("`{k}`")),
        Some(k) => format!("`{k}` must be {}", kind(&schema, k).1),
        None => text.to_string(),
    }
}

/// The name in backquotes right after `prefix` in `text`.
fn quoted_after<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    text.strip_prefix(prefix)?.split('`').next()
}

/// What parameter `name` of `schema` is, as (a noun, a phrase): ("hex id", "a hex id").
fn kind(schema: &Value, name: &str) -> (String, String) {
    let field = resolve(schema, &schema["properties"][name]);
    let (k, nullable) = describe(schema, field);
    match nullable {
        true => (format!("{}, or null", k.0), format!("{}, or null", k.1)),
        false => k,
    }
}

/// Follow a `$ref` into the schema's `$defs`.
fn resolve<'a>(root: &'a Value, s: &'a Value) -> &'a Value {
    match s["$ref"].as_str().and_then(|r| r.strip_prefix("#/$defs/")) {
        Some(name) => &root["$defs"][name],
        None => s,
    }
}

/// The kind of sub-schema `s`, and whether it may be `null`.
fn describe(root: &Value, s: &Value) -> ((String, String), bool) {
    let types: Vec<&str> = match &s["type"] {
        Value::String(t) => vec![t.as_str()],
        Value::Array(a) => a.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    let mut nullable = types.contains(&"null");
    for key in ["anyOf", "oneOf"] {
        if let Some(alts) = s[key].as_array() {
            nullable |= alts.iter().any(|a| a["type"] == "null");
            let words: Vec<&Value> = alts.iter().filter(|a| a["type"] != "null").map(|a| resolve(root, a)).collect();
            let consts: Option<Vec<String>> = words.iter().map(|a| a.get("const").map(Value::to_string)).collect();
            if let Some(consts) = consts.filter(|c| !c.is_empty()) {
                let one = format!("one of {}", consts.join(", "));
                return ((one.clone(), one), nullable);
            }
            if let Some(a) = alts.iter().find(|a| a["type"] != "null") {
                return (describe(root, resolve(root, a)).0, nullable);
            }
        }
    }
    let pair = |n: &str, p: &str| (n.to_string(), p.to_string());
    let k = if let Some(words) = s["enum"].as_array() {
        let words: Vec<String> = words.iter().filter(|w| !w.is_null()).map(Value::to_string).collect();
        (format!("one of {}", words.join(", ")), format!("one of {}", words.join(", ")))
    } else if s["format"] == "hex-id" {
        pair("hex id", "a hex id")
    } else if s["format"] == "point" {
        pair("point [x, y]", "a point [x, y]")
    } else if types.contains(&"array") {
        let (item, _) = describe(root, resolve(root, &s["items"]));
        (format!("list of {}s", item.0), format!("a list of {}s", item.0))
    } else if types.contains(&"boolean") {
        pair("true or false", "true or false")
    } else if types.contains(&"integer") {
        pair("whole number", "a whole number")
    } else if types.contains(&"number") {
        pair("number", "a number")
    } else if types.contains(&"string") {
        pair("text", "text")
    } else if types.contains(&"object") {
        pair("object", "an object")
    } else {
        pair("value", "a value")
    };
    (k, nullable)
}

/// The JSON Schema of `P`'s parameters.
pub fn schema<P: JsonSchema>() -> Value {
    serde_json::to_value(schemars::schema_for!(P)).unwrap_or(Value::Null)
}

/// A command with a typed input, as registered: its id, its run function ([`typed`]) and its
/// schema.
#[derive(Clone, Copy)]
pub struct Typed {
    pub id: &'static str,
    pub run: Run,
    pub schema: fn() -> Value,
}

impl Typed {
    /// Command `id` reading its parameters as `P`.
    pub(crate) const fn of<P: Command>(id: &'static str) -> Typed {
        Typed { id, run: typed::<P>, schema: schema::<P> }
    }
}

/// A finite point written `[x, y]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point(pub Pt);

impl<'de> Deserialize<'de> for Point {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(d)?;
        crate::cmd::point(&v).map(Point).ok_or_else(|| D::Error::custom("invalid value: not a finite point"))
    }
}

impl JsonSchema for Point {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Point".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({"type": "array", "format": "point", "items": {"type": "number"}, "minItems": 2, "maxItems": 2})
    }
}

/// A list of `MIN` to `MAX` entries.
#[derive(Debug, Clone, PartialEq)]
pub struct Bounded<T, const MIN: usize, const MAX: usize>(pub Vec<T>);

impl<'de, T: Deserialize<'de>, const MIN: usize, const MAX: usize> Deserialize<'de> for Bounded<T, MIN, MAX> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Vec::<T>::deserialize(d)?;
        if v.len() > MAX {
            return Err(D::Error::custom(format!("{FIELD} takes at most {MAX} entries")));
        }
        if v.len() < MIN {
            return Err(D::Error::custom(format!("{FIELD} needs at least {MIN} entries")));
        }
        Ok(Bounded(v))
    }
}

impl<T: JsonSchema, const MIN: usize, const MAX: usize> JsonSchema for Bounded<T, MIN, MAX> {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> std::borrow::Cow<'static, str> {
        format!("Bounded_{}_{MIN}_{MAX}", T::schema_name()).into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let items = generator.subschema_for::<T>();
        schemars::json_schema!({"type": "array", "items": items, "minItems": MIN, "maxItems": MAX})
    }
}

/// A parameter written as text or as a number (a slot, a channel, a page number, a byte): the
/// rules read it as text, a number written as JSON writes it (`3` reads as `"3"`).
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum TextOrNumber {
    /// Text.
    Text(String),
    /// A number.
    Number(serde_json::Number),
}

impl TextOrNumber {
    /// The value as text: the text itself, or the number as JSON writes it.
    pub fn text(&self) -> String {
        match self {
            TextOrNumber::Text(t) => t.clone(),
            TextOrNumber::Number(n) => n.to_string(),
        }
    }
}

/// The schema of a parameter that must be given and may be `null`: `T`, or null. Use it as
/// `#[schemars(required, with = "OrNull<T>")]` on a [`nullable`] field (schemars drops the null of
/// a required `Option`).
pub struct OrNull<T>(std::marker::PhantomData<T>);

impl<T: JsonSchema> JsonSchema for OrNull<T> {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> std::borrow::Cow<'static, str> {
        format!("OrNull_{}", T::schema_name()).into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let inner = generator.subschema_for::<T>();
        schemars::json_schema!({"anyOf": [inner, {"type": "null"}]})
    }
}

/// `deserialize_with` for a parameter whose `null` means something (clear, release): the field
/// is `Option<Option<T>>` with `#[serde(default)]`, so absent is `None` and `null` is
/// `Some(None)`.
///
/// # Errors
/// A value that is not a `T`.
pub(crate) fn nullable<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(d).map(Some)
}

/// `deserialize_with` for a legacy switch that ignored anything but `true` or `false`.
///
/// # Errors
/// None: anything else reads as not given.
pub(crate) fn lenient<'de, D: Deserializer<'de>>(d: D) -> Result<Option<bool>, D::Error> {
    Ok(Value::deserialize(d)?.as_bool())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecad_model::ConductorId;
    use serde_json::json;

    #[derive(Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    #[allow(dead_code)]
    struct P {
        conductor: ConductorId,
        #[serde(default)]
        at: Option<Point>,
        #[serde(default)]
        many: Option<Bounded<ConductorId, 0, 2>>,
        #[serde(default)]
        on: Option<bool>,
        #[serde(default, deserialize_with = "nullable")]
        value: Option<Option<String>>,
    }

    impl Command for P {
        fn run(self, _: &mut Session) -> cadcraft_engine::Result<Value> {
            Ok(Value::Null)
        }
    }

    fn says(v: Value) -> Option<String> {
        parse::<P>(&v).err().map(|e| e.to_string())
    }

    #[test]
    fn input_refusals_are_uniform() {
        let rows = [
            (json!({}), "`conductor` (hex id) is required"),
            (Value::Null, "`conductor` (hex id) is required"),
            (json!({"conductor": 5}), "`conductor` must be a hex id"),
            (json!({"conductor": "zz"}), "`conductor` must be a hex id"),
            (json!({"conductor": "1", "at": "here"}), "`at` must be a point [x, y], or null"),
            (json!({"conductor": "1", "on": 1}), "`on` must be true or false, or null"),
            (json!({"conductor": "1", "value": 1}), "`value` must be text, or null"),
            (json!({"conductor": "1", "many": ["1", "2", "3"]}), "`many` takes at most 2 entries"),
            (json!({"conductor": "1", "many": ["x"]}), "`many` must be a list of hex ids, or null"),
            (
                json!({"conductor": "1", "colour": 1}),
                "`colour` is not a parameter of this command; it takes `at`, `conductor`, `many`, `on`, `value`",
            ),
        ];
        for (v, want) in rows {
            assert_eq!(says(v.clone()).as_deref(), Some(want), "{v}");
        }
    }

    #[test]
    fn text_or_number_reads_both_as_text() {
        #[derive(Deserialize, JsonSchema)]
        #[serde(deny_unknown_fields)]
        struct Q {
            #[serde(default)]
            slot: Option<TextOrNumber>,
        }
        impl Command for Q {
            fn run(self, _: &mut Session) -> cadcraft_engine::Result<Value> {
                Ok(Value::Null)
            }
        }
        let read = |v: Value| parse::<Q>(&v).map(|q| q.slot.map(|s| s.text())).map_err(|e| e.to_string());
        assert_eq!(read(json!({"slot": 3})), Ok(Some("3".into())));
        assert_eq!(read(json!({"slot": "03"})), Ok(Some("03".into())));
        assert_eq!(read(json!({"slot": 2.5})), Ok(Some("2.5".into())));
        assert_eq!(read(json!({"slot": null})), Ok(None));
        assert_eq!(read(json!({"slot": true})), Err("`slot` must be text, or null".into()));
    }

    #[test]
    fn null_and_absent_differ() {
        let p = parse::<P>(&json!({"conductor": "1", "value": null})).ok().map(|p| p.value);
        assert_eq!(p, Some(Some(None)));
        let p = parse::<P>(&json!({"conductor": "1"})).ok().map(|p| p.value);
        assert_eq!(p, Some(None));
    }
}
