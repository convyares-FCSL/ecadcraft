//! A cable's cores (design 028 §6): the core offer, its notation (`7G1.5`), and the used, spare and
//! over count of its members.

use std::collections::BTreeSet;

use ecad_model::{CableId, Core, Document};

use super::*;
use crate::CmdError;

fn cores_refusal(text: &str) -> CmdError {
    let shown: String = text.chars().take(24).collect();
    let more = if text.chars().count() > 24 { "\u{2026}" } else { "" };
    CmdError::refused(format!("\"{shown}{more}\" is not a core list. Write 7, 7G, 5X, 4x2x0.25 (pairs), or BN,BK,GY,BU,GNYE."))
}

/// A core count as a cable type writes it (design 028 §6.1, owner decisions 2026-11-12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Notation {
    /// Cores, or pairs or triples when `group` is 2 or 3.
    pub n: usize,
    /// `G` (one of the n is green-yellow): `Some(true)`; `X` (none): `Some(false)`; not stated:
    /// `None` (no green-yellow expected).
    pub pe: Option<bool>,
    /// 1, or 2 for pairs and 3 for triples.
    pub group: usize,
}

impl Notation {
    /// How many cores.
    pub fn cores(self) -> usize {
        self.n.saturating_mul(self.group)
    }

    /// The cores' labels: `1`…`n` (`G`: the last is GNYE), or `1a`, `1b`, `2a`… in pairs.
    pub fn labels(self) -> Vec<String> {
        if self.group > 1 {
            let letters = ['a', 'b', 'c'];
            return (1..=self.n).flat_map(|i| letters.iter().take(self.group).map(move |l| format!("{i}{l}"))).collect();
        }
        match self.pe {
            Some(true) => (1..self.n).map(|i| i.to_string()).chain(std::iter::once(GNYE.to_string())).collect(),
            _ => (1..=self.n).map(|i| i.to_string()).collect(),
        }
    }

    /// As shown: `7G`, `7X`, `3`, `4x2`.
    pub fn text(self) -> String {
        match (self.group, self.pe) {
            (g, _) if g > 1 => format!("{}x{g}", self.n),
            (_, Some(true)) => format!("{}G", self.n),
            (_, Some(false)) => format!("{}X", self.n),
            (_, None) => self.n.to_string(),
        }
    }
}

/// Most characters of a type the notation scan reads.
const MAX_TYPE_CHARS: usize = 256;

fn is_times(c: char) -> bool {
    matches!(c, 'x' | 'X' | '\u{d7}')
}

/// The digits at `i` as a number (capped) and the index after them.
fn digits_at(c: &[char], i: usize) -> Option<(usize, usize)> {
    let mut j = i;
    let mut n: usize = 0;
    while let Some(d) = c.get(j).and_then(|ch| ch.to_digit(10)) {
        n = n.saturating_mul(10).saturating_add(d as usize).min(1_000_000);
        j += 1;
    }
    (j > i).then_some((n, j))
}

fn skip_spaces(c: &[char], mut i: usize) -> usize {
    while c.get(i).is_some_and(|ch| *ch == ' ') {
        i += 1;
    }
    i
}

/// The first core count `text` writes (design 028 §6.1): `nGs` / `nXs` (G: green-yellow among
/// the n; X: none); `n x s` with a lower-case multiplier (n cores, green-yellow not stated); pairs
/// and triples `n(2xs)`, `nx2xs`, `n x 2 x s` (`3` for triples). Words such as `C`, `CY` and
/// `(TP)` hold no count and are passed over; `4(2x0.25)` is 4 pairs, never 2 cores.
pub fn notation(text: &str) -> Option<Notation> {
    let c: Vec<char> = text.chars().take(MAX_TYPE_CHARS).collect();
    let digit = |i: usize| c.get(i).is_some_and(char::is_ascii_digit);
    let mut i = 0;
    while i < c.len() {
        let starts = digit(i) && (i == 0 || !c.get(i - 1).is_some_and(|p| p.is_ascii_digit() || *p == '.' || *p == ','));
        if !starts {
            i += 1;
            continue;
        }
        let Some((n, j)) = digits_at(&c, i) else { break };
        let ok_n = (1..=MAX_CORES).contains(&n);
        let k = skip_spaces(&c, j);
        match c.get(k) {
            // n(2xs): pairs or triples.
            Some('(') if ok_n => {
                let k2 = skip_spaces(&c, k + 1);
                if let Some((g, k3)) = digits_at(&c, k2) {
                    let k4 = skip_spaces(&c, k3);
                    if (2..=3).contains(&g) && c.get(k4).is_some_and(|x| is_times(*x)) && digit(skip_spaces(&c, k4 + 1)) {
                        return Some(Notation { n, pe: None, group: g });
                    }
                }
            }
            Some(x) if ok_n && is_times(*x) => {
                let k2 = skip_spaces(&c, k + 1);
                if let Some((m, k3)) = digits_at(&c, k2) {
                    // n x 2 x s: pairs or triples.
                    let k4 = skip_spaces(&c, k3);
                    if (2..=3).contains(&m) && c.get(k4).is_some_and(|y| is_times(*y)) && digit(skip_spaces(&c, k4 + 1)) {
                        return Some(Notation { n, pe: None, group: m });
                    }
                    // nXs (an upper-case X straight after the count): no green-yellow; n x s: not stated.
                    let pe = (*x == 'X' && k == j).then_some(false);
                    return Some(Notation { n, pe, group: 1 });
                }
            }
            Some('G' | 'g') if ok_n && k == j && digit(k + 1) => return Some(Notation { n, pe: Some(true), group: 1 }),
            _ => {}
        }
        i = j;
    }
    None
}

/// The core count a cable type names (`YSLY-JZ 7G1.5` → 7 with green-yellow): [`notation`],
/// with `-JZ` meaning G and `-OZ` meaning X when the count itself does not say.
pub fn type_notation(kind: &str) -> Option<Notation> {
    let mut n = notation(kind)?;
    if n.group == 1 && n.pe.is_none() {
        let upper = kind.to_ascii_uppercase();
        if upper.contains("-JZ") {
            n.pe = Some(true);
        } else if upper.contains("-OZ") {
            n.pe = Some(false);
        }
    }
    (n.cores() <= MAX_CORES).then_some(n)
}

/// The cross-section a type's notation names, in mm² (brief 057, design 057 §2.2): the number
/// after the count's `G`, `X` or multiplier (`H07RN-F 4G1.5` → 1.5, `3x2.5` → 2.5, `4x2x0.25` →
/// 0.25); `None` without a notation or a readable size. [`Notation`] is unchanged.
pub fn notation_size(kind: &str) -> Option<f64> {
    let c: Vec<char> = kind.chars().take(MAX_TYPE_CHARS).collect();
    let mut i = 0;
    while i < c.len() {
        let starts = c.get(i).is_some_and(char::is_ascii_digit)
            && (i == 0 || !c.get(i - 1).is_some_and(|p| p.is_ascii_alphanumeric() || *p == '.' || *p == ','));
        if !starts {
            i += 1;
            continue;
        }
        let Some((_, mut j)) = digits_at(&c, i) else { break };
        let mut seen = false;
        // One or two `G`/`X`/`x` steps (`4G1.5`, `4x2x0.25`, `4(2x0.25)`), then the size.
        loop {
            let k = skip_spaces(&c, j);
            let k = if c.get(k) == Some(&'(') { skip_spaces(&c, k + 1) } else { k };
            match c.get(k) {
                Some(x) if is_times(*x) || matches!(x, 'G' | 'g') => {
                    let k2 = skip_spaces(&c, k + 1);
                    let mut e = k2;
                    while c.get(e).is_some_and(|ch| ch.is_ascii_digit() || *ch == '.' || *ch == ',') {
                        e += 1;
                    }
                    if e == k2 {
                        break;
                    }
                    seen = true;
                    let s: String = c.get(k2..e).map(|v| v.iter().collect()).unwrap_or_default();
                    let k3 = skip_spaces(&c, e);
                    let more = c.get(k3).is_some_and(|x| is_times(*x));
                    if more {
                        j = e;
                        continue;
                    }
                    return crate::devices::ratings::parse::cross_section(&s);
                }
                _ => break,
            }
        }
        let _ = seen;
        i = j.max(i + 1);
    }
    None
}

/// Read a core offer (§6.1): `7`, `7G`, `5X`, a type's notation (`4x2x0.25`, `4(2x0.25)`,
/// `3x1.5`, `12G0.5`), or a list `BN,BK,GY,BU,GNYE`.
///
/// # Errors
/// Unreadable text, no cores, more than [`MAX_CORES`], an empty or repeated list entry.
pub fn parse_cores(text: &str) -> Result<Vec<String>, CmdError> {
    let t = text.trim();
    if t.chars().count() > MAX_CORES * (crate::cables::words::MAX_CORE_CHARS + 1) {
        return Err(CmdError::refused(format!("A cable offers at most {MAX_CORES} cores.")));
    }
    let upper = t.to_ascii_uppercase();
    let (digits, letter) = match upper.char_indices().last() {
        Some((i, c @ ('G' | 'X'))) => (upper.get(..i).unwrap_or(""), Some(c)),
        _ => (upper.as_str(), None),
    };
    if ecad_model::naming::words::is_digits(digits) {
        let n: usize = digits.parse().map_err(|_| CmdError::refused(format!("A cable offers at most {MAX_CORES} cores.")))?;
        if n == 0 {
            return Err(CmdError::refused("A cable offers at least 1 core. Write 7, 7G or 5X; empty clears it."));
        }
        if n > MAX_CORES {
            return Err(CmdError::refused(format!("A cable offers at most {MAX_CORES} cores.")));
        }
        let pe = match letter {
            Some('G') => Some(true),
            Some('X') => Some(false),
            _ => None,
        };
        return Ok(Notation { n, pe, group: 1 }.labels());
    }
    if !t.contains(',') && t.chars().any(|c| c.is_ascii_digit()) {
        let n = type_notation(t).ok_or_else(|| cores_refusal(text))?;
        return Ok(n.labels());
    }
    let mut out: Vec<String> = Vec::new();
    for part in t.split(',') {
        let label = crate::terminal_strips::core_label(Some(part))
            .map_err(|e| CmdError::refused(format!("{e} {}", cores_refusal(text))))?
            .ok_or_else(|| cores_refusal(text))?;
        if out.iter().any(|x| x.eq_ignore_ascii_case(&label)) {
            return Err(CmdError::refused(format!("The core list names {label} twice. Write each core once.")));
        }
        out.push(label);
        if out.len() > MAX_CORES {
            return Err(CmdError::refused(format!("A cable offers at most {MAX_CORES} cores.")));
        }
    }
    Ok(out)
}

/// The `cores` value as stored (counts upper-case, lists trimmed, notations as written); empty
/// clears it.
///
/// # Errors
/// [`parse_cores`]'s.
pub fn cores_value(value: Option<&str>) -> Result<Option<String>, CmdError> {
    let Some(v) = value.map(str::trim).filter(|v| !v.is_empty()) else { return Ok(None) };
    let cores = parse_cores(v)?;
    let upper = v.to_ascii_uppercase();
    if upper.bytes().all(|b| matches!(b, b'0'..=b'9' | b'G' | b'X')) {
        return Ok(Some(upper));
    }
    if !v.contains(',') && v.chars().any(|c| c.is_ascii_digit()) {
        return Ok(Some(v.to_string()));
    }
    Ok(Some(cores.join(",")))
}

/// The core count a cable type names, as shown (`YSLY-JZ 7G1.5` → `7G`, `4x2x0.25` → `4x2`).
pub fn offer_from_type(kind: &str) -> Option<String> {
    type_notation(kind).map(Notation::text)
}

/// Where a cable's offer comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfferSource {
    Cores,
    Type,
}

impl OfferSource {
    pub fn word(self) -> &'static str {
        match self {
            OfferSource::Cores => "cores",
            OfferSource::Type => "type",
        }
    }
}

/// A cable's core offer and where it comes from; `None` when unknown.
pub fn cable_offer(props: &ecad_model::Properties) -> Option<(Vec<String>, OfferSource, String)> {
    if let Some(v) = props.get(CORES_KEY).map(|v| v.trim()).filter(|v| !v.is_empty())
        && let Ok(c) = parse_cores(v)
    {
        return Some((c, OfferSource::Cores, v.to_string()));
    }
    let n = type_notation(props.get("type")?)?;
    Some((n.labels(), OfferSource::Type, n.text()))
}

/// How a cable's members meet its offer (§6.2).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CoreCount {
    /// The offer, when known.
    pub offer: Option<Vec<String>>,
    pub source: Option<OfferSource>,
    /// The offer as written (`7G`), for words.
    pub offer_text: Option<String>,
    /// Per member: the offered core it is counted as (`None`: over, or no offer).
    pub matched: Vec<Option<String>>,
    /// Offered cores no member uses, in offer order.
    pub spare: Vec<String>,
    /// Members beyond the offer (member indexes).
    pub over: Vec<usize>,
    /// Members whose stored label an earlier member already has (member indexes).
    pub twice: Vec<usize>,
}

impl CoreCount {
    /// Used: the members counted against the offer (every member when the offer is unknown).
    pub fn used(&self) -> usize {
        match &self.offer {
            Some(_) => self.matched.iter().filter(|m| m.is_some()).count(),
            None => self.matched.len(),
        }
    }

    /// `2 (6, GNYE)`, `0`, or `None` when the offer is unknown.
    pub fn spare_text(&self) -> Option<String> {
        self.offer.as_ref()?;
        Some(if self.spare.is_empty() { "0".into() } else { format!("{} ({})", self.spare.len(), self.spare.join(", ")) })
    }

    /// The core a new member takes: the first spare one.
    pub fn next_free(&self) -> Option<String> {
        self.spare.first().cloned()
    }
}

/// Count a cable's stored cores against `offer` (§6.2, design 072 §4.5: read as stored, never
/// matched by order). A placed core counts as the offered core of its label; a placed core with
/// no label, a label the offer lacks, or a label an earlier core has is over. The spare cores are
/// the stored cores on no line, in core order.
pub fn count_cores(offer: Option<(Vec<String>, OfferSource, String)>, members: &[Core]) -> CoreCount {
    let placed = |m: &Core| m.position != ecad_model::CorePosition::Unplaced;
    let label = |m: &Core| m.label_str().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string);
    let mut twice = Vec::new();
    let mut labels: BTreeSet<String> = BTreeSet::new();
    for (i, m) in members.iter().enumerate().filter(|(_, m)| placed(m)) {
        if let Some(l) = label(m)
            && !labels.insert(l.to_ascii_uppercase())
        {
            twice.push(i);
        }
    }
    let Some((offer, source, text)) = offer else {
        return CoreCount { matched: vec![None; members.len()], twice, ..CoreCount::default() };
    };
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut matched: Vec<Option<String>> = vec![None; members.len()];
    let mut over = Vec::new();
    for (i, m) in members.iter().enumerate().filter(|(_, m)| placed(m)) {
        let hit = label(m).and_then(|l| offer.iter().find(|o| o.eq_ignore_ascii_case(&l))).filter(|o| seen.insert(o.to_ascii_uppercase()));
        match (hit, matched.get_mut(i)) {
            (Some(o), Some(x)) => *x = Some(o.clone()),
            _ => over.push(i),
        }
    }
    let spare = members.iter().filter(|m| !placed(m)).filter_map(label).collect();
    CoreCount { offer: Some(offer), source: Some(source), offer_text: Some(text), matched, spare, over, twice }
}

/// A cable's core count on `doc`.
pub fn cable_cores(doc: &Document, k: CableId) -> Option<CoreCount> {
    let cab = doc.cables.get(&k)?;
    Some(count_cores(crate::styles::cable::offer(doc, k), &cab.cores))
}

/// The label `cablecore` gives a member whose label another member has (the `core-twice` fix):
/// the first spare core, else (no offer, or none spare) the next number after the highest
/// numeric label.
pub fn next_free_label(doc: &Document, k: CableId) -> Option<String> {
    let cab = doc.cables.get(&k)?;
    let count = count_cores(crate::styles::cable::offer(doc, k), &cab.cores);
    if let Some(c) = count.next_free() {
        return Some(c);
    }
    let high = cab.cores.iter().filter_map(|m| m.label_str()?.trim().parse::<u64>().ok()).filter(|n| *n < 1_000_000).max().unwrap_or(0);
    Some((high + 1).to_string())
}
