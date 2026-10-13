//! The canonical form of a record (proposal §20.10): RFC 8785, the JSON Canonicalization Scheme.
//!
//! Used for hashing, and as the exact bytes the JSON Lines codec writes, so a record written by
//! this crate is already canonical. Independent of `serde_json`'s map ordering (which a feature
//! elsewhere in a build could change): keys are sorted here, by UTF-16 code units.

use serde_json::Value;

/// The largest integer a double holds exactly together with all smaller ones: 2^53 − 1.
pub const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;

/// Why a value has no canonical form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalError {
    /// NaN or ±∞ (cannot occur in parsed JSON, but can in values built in memory).
    NonFinite,
    /// An integer that a double cannot hold exactly.
    InexactInteger(String),
}

impl std::fmt::Display for CanonicalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CanonicalError::NonFinite => f.write_str("a number is not finite"),
            CanonicalError::InexactInteger(n) => write!(f, "integer {n} is not exactly representable as a double"),
        }
    }
}

/// The canonical bytes of `v`.
///
/// # Errors
/// [`CanonicalError`] for numbers RFC 8785 cannot represent exactly.
pub fn canonical(v: &Value) -> Result<Vec<u8>, CanonicalError> {
    let mut out = Vec::new();
    write_value(v, &mut out)?;
    Ok(out)
}

fn write_value(v: &Value, out: &mut Vec<u8>) -> Result<(), CanonicalError> {
    match v {
        Value::Null => out.extend_from_slice(b"null"),
        Value::Bool(b) => out.extend_from_slice(if *b { b"true" } else { b"false" }),
        Value::Number(n) => out.extend_from_slice(number(n)?.as_bytes()),
        Value::String(s) => write_string(s, out),
        Value::Array(a) => {
            out.push(b'[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                write_value(x, out)?;
            }
            out.push(b']');
        }
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            out.push(b'{');
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                write_string(k, out);
                out.push(b':');
                if let Some(x) = m.get(*k) {
                    write_value(x, out)?;
                }
            }
            out.push(b'}');
        }
    }
    Ok(())
}

fn number(n: &serde_json::Number) -> Result<String, CanonicalError> {
    if let Some(u) = n.as_u64() {
        return integer(u, false, n);
    }
    if let Some(i) = n.as_i64() {
        return integer(i.unsigned_abs(), i < 0, n);
    }
    es_number(n.as_f64().ok_or(CanonicalError::NonFinite)?)
}

/// An integer JSON number: written as the double it denotes, which must hold it exactly.
fn integer(magnitude: u64, negative: bool, n: &serde_json::Number) -> Result<String, CanonicalError> {
    let d = magnitude as f64;
    if d as u64 != magnitude || d >= 18_446_744_073_709_551_616.0 {
        return Err(CanonicalError::InexactInteger(n.to_string()));
    }
    es_number(if negative { -d } else { d })
}

/// A double as ECMAScript `Number.prototype.toString` writes it (RFC 8785 §3.2.2.3): shortest
/// round-trip digits, plain notation for 1e-6 ≤ |x| < 1e21, exponent notation otherwise, and
/// `-0` as `0`.
///
/// # Errors
/// [`CanonicalError::NonFinite`] for NaN and ±∞.
///
/// # Example
/// ```rust
/// use ecad_file::canonical::es_number;
/// assert_eq!(es_number(1e21).unwrap(), "1e+21");
/// assert_eq!(es_number(1e-7).unwrap(), "1e-7");
/// assert_eq!(es_number(-0.0).unwrap(), "0");
/// ```
pub fn es_number(x: f64) -> Result<String, CanonicalError> {
    if !x.is_finite() {
        return Err(CanonicalError::NonFinite);
    }
    if x == 0.0 {
        return Ok("0".into());
    }
    // Rust's `{:e}` gives the shortest digits that round-trip: "d.ddde±x" or "de±x".
    let sci = format!("{:e}", x.abs());
    let (mantissa, exp) = sci.split_once('e').unwrap_or((sci.as_str(), "0"));
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let exp: i64 = exp.parse().unwrap_or(0);
    let k = digits.len() as i64;
    let n = exp + 1; // value = 0.d1d2…dk × 10^n
    let mut s = String::new();
    if x < 0.0 {
        s.push('-');
    }
    if k <= n && n <= 21 {
        s.push_str(&digits);
        s.extend(std::iter::repeat_n('0', usize::try_from(n - k).unwrap_or(0)));
    } else if 0 < n && n <= 21 {
        let split = usize::try_from(n).unwrap_or(0);
        s.push_str(digits.get(..split).unwrap_or_default());
        s.push('.');
        s.push_str(digits.get(split..).unwrap_or_default());
    } else if -6 < n && n <= 0 {
        s.push_str("0.");
        s.extend(std::iter::repeat_n('0', usize::try_from(-n).unwrap_or(0)));
        s.push_str(&digits);
    } else {
        let e = n - 1;
        s.push_str(digits.get(..1).unwrap_or_default());
        if k > 1 {
            s.push('.');
            s.push_str(digits.get(1..).unwrap_or_default());
        }
        s.push('e');
        s.push(if e >= 0 { '+' } else { '-' });
        s.push_str(&e.abs().to_string());
    }
    Ok(s)
}

fn write_string(s: &str, out: &mut Vec<u8>) {
    out.push(b'"');
    for c in s.chars() {
        match c {
            '"' => out.extend_from_slice(b"\\\""),
            '\\' => out.extend_from_slice(b"\\\\"),
            '\u{08}' => out.extend_from_slice(b"\\b"),
            '\u{09}' => out.extend_from_slice(b"\\t"),
            '\u{0A}' => out.extend_from_slice(b"\\n"),
            '\u{0C}' => out.extend_from_slice(b"\\f"),
            '\u{0D}' => out.extend_from_slice(b"\\r"),
            c if (c as u32) < 0x20 => out.extend_from_slice(format!("\\u{:04x}", c as u32).as_bytes()),
            c => {
                let mut buf = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
        }
    }
    out.push(b'"');
}
