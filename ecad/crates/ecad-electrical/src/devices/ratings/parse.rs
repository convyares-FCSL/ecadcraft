//! Reading the numbers a rating rests on. Every input is hostile: a
//! parser reads at most [`MAX_CHARS`] characters (the device rating [`MAX_RATING_CHARS`]), never
//! indexes, and returns `None` for anything not finite or out of range.

/// Most characters a value parser reads.
pub(crate) const MAX_CHARS: usize = 32;
/// Most characters of a device's `rating` text scanned for its current.
pub(crate) const MAX_RATING_CHARS: usize = 64;
/// The largest current anything here accepts, in amperes.
pub const MAX_AMPS: f64 = 6300.0;

/// The text, trimmed, when it is at most `max` characters; else `None` (a longer value is never
/// read, so it cannot make a parser slow).
fn bounded(text: &str, max: usize) -> Option<&str> {
    let t = text.trim();
    (t.chars().nth(max).is_none()).then_some(t)
}

/// A plain decimal number: digits with one `.` or `,` (`3,2` is 3.2), an optional leading `-`.
/// No exponent, no `inf`/`nan`, no `+`.
pub(crate) fn decimal(t: &str) -> Option<f64> {
    let t = t.trim();
    if t.is_empty() {
        return None;
    }
    let body = t.strip_prefix('-').unwrap_or(t);
    let mut sep = 0;
    let mut digits = 0;
    for c in body.chars() {
        match c {
            '0'..='9' => digits += 1,
            '.' | ',' => sep += 1,
            _ => return None,
        }
    }
    if digits == 0 || sep > 1 {
        return None;
    }
    let x: f64 = t.replace(',', ".").parse().ok()?;
    x.is_finite().then_some(x)
}

/// `text` split into lower-case pieces at white space, `,` and `;`. A comma between digits is a
/// decimal comma (`0,5 kW` reads as `0.5 kW`), not a separator. Every rating reader reads these
/// pieces, so a number reads alike for every kind of device.
pub(crate) fn pieces(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let text: String = chars
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let digit = |j: Option<usize>| j.and_then(|j| chars.get(j)).is_some_and(char::is_ascii_digit);
            if *c == ',' && digit(i.checked_sub(1)) && digit(Some(i + 1)) { '.' } else { *c }
        })
        .collect();
    text.split(|c: char| c.is_whitespace() || c == ',' || c == ';').filter(|p| !p.is_empty()).map(str::to_lowercase).collect()
}

/// A lower-case piece read as a number and the unit run on after it (`7.5kw` gives 7.5 and `kw`,
/// `24` gives 24 and no unit); `None` when it does not start with a [`decimal`] number.
pub(crate) fn number_unit(lower: &str) -> Option<(f64, &str)> {
    let split = lower.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(lower.len());
    let (num, unit) = lower.split_at(split);
    Some((decimal(num)?, unit))
}

/// `text` without the unit `unit` at its end (case-insensitive, spaces allowed before it).
fn strip_unit<'a>(text: &'a str, units: &[&str]) -> &'a str {
    let t = text.trim_end();
    for u in units {
        let n = u.len();
        if t.len() >= n
            && let Some(tail) = t.get(t.len() - n..)
            && tail.eq_ignore_ascii_case(u)
            && let Some(head) = t.get(..t.len() - n)
        {
            return head.trim_end();
        }
    }
    t
}

/// A current typed by a person (`load`, `rating.manual`, `rated_current`): `3.2`, `3.2 A`,
/// `3,2 A`; finite, 0 < x ≤ 6300.
pub fn current(text: &str) -> Option<f64> {
    let t = bounded(text, MAX_CHARS)?;
    decimal(strip_unit(t, &["A"])).filter(|x| *x > 0.0 && *x <= MAX_AMPS)
}

/// A length in metres (a `length`): `12`, `12 m`, `12.5m`, `1200 mm`; 0 < x ≤ 10 000 m.
pub fn length_m(text: &str) -> Option<f64> {
    let t = bounded(text, MAX_CHARS)?;
    let mm = strip_unit(t, &["mm"]);
    let x = if mm.len() != t.len() { decimal(mm)? / 1000.0 } else { decimal(strip_unit(t, &["m"]))? };
    (x > 0.0 && x <= 10_000.0).then_some(x)
}

/// A cross-section in mm²: `2.5`, `2.5 mm²`, `2.5mm2`; 0.05–1000. `18 AWG` and others: `None`.
pub fn cross_section(text: &str) -> Option<f64> {
    let t = bounded(text, MAX_CHARS)?;
    decimal(strip_unit(t, &["mm\u{b2}", "mm2", "mm^2", "mm"])).filter(|x| (0.05..=1000.0).contains(x))
}

/// A temperature in °C: `35`, `35 °C`, `35C`, `35°C`, within `min..=max`.
pub fn temperature(text: &str, min: f64, max: f64) -> Option<f64> {
    let t = bounded(text, MAX_CHARS)?;
    decimal(strip_unit(t, &["\u{b0}C", "C", "\u{b0}"])).filter(|x| (min..=max).contains(x))
}

/// An ambient temperature: −40 to 90 °C.
pub fn ambient(text: &str) -> Option<f64> {
    temperature(text, -40.0, 90.0)
}

use ecad_model::objects::wire::Bundling;

/// Read a bundling input: a factor 0.30–1.00, or a circuit count 1–20 written
/// `3 bunched` (or `3 circuits`); the stored type is the model's [`Bundling`].
pub fn bundling(text: &str) -> Option<Bundling> {
    let t = bounded(text, MAX_CHARS)?;
    let lower = t.to_ascii_lowercase();
    for word in ["bunched", "circuits", "circuit"] {
        if let Some(head) = lower.strip_suffix(word) {
            return count(head).and_then(|n| u8::try_from(n).ok()).map(Bundling::Circuits);
        }
    }
    decimal(t).filter(|x| (0.30..=1.00).contains(x)).map(Bundling::Factor)
}

/// A circuit count 1–20 (`rating.grouped`).
pub fn count(text: &str) -> Option<u32> {
    let t = bounded(text, MAX_CHARS)?;
    if !ecad_model::naming::words::is_digits(t) {
        return None;
    }
    t.parse::<u32>().ok().filter(|n| (1..=20).contains(n))
}

/// A supply voltage: `400`, `400 V`; 1–1000 V.
pub fn voltage(text: &str) -> Option<f64> {
    let t = bounded(text, MAX_CHARS)?;
    decimal(strip_unit(t, &["V"])).filter(|x| (1.0..=1000.0).contains(x))
}

/// A volt-drop limit: `5`, `5 %`; 0.5–20 %.
pub fn percent(text: &str) -> Option<f64> {
    let t = bounded(text, MAX_CHARS)?;
    decimal(strip_unit(t, &["%"])).filter(|x| (0.5..=20.0).contains(x))
}

/// The rated current a protective device's free `rating` text gives: the first
/// number directly followed by `A` (spaces allowed; not `mA` or `kA`), else a curve letter
/// `B C D K Z` directly followed by a number (`C16`), else `None`. A bare number is not read.
/// Finite, > 0 and ≤ 6300 A. At most the first 64 characters are scanned.
pub fn amps(text: &str) -> Option<f64> {
    let c: Vec<char> = text.chars().take(MAX_RATING_CHARS).collect();
    let is_num = |ch: char| ch.is_ascii_digit() || ch == '.' || ch == ',';
    let ok = |x: f64| (x.is_finite() && x > 0.0 && x <= MAX_AMPS).then_some(x);
    // 1. A number followed by `A`.
    let mut i = 0;
    while i < c.len() {
        let starts = c.get(i).is_some_and(char::is_ascii_digit) && (i == 0 || !c.get(i - 1).is_some_and(|p| is_num(*p) || p.is_ascii_alphabetic()));
        if !starts {
            i += 1;
            continue;
        }
        let mut j = i;
        while c.get(j).is_some_and(|ch| is_num(*ch)) {
            j += 1;
        }
        let num: String = c.get(i..j).map(|s| s.iter().collect()).unwrap_or_default();
        let neg = i > 0 && c.get(i - 1) == Some(&'-');
        let mut k = j;
        while c.get(k) == Some(&' ') {
            k += 1;
        }
        let unit = c.get(k).copied();
        let after = c.get(k + 1).copied();
        if matches!(unit, Some('A' | 'a')) && !after.is_some_and(|a| a.is_ascii_alphanumeric()) {
            if neg {
                return None;
            }
            return decimal(&num).and_then(ok);
        }
        i = j.max(i + 1);
    }
    // 2. A curve letter followed by a number (`C16`).
    let mut i = 0;
    while i < c.len() {
        let letter = c.get(i).is_some_and(|ch| matches!(ch.to_ascii_uppercase(), 'B' | 'C' | 'D' | 'K' | 'Z'));
        let free = i == 0 || !c.get(i - 1).is_some_and(|p| p.is_ascii_alphanumeric());
        if letter && free && c.get(i + 1).is_some_and(char::is_ascii_digit) {
            let mut j = i + 1;
            while c.get(j).is_some_and(|ch| is_num(*ch)) {
                j += 1;
            }
            if c.get(j).is_some_and(|ch| ch.is_ascii_alphabetic()) {
                i = j;
                continue;
            }
            let num: String = c.get(i + 1..j).map(|s| s.iter().collect()).unwrap_or_default();
            return decimal(&num).and_then(ok);
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_ratings_read_as_the_design_says() {
        for (t, want) in [
            ("4A C", Some(4.0)),
            ("C16", Some(16.0)),
            ("13A C 30mA", Some(13.0)),
            ("6.3 A T", Some(6.3)),
            ("4A", Some(4.0)),
            ("20 A", Some(20.0)),
            ("2 A", Some(2.0)),
        ] {
            assert_eq!(amps(t), want, "{t}");
        }
        for t in ["16", "K", "0 A", "-4 A", "1e9 A", "NaN A", "30mA", "10kA", "", "6300.1 A"] {
            assert_eq!(amps(t), None, "{t}");
        }
        assert_eq!(amps(&"9".repeat(10_000)), None);
        assert_eq!(amps(&format!("{}4A", " ".repeat(70))), None);
    }

    #[test]
    fn hostile_values_are_refused() {
        for t in ["0", "-3", "NaN", "1e400", "inf", "6301", "3.2.1", "", "abc", &"1".repeat(40)] {
            assert_eq!(current(t), None, "{t}");
        }
        assert_eq!(current("3,2 A"), Some(3.2));
        assert_eq!(length_m("1200 mm"), Some(1.2));
        assert_eq!(length_m("12.5m"), Some(12.5));
        assert_eq!(length_m("10001"), None);
        assert_eq!(cross_section("2.5 mm\u{b2}"), Some(2.5));
        assert_eq!(cross_section("2.5mm2"), Some(2.5));
        assert_eq!(cross_section("18 AWG"), None);
        assert_eq!(ambient("35 \u{b0}C"), Some(35.0));
        assert_eq!(ambient("95"), None);
        assert_eq!(bundling("3 bunched"), Some(Bundling::Circuits(3)));
        assert_eq!(bundling("0.8"), Some(Bundling::Factor(0.8)));
        assert_eq!(bundling("0.2"), None);
        assert_eq!(bundling("21 bunched"), None);
        assert_eq!(voltage("400 V"), Some(400.0));
        assert_eq!(percent("5 %"), Some(5.0));
        assert_eq!(percent("25"), None);
    }
}
