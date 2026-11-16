//! Reading a command's JSON parameters into its params struct (brief 070).
//!
//! Each command has a `#[derive(Deserialize, JsonSchema)] #[serde(deny_unknown_fields)]` struct
//! that implements [`Command`]; [`typed`] is its [`cadcraft_engine::Run`]. Fields are read through
//! the helpers here (wrapped by [`field!`] with the parameter's name), so a refusal says what the
//! hand-written readers said before; a missing required parameter says the command's own
//! sentence ([`Command::REQUIRED`]). The JSON Schema comes from the same struct ([`schema`]), so
//! the documentation an agent reads cannot drift from what the command accepts.

use cadcraft_engine::Session;
use cadcraft_engine::cmd::Run;
use ecad_electrical::CmdError;
use ecad_model::Pt;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::cmd::err;

/// A command with a typed input.
pub(crate) trait Command: DeserializeOwned + JsonSchema {
    /// What a missing required parameter says, by name. A required parameter not listed says
    /// serde's own "missing field" sentence.
    const REQUIRED: &'static [(&'static str, &'static str)] = &[];

    /// Run the command with its input read.
    ///
    /// # Errors
    /// The command's refusal, in words.
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value>;
}

/// The [`Run`] function of command `P`: read the parameters, then run it.
///
/// # Errors
/// Parameters that do not read ([`parse`]), or the command's own refusal.
pub(crate) fn typed<P: Command>(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let p = parse::<P>(params).map_err(|e| err(e.to_string()))?;
    p.run(s)
}

/// Read `params` as `P`. Anything but an object reads as no parameters (as `params.get` did).
///
/// # Errors
/// [`CmdError::Input`]: a mistyped, unknown or missing parameter, in words.
pub(crate) fn parse<P: Command>(params: &Value) -> Result<P, CmdError> {
    let empty = Value::Object(serde_json::Map::new());
    let v = if params.is_object() { params } else { &empty };
    P::deserialize(v).map_err(|e| {
        let text = e.to_string();
        let says = missing_field(&text).and_then(|f| P::REQUIRED.iter().find(|(k, _)| *k == f)).map(|(_, says)| *says);
        CmdError::input(says.map_or(text, str::to_string))
    })
}

/// The parameter serde's "missing field `x`" names.
fn missing_field(text: &str) -> Option<&str> {
    text.strip_prefix("missing field `")?.strip_suffix('`')
}

/// The JSON Schema of `P`'s parameters.
pub fn schema<P: JsonSchema>() -> Value {
    serde_json::to_value(schemars::schema_for!(P)).unwrap_or(Value::Null)
}

/// A command with a typed input, as registered: its id, its [`Run`] ([`typed`]) and its schema.
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

/// Wrap a helper of this module into a serde `deserialize_with` function, with the parameter's
/// name baked in: `field!(fn after -> Option<String> = |v| input::opt_string(v, "after"));`.
macro_rules! field {
    ($vis:vis fn $name:ident -> $ty:ty = $helper:expr) => {
        $vis fn $name<'de, D: serde::Deserializer<'de>>(d: D) -> Result<$ty, D::Error> {
            let v = <serde_json::Value as serde::Deserialize>::deserialize(d)?;
            let read: fn(serde_json::Value) -> Result<$ty, ecad_electrical::CmdError> = $helper;
            read(v).map_err(serde::de::Error::custom)
        }
    };
}
pub(crate) use field;

/// Text, or `null` for none.
///
/// # Errors
/// Anything else: "`key` must be a string".
pub(crate) fn opt_string(v: Value, key: &str) -> Result<Option<String>, CmdError> {
    match v {
        Value::Null => Ok(None),
        Value::String(s) => Ok(Some(s)),
        _ => Err(CmdError::input(format!("`{key}` must be a string"))),
    }
}

/// Text or `null` (`Some(None)`), for a parameter that must be given; the field defaults to
/// `None` when absent.
///
/// # Errors
/// Anything else: `says`.
pub(crate) fn nullable_string(v: Value, says: &'static str) -> Result<Option<Option<String>>, CmdError> {
    match v {
        Value::Null => Ok(Some(None)),
        Value::String(s) => Ok(Some(Some(s))),
        _ => Err(CmdError::input(says)),
    }
}

/// A hex ID, or `null` for none.
///
/// # Errors
/// Anything else: "`key` (hex id) is required".
pub(crate) fn opt_hex<T: From<u128>>(v: Value, key: &str) -> Result<Option<T>, CmdError> {
    match v {
        Value::Null => Ok(None),
        v => v
            .as_str()
            .and_then(|t| u128::from_str_radix(t.trim(), 16).ok())
            .map(|n| Some(T::from(n)))
            .ok_or_else(|| CmdError::input(format!("`{key}` (hex id) is required"))),
    }
}

/// A hex ID that must be given.
///
/// # Errors
/// Anything but a hex ID: "`key` (hex id) is required".
pub(crate) fn hex<T: From<u128>>(v: Value, key: &str) -> Result<T, CmdError> {
    opt_hex(v, key)?.ok_or_else(|| CmdError::input(format!("`{key}` (hex id) is required")))
}

/// A list of at most `max` hex IDs, or `null` for none. Every entry must read: a typo is an error,
/// never a smaller set.
///
/// # Errors
/// Not a list, too many entries, or an entry that is not a hex ID.
pub(crate) fn opt_hex_list<T: From<u128>>(v: Value, key: &str, max: usize) -> Result<Option<Vec<T>>, CmdError> {
    let a = match v {
        Value::Null => return Ok(None),
        Value::Array(a) => a,
        _ => return Err(CmdError::input(format!("`{key}` (array of hex ids) is required"))),
    };
    if a.len() > max {
        return Err(CmdError::input(format!("at most {max} entries in `{key}` per call")));
    }
    a.iter()
        .map(|v| {
            v.as_str()
                .and_then(|t| u128::from_str_radix(t.trim(), 16).ok())
                .map(T::from)
                .ok_or_else(|| CmdError::input(format!("`{v}` in `{key}` is not a valid hex id")))
        })
        .collect::<Result<Vec<T>, CmdError>>()
        .map(Some)
}

/// A count clamped to `1..=max`; `null` is `default`.
///
/// # Errors
/// Not a whole number: "`key` must be a number".
pub(crate) fn limit(v: Value, key: &str, default: usize, max: usize) -> Result<usize, CmdError> {
    match v {
        Value::Null => Ok(default),
        v => v.as_u64().map(|n| (n as usize).clamp(1, max)).ok_or_else(|| CmdError::input(format!("`{key}` must be a number"))),
    }
}

/// `true` or `false`, or `null` for none.
///
/// # Errors
/// Anything else: "`key` must be true or false".
pub(crate) fn opt_bool(v: Value, key: &str) -> Result<Option<bool>, CmdError> {
    match v {
        Value::Null => Ok(None),
        Value::Bool(b) => Ok(Some(b)),
        _ => Err(CmdError::input(format!("`{key}` must be true or false"))),
    }
}

/// A switch read leniently: anything but `true` or `false` is not given.
pub(crate) fn lenient_bool(v: Value) -> Result<Option<bool>, CmdError> {
    Ok(v.as_bool())
}

/// A whole number read leniently: anything else is not given.
pub(crate) fn lenient_u64(v: Value) -> Result<Option<u64>, CmdError> {
    Ok(v.as_u64())
}

/// A finite `[x, y]` point.
///
/// # Errors
/// Anything else: `says`.
pub(crate) fn point(v: Value, says: &'static str) -> Result<Pt, CmdError> {
    crate::cmd::point(&v).ok_or(CmdError::input(says))
}

/// A whole number, or `null` for none, checked by the reader of the value.
///
/// # Errors
/// Anything else: `says`.
pub(crate) fn opt_u64(v: Value, says: &'static str) -> Result<Option<u64>, CmdError> {
    match v {
        Value::Null => Ok(None),
        v => v.as_u64().map(Some).ok_or(CmdError::input(says)),
    }
}

/// A number, or `null` for none, checked by the reader of the value.
///
/// # Errors
/// Anything else: `says`.
pub(crate) fn opt_f64(v: Value, says: &'static str) -> Result<Option<f64>, CmdError> {
    match v {
        Value::Null => Ok(None),
        v => v.as_f64().map(Some).ok_or(CmdError::input(says)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_names_a_missing_field_as_this_module_reads_it() {
        #[derive(serde::Deserialize, JsonSchema)]
        struct P {
            #[allow(dead_code)]
            at: u8,
        }
        impl Command for P {
            const REQUIRED: &'static [(&'static str, &'static str)] = &[("at", "`at` [x,y] is required")];
            fn run(self, _: &mut Session) -> cadcraft_engine::Result<Value> {
                Ok(Value::Null)
            }
        }
        let e = parse::<P>(&serde_json::json!({})).err().map(|e| e.to_string());
        assert_eq!(e.as_deref(), Some("`at` [x,y] is required"));
        let e = parse::<P>(&Value::Null).err().map(|e| e.to_string());
        assert_eq!(e.as_deref(), Some("`at` [x,y] is required"));
    }
}
