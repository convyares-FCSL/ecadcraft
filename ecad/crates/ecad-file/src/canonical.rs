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
    // Rust's `{:e}` gives the shortest digits that round-trip: "d.ddde±x" or "de±x". When two
    // such digit strings are equally close, Rust rounds half up but ECMAScript picks the even one,
    // so the last digit is settled exactly by `closest_digits`.
    let sci = format!("{:e}", x.abs());
    let (mantissa, exp) = sci.split_once('e').unwrap_or((sci.as_str(), "0"));
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let exp: i64 = exp.parse().unwrap_or(0);
    let digits = closest_digits(x.abs(), &digits, exp);
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

/// ECMAScript's choice among the shortest digit strings (ECMA-262 Number::toString, step 5):
/// of the `k`-digit values that round-trip to `x`, the one closest to `x`, and the even one if two
/// are equally close. `digits` is Rust's shortest string for `x` (value `digits × 10^(exp-k+1)`);
/// the candidates are it and its neighbours in the last digit, compared exactly.
fn closest_digits(x: f64, digits: &str, exp: i64) -> String {
    let k = digits.len();
    let Ok(c) = digits.parse::<u64>() else { return digits.to_string() };
    if k == 0 || k > 17 {
        return digits.to_string();
    }
    let low = 10u64.pow(k as u32 - 1);
    let high = low.saturating_mul(10);
    let e10 = exp - (k as i64 - 1);
    let round_trips = |v: u64| format!("{v}e{e10}").parse::<f64>().is_ok_and(|y| y == x);
    let candidates: Vec<u64> = [c.checked_sub(1), Some(c), c.checked_add(1)]
        .into_iter()
        .flatten()
        .filter(|v| (low..high).contains(v))
        .filter(|v| round_trips(*v))
        .collect();
    let Some(mut best) = candidates.first().copied() else { return digits.to_string() };
    for next in candidates.iter().skip(1).copied() {
        // Compare 2x with best + next, both scaled by 10^e10: above the midpoint, `next` is closer.
        match compare_double_with_decimal(x, best + next, e10) {
            std::cmp::Ordering::Greater => best = next,
            std::cmp::Ordering::Equal => {
                if next % 2 == 0 {
                    best = next;
                }
                break;
            }
            std::cmp::Ordering::Less => break,
        }
    }
    let s = best.to_string();
    let trimmed = s.trim_end_matches('0');
    if trimmed.is_empty() { s } else { trimmed.to_string() }
}

/// Compare `2x` with `m × 10^e10` exactly (`x` finite and positive).
fn compare_double_with_decimal(x: f64, m: u64, e10: i64) -> std::cmp::Ordering {
    let bits = x.to_bits();
    let frac = bits & ((1u64 << 52) - 1);
    let biased = ((bits >> 52) & 0x7ff) as i64;
    let (mant, e2) = if biased == 0 { (frac, -1074) } else { (frac | (1u64 << 52), biased - 1075) };
    // 2x = mant × 2^(e2 + 1); m × 10^e10 = m × 2^e10 × 5^e10.
    let mut left = Big::from(mant);
    let mut right = Big::from(m);
    let p2 = e2 + 1 - e10;
    if p2 > 0 {
        left.mul_pow(2, p2)
    } else {
        right.mul_pow(2, -p2)
    }
    if e10 < 0 {
        left.mul_pow(5, -e10)
    } else {
        right.mul_pow(5, e10)
    }
    left.cmp(&right)
}

/// A minimal unsigned big integer (little-endian 32-bit limbs) for exact comparisons.
struct Big(Vec<u32>);

impl PartialEq for Big {
    fn eq(&self, other: &Self) -> bool {
        self.significant() == other.significant()
    }
}

impl Eq for Big {}

impl Big {
    fn from(v: u64) -> Self {
        Big(vec![v as u32, (v >> 32) as u32])
    }

    fn mul_small(&mut self, m: u32) {
        let mut carry = 0u64;
        for limb in &mut self.0 {
            let p = u64::from(*limb) * u64::from(m) + carry;
            *limb = p as u32;
            carry = p >> 32;
        }
        if carry > 0 {
            self.0.push(carry as u32);
        }
    }

    /// Multiply by `base^n` (`base` 2 or 5; `n` is bounded by the double range, capped anyway).
    fn mul_pow(&mut self, base: u32, n: i64) {
        let (chunk, step) = if base == 2 { (1u32 << 31, 31) } else { (5u32.pow(13), 13) };
        let mut n = n.clamp(0, 4096);
        while n >= step {
            self.mul_small(chunk);
            n -= step;
        }
        for _ in 0..n {
            self.mul_small(base);
        }
    }

    fn significant(&self) -> &[u32] {
        let len = self.0.iter().rposition(|l| *l != 0).map_or(0, |i| i + 1);
        self.0.get(..len).unwrap_or_default()
    }
}

impl PartialOrd for Big {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Big {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let (a, b) = (self.significant(), other.significant());
        a.len().cmp(&b.len()).then_with(|| a.iter().rev().cmp(b.iter().rev()))
    }
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
