//! Quiet points, change-over and fuse-bank labels, and rating keys.

use super::*;

//
// Labels, quiet points and anchors are catalogue code; only
// user-overridable values (the prefix) are in the defaults registry, `defaults/symbols.rs`.

/// Whether point `point` (a definition key) of `s` is quiet: left unwired, it raises no
/// `Unconnected` check.
pub fn is_quiet(s: &Starter, point: &str) -> bool {
    match s.quiet {
        Quiet::None => false,
        Quiet::All => true,
        Quiet::AllBut(loud) => !loud.contains(&point),
    }
}

// ---------------------------------------------------------------- change-over contacts

/// The labels a change-over contact gets joining a device whose labels are `used`:
/// s = 1 + the highest IEC sequence digit among labels of the form `[1-9][1-4]`, then `s1`,
/// `s2`, `s4` (common, NC, NO: definition order).
///
/// # Errors
/// s would be over 9: `<who> already has 9 contacts.`
pub fn changeover_labels(used: &BTreeSet<String>, who: &str) -> Result<Vec<String>, CmdError> {
    sequence_labels(used, &BTreeSet::new(), &['1', '2', '4'], who)
}

/// The labels a contact gets joining a device: `suffixes` after the
/// next IEC sequence digit s, in that order (`['3', '4']` NO, `['1', '2']` NC, `['1', '2', '4']`
/// change-over).
///
/// # Semantics
/// s = 1 + the highest digit among the labels of `used` of the form `[1-9][1-4]` that are not in
/// `skip`, then the first digit from there whose labels `used` does not already hold. `skip` is a
/// fuse bank's own outputs ([`bank_outputs`]): fuse `11` or `12` is not a contact, so a fault
/// contact on a 12-way bank is still `13`/`14`, and an NC skips to `21`/`22`.
///
/// # Errors
/// s would be over 9: `<who> already has 9 contacts.`
pub fn sequence_labels(used: &BTreeSet<String>, skip: &BTreeSet<String>, suffixes: &[char], who: &str) -> Result<Vec<String>, CmdError> {
    let highest = used
        .iter()
        .filter(|l| !skip.contains(*l))
        .filter_map(|l| match l.as_bytes() {
            [s @ b'1'..=b'9', b'1'..=b'4'] => Some(u32::from(s - b'0')),
            _ => None,
        })
        .max()
        .unwrap_or(0);
    let labels = |s: u32| suffixes.iter().map(|x| format!("{s}{x}")).collect::<Vec<_>>();
    // At most nine digits are tried.
    (highest + 1..=9)
        .map(labels)
        .find(|l| !l.iter().any(|x| used.contains(x)))
        .ok_or_else(|| CmdError::refused(format!("{who} already has 9 contacts.")))
}

/// The output labels of device `c`'s fuse bank, which are not contact labels; empty when it is
/// not a fuse bank.
pub(crate) fn bank_outputs(doc: &Document, c: ecad_model::DeviceId) -> BTreeSet<String> {
    bank_labels(doc, c).into_iter().collect()
}

/// The output labels of device `c` when it is a fuse bank (its typed `device_type`): `1` to its
/// stored ways; empty for any other device.
pub(crate) fn bank_labels(doc: &Document, c: ecad_model::DeviceId) -> Vec<String> {
    use ecad_model::objects::devices::{DeviceType, Protection};
    match doc.devices().get(&c).map(|d| &d.device_type) {
        Some(DeviceType::Protection(Protection::FuseBank(bank))) => (1..=bank.ways).map(|i| i.to_string()).collect(),
        _ => Vec::new(),
    }
}

/// The output labels device `c`'s fuse bank head symbol draws, in its order; empty when its head
/// is not a fuse bank symbol (only to check it against the stored ways).
fn head_bank_labels(doc: &Document, c: ecad_model::DeviceId) -> Vec<String> {
    doc.representations_of(Subject::Device(c))
        .filter(|(_, r)| reps::rep_type(doc, r) == ecad_model::PartRole::Head)
        .filter_map(|(_, r)| crate::devices::tags::rep_starter(doc, r))
        .find(|s| is_fuse_bank(s))
        .map(|s| s.labels().into_iter().filter(|l| ecad_model::naming::words::is_digits(l)).collect())
        .unwrap_or_default()
}

// ---------------------------------------------------------------- ratings

use ecad_model::objects::devices::{RATING_KEY, RATING_OF_PREFIX};

/// Most characters of `<n>` in `rating.<n>` that are read; longer is refused without parsing it
/// (a hostile key cannot make a lookup or a message long).
pub(crate) const MAX_FUSE_LABEL: usize = 12;

/// Whether `key` is `rating` or `rating.<n>`.
pub(crate) fn is_rating_key(key: &str) -> bool {
    key == RATING_KEY || key.starts_with(RATING_OF_PREFIX)
}

/// Whether `s` is a fuse bank (its fuses take `rating.<n>`).
/// When device `c` is a fuse bank whose head symbol draws another number of ways than it stores:
/// the stored ways and the symbol's.
pub(crate) fn bank_ways_mismatch(doc: &Document, c: ecad_model::DeviceId) -> Option<(u8, usize)> {
    use ecad_model::objects::devices::{DeviceType, Protection};
    let Some(DeviceType::Protection(Protection::FuseBank(bank))) = doc.devices().get(&c).map(|d| &d.device_type) else {
        return None;
    };
    let stored = bank.ways;
    let drawn = head_bank_labels(doc, c).len();
    (drawn > 0 && drawn != usize::from(stored)).then_some((stored, drawn))
}

/// The points of device `c` whose stored protective earth differs from what the symbol showing
/// them now says ([`crate::symbols::seeds_pe`]): each point and its stored value.
pub(crate) fn pe_mismatches(doc: &Document, c: ecad_model::DeviceId) -> Vec<(ecad_model::ConnectionPointId, bool)> {
    let mut out = Vec::new();
    for (_, r) in doc.representations_of(Subject::Device(c)) {
        let Some((def, _)) = crate::devices::tags::rep_definition(doc, r) else { continue };
        for (t, at) in crate::devices::merge::shown_connection_points(doc, c, r) {
            let Some(td) = def.connection_points.iter().find(|td| td.at.near(at, 1e-9)) else { continue };
            let Some(stored) = doc.connection_points().get(&t).map(|p| p.protective_earth) else { continue };
            if stored != crate::symbols::seeds_pe(def, &td.key) && !out.iter().any(|(x, _)| *x == t) {
                out.push((t, stored));
            }
        }
    }
    out
}

pub(crate) fn is_fuse_bank(s: &Starter) -> bool {
    s.key.fuse_bank_ways().is_some()
}

/// Check a `rating.<n>` value set on `object` that is neither a protective device nor a terminal
/// strip (a device's own rating, a fuse bank's fuses and a strip's fuse positions
/// are typed fields, `cmd::props::b`).
///
/// # Semantics
/// `rating.<n>` on such a device is refused, and clearing it is always allowed. Any other key, or
/// an object that is not a device: no opinion, the value comes back unchanged.
///
/// # Errors
/// `rating.<n>` set on a device that is not a fuse bank or a strip.
pub(crate) fn setprop_rule(doc: &Document, object: ObjectRef, key: &str, value: Option<&str>) -> Result<Option<String>, CmdError> {
    let ObjectRef::Device(c) = object else { return Ok(value.map(str::to_string)) };
    if !is_rating_key(key) {
        return Ok(value.map(str::to_string));
    }
    // Clearing always works: a `rating.<n>` left on a device that is no
    // longer a fuse bank (its bank symbol erased, or an older free property) must stay removable.
    let clearing = value.map(str::trim).is_none_or(str::is_empty);
    if let Some(n) = key.strip_prefix(RATING_OF_PREFIX).filter(|_| !clearing) {
        let who = crate::devices::tags::tag_words(doc, c);
        // Capped: only the first 12 characters are ever shown.
        let shown: String = n.chars().take(MAX_FUSE_LABEL).collect();
        let more = if n.chars().nth(MAX_FUSE_LABEL).is_some() { "…" } else { "" };
        return Err(CmdError::refused(format!("rating.{shown}{more} is for a fuse bank's fuses; {who} is not a fuse bank.")));
    }
    crate::devices::json::property_value(doc, key, value)
}
