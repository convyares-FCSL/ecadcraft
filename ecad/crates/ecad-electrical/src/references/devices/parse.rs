//! The grammar of what a part offers and where its contact table goes (FP1, FP2): `3P+1NO+1NC`, `at 12,-30`.

use crate::CmdError;
use crate::devices::traits::DeviceKind;
use ecad_model::objects::devices::PoleKind;
use ecad_model::objects::devices::contacts::{MAX_OFFSET_MM, MAX_POLES};
use ecad_model::objects::devices::{ContactEntry, ContactLabel, ContactPoles, ContactSet, ContactTable};
use ecad_model::{ContactMirror, Device as StoredDevice, DeviceId, DeviceType, Document, Pt};

/// The `setprop` key of the contacts a part offers (FP1): `3P+1NO+1NC`. A typed field, never in a
/// property bag.
pub const CONTACTS_KEY: &str = "contacts";
/// The `setprop` key of where a switching device's contact table goes (FP2). A typed field.
pub const MIRROR_KEY: &str = "mirror";

// ---------------------------------------------------------------- offer grammar (FP1)

/// One pole a part offers, with its labels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfferedPole {
    pub labels: [String; 2],
    pub kind: PoleKind,
    /// A power (main) pole rather than an auxiliary contact.
    pub power: bool,
}

/// The refusal of an unreadable offer.
pub(crate) fn not_a_contact_list(text: &str) -> String {
    let shown: String = text.trim().chars().take(40).collect();
    format!("\"{shown}\" is not a contact list. Write it like 3P+1NO+1NC, or 13/14 NO, 21/22 NC.")
}

/// Read a typed offer (design 025 FP1): entries separated by `+` or `,`, case-insensitive, spaces
/// ignored. `3P` (power NO poles), `1PNC` (power NC poles), `1NO` / `2NC` (auxiliary contacts), or
/// one contact with explicit labels: `13/14 NO`, `21/22 NC`, `L1/T1 P`, `1/2 PNC`.
///
/// # Errors
/// Unreadable text, no poles, or more than [`MAX_POLES`].
pub(crate) fn parse_set(text: &str) -> Result<ContactSet, CmdError> {
    let bad = || CmdError::refused(not_a_contact_list(text));
    let too_many = || CmdError::refused(format!("A part offers at most {MAX_POLES} contacts."));
    let mut out: Vec<ContactEntry> = Vec::new();
    for entry in text.split(['+', ',']) {
        let e: String = entry.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_uppercase();
        if e.is_empty() {
            return Err(bad());
        }
        let (body, kind, power) = if let Some(b) = e.strip_suffix("PNC") {
            (b, PoleKind::Nc, true)
        } else if let Some(b) = e.strip_suffix("NO") {
            (b, PoleKind::No, false)
        } else if let Some(b) = e.strip_suffix("NC") {
            (b, PoleKind::Nc, false)
        } else if let Some(b) = e.strip_suffix('P') {
            (b, PoleKind::No, true)
        } else {
            return Err(bad());
        };
        let poles = if let Some((a, b)) = body.split_once('/') {
            let label = |l: &str| {
                let ok = !l.is_empty() && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '.');
                if ok { ContactLabel::new(l).ok() } else { None }
            };
            ContactPoles::Labels([label(a).ok_or_else(bad)?, label(b).ok_or_else(bad)?])
        } else {
            let n: usize = body.parse().map_err(|_| bad())?;
            if n == 0 {
                return Err(bad());
            }
            ContactPoles::Count(u8::try_from(n).ok().and_then(std::num::NonZeroU8::new).ok_or_else(too_many)?)
        };
        out.push(ContactEntry { poles, kind, power });
        if out.iter().map(ContactEntry::len).sum::<usize>() > MAX_POLES {
            return Err(too_many());
        }
    }
    if out.is_empty() {
        return Err(bad());
    }
    Ok(ContactSet::new(out))
}

/// The poles `set` offers, numbered (design 025 FP1): power NO poles `1/2`, `3/4`…, power NC poles
/// continuing the power numbering, auxiliary contacts with the IEC sequence digit in written order
/// (`13/14` NO, `21/22` NC). A counted pole skips labels already offered.
pub(crate) fn poles_offered(set: &ContactSet) -> Vec<OfferedPole> {
    let mut out: Vec<OfferedPole> = Vec::new();
    let (mut power_n, mut seq) = (0u32, 0u32);
    for e in set.entries() {
        let (kind, power) = (e.kind, e.power);
        match &e.poles {
            ContactPoles::Labels([a, b]) => out.push(OfferedPole { labels: [a.to_string(), b.to_string()], kind, power }),
            ContactPoles::Count(n) => {
                for _ in 0..n.get() {
                    let labels = loop {
                        let labels = if power {
                            power_n += 1;
                            [(2 * power_n - 1).to_string(), (2 * power_n).to_string()]
                        } else {
                            seq += 1;
                            let (a, b) = if kind == PoleKind::No { (3, 4) } else { (1, 2) };
                            [format!("{seq}{a}"), format!("{seq}{b}")]
                        };
                        if !out.iter().any(|o| o.labels.iter().any(|l| labels.contains(l))) {
                            break labels;
                        }
                    };
                    out.push(OfferedPole { labels, kind, power });
                }
            }
        }
    }
    out
}

/// Read an offer and number its poles ([`parse_set`], [`poles_offered`]).
///
/// # Errors
/// As [`parse_set`].
pub fn parse_offer(text: &str) -> Result<Vec<OfferedPole>, CmdError> {
    parse_set(text).map(|s| poles_offered(&s))
}

/// The contacts component `c` offers: its typed set ([`crate::devices::traits::Contacts`]); `None` when its
/// group has no contacts.
pub(crate) fn contacts_of(doc: &Document, c: DeviceId) -> Option<&ContactSet> {
    doc.devices().get(&c)?.device_type.as_contacts().map(|x| x.contacts())
}

/// What component `c` offers, as written and numbered, when it offers any.
pub fn offer_of(doc: &Document, c: DeviceId) -> Option<(String, Vec<OfferedPole>)> {
    contacts_of(doc, c).filter(|s| !s.is_empty()).map(|s| (s.to_string(), poles_offered(s)))
}

/// Device `d` (named `who` in refusals) with its contacts set to the typed `text` (`None` or blank
/// clears them).
///
/// # Errors
/// A device whose group has no contacts; as [`parse_set`].
pub fn with_contacts(d: &StoredDevice, who: &str, text: Option<&str>) -> Result<StoredDevice, CmdError> {
    let set = match text.map(str::trim).filter(|t| !t.is_empty()) {
        None => ContactSet::default(),
        Some(t) => parse_set(t)?,
    };
    let mut out = d.clone();
    match out.device_type.as_contacts_mut() {
        Some(x) => *x.contacts_mut() = set,
        None if set.is_empty() => {}
        None => {
            return Err(CmdError::refused(format!("{who} is a {}: it has no contacts.", d.device_type.group().label().to_lowercase())));
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------- placement (FP2, FP3)

/// Where a device's table goes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Placement {
    Auto,
    Below,
    Right,
    Off,
    /// Millimetres from the head symbol's insertion point (Move Mirror Table).
    At(Pt),
}

impl Placement {
    /// Its stored word: `auto`, `below`, `right`, `off`, `at 12,-30`.
    pub fn word(self) -> String {
        match self {
            Placement::Auto => "auto".into(),
            Placement::Below => "below".into(),
            Placement::Right => "right".into(),
            Placement::Off => "off".into(),
            Placement::At(p) => format!("at {},{}", trim_num(p.x), trim_num(p.y)),
        }
    }

    /// The placement a stored [`ContactTable`] says.
    pub(crate) fn of_table(t: ContactTable) -> Placement {
        match t {
            ContactTable::Auto => Placement::Auto,
            ContactTable::Below => Placement::Below,
            ContactTable::Right => Placement::Right,
            ContactTable::Off => Placement::Off,
            ContactTable::At { dx, dy } => Placement::At(Pt::new(dx, dy)),
        }
    }

    /// Its stored form.
    pub fn table(self) -> ContactTable {
        match self {
            Placement::Auto => ContactTable::Auto,
            Placement::Below => ContactTable::Below,
            Placement::Right => ContactTable::Right,
            Placement::Off => ContactTable::Off,
            Placement::At(p) => ContactTable::At { dx: p.x, dy: p.y },
        }
    }

    fn of_default(m: ContactMirror) -> Placement {
        match m {
            ContactMirror::Auto => Placement::Auto,
            ContactMirror::Below => Placement::Below,
            ContactMirror::Right => Placement::Right,
            ContactMirror::Off => Placement::Off,
        }
    }
}

/// A number without trailing zeros (`12`, `-3.5`), rounded to a hundredth of a millimetre.
fn trim_num(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    let r = if r == 0.0 { 0.0 } else { r };
    format!("{r}")
}

/// The refusal of an unreadable `mirror` value.
pub(crate) fn not_a_placement(text: &str) -> String {
    let shown: String = text.trim().chars().take(40).collect();
    format!("\"{shown}\" is not a table placement. Use auto, below, right, off, or at dx,dy in millimetres.")
}

/// Read a `mirror` value (design 025 FP2).
///
/// # Errors
/// Not one of the words, or an offset that is not two finite numbers within [`MAX_OFFSET_MM`].
pub fn parse_mirror(text: &str) -> Result<Placement, CmdError> {
    let t = text.trim();
    if let Some(m) = ContactMirror::parse(t) {
        return Ok(Placement::of_default(m));
    }
    let lower = t.to_ascii_lowercase();
    let rest = lower.strip_prefix("at").ok_or_else(|| CmdError::refused(not_a_placement(t)))?.trim();
    let (a, b) = rest.split_once(',').ok_or_else(|| CmdError::refused(not_a_placement(t)))?;
    let (x, y) = (
        a.trim().parse::<f64>().map_err(|_| CmdError::refused(not_a_placement(t)))?,
        b.trim().parse::<f64>().map_err(|_| CmdError::refused(not_a_placement(t)))?,
    );
    if !(x.is_finite() && y.is_finite() && x.abs() <= MAX_OFFSET_MM && y.abs() <= MAX_OFFSET_MM) {
        return Err(CmdError::refused(not_a_placement(t)));
    }
    Ok(Placement::At(Pt::new(x, y)))
}

/// The table placement a typed `mirror` value says; `None` for empty, `default` or `project
/// default` (the project's default).
///
/// # Errors
/// As [`parse_mirror`].
pub fn table_of_typed(text: Option<&str>) -> Result<Option<ContactTable>, CmdError> {
    let Some(t) = text.map(str::trim).filter(|t| !t.is_empty()) else { return Ok(None) };
    if t.eq_ignore_ascii_case("default") || t.to_ascii_lowercase().starts_with("project default") {
        return Ok(None);
    }
    Ok(Some(parse_mirror(t)?.table()))
}

/// Device `d` (named `who` in refusals) with its contact table placed at `table` (`None`: the
/// project's default).
///
/// # Errors
/// Not a switching device (only a switching device has a contact table of its own).
pub(crate) fn with_table(d: &StoredDevice, who: &str, table: Option<ContactTable>) -> Result<StoredDevice, CmdError> {
    let mut out = d.clone();
    match &mut out.device_type {
        DeviceType::Switching(s) => s.contact_table = table,
        _ if table.is_none() => {}
        _ => return Err(CmdError::refused(format!("{who} has no contact table: only a switching device has one."))),
    }
    Ok(out)
}

/// Where `c`'s table goes and whether the device says so itself (else the project default).
pub fn placement_of(doc: &Document, c: DeviceId) -> (Placement, bool) {
    match doc.devices().get(&c).and_then(|x| x.device_type.as_contacts()).and_then(|x| x.contact_table()) {
        Some(t) => (Placement::of_table(t), true),
        None => (Placement::of_default(doc.settings().contact_mirror), false),
    }
}
