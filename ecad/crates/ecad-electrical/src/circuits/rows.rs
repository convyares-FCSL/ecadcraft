//! A subject's results as Checks rows.

use super::kept::*;
use super::*;

use crate::checks::{Check, Fix, Kind as CheckKind, Object};

/// Whether `k` is one of the rated kinds.
pub(crate) fn is_rating_kind(k: CheckKind) -> bool {
    matches!(
        k,
        CheckKind::RatingUnder
            | CheckKind::RatingTooHot
            | CheckKind::RatingLoadOver
            | CheckKind::VoltdropOver
            | CheckKind::RatingBundlingUnset
            | CheckKind::RatingDeviceMissing
            | CheckKind::RatingNoDevice
            | CheckKind::RatingTwoDevices
            | CheckKind::RatingDeviceUnread
            | CheckKind::RatingNamedDevice
            | CheckKind::RatingNoData
            | CheckKind::RatingNoTable
            | CheckKind::RatingTableBadRow
            | CheckKind::RatingTableOdd
            | CheckKind::RatingAssumed
            | CheckKind::RatingNoCurrent
            | CheckKind::RatingBadValue
    )
}

/// Where a subject's rows point.
pub(super) fn place(doc: &Document, s: RatedSubject) -> (Option<ecad_model::PageId>, Option<ecad_model::Pt>, Object) {
    match s {
        RatedSubject::Wire(c) => {
            let (p, a) = crate::checks::places::conductor_place(doc, c, None);
            (p, a, Object::Wire(c))
        }
        RatedSubject::Cable(k) => {
            let (p, a) =
                ecad_model::naming::numbering::rep_anchor(doc, ecad_model::Subject::Cable(k)).map_or((None, None), |(p, a)| (Some(p), Some(a)));
            (p, a, Object::Cable(k))
        }
    }
}

/// A subject's own rows; the count kinds are left to [`summary_rows`].
pub fn rows(doc: &Document, r: &Rating) -> Vec<Check> {
    let s = r.subject;
    let (page, at, object) = place(doc, s);
    let who = s.words(doc);
    let show = Some(Fix::ShowRating(s));
    let dev = r.device.as_ref();
    let i_n = dev.and_then(|d| d.amps);
    let mut out = Vec::new();
    let mut row = |kind: CheckKind, which: String, fix: Option<Fix>| out.push(Check { kind, page, at, object, which, fix });
    for i in &r.issues {
        match i {
            Issue::Under { larger } => {
                let (Some(iz), Some(d), Some(a)) = (r.iz, dev, i_n) else { continue };
                let fix = match (larger, s) {
                    (Some(sz), RatedSubject::Wire(c)) => Some(Fix::UseSize(c, *sz)),
                    (Some(sz), RatedSubject::Cable(k)) => Some(Fix::PickCableType(k, *sz)),
                    _ => show,
                };
                row(
                    CheckKind::RatingUnder,
                    format!("{who} carries {} A; {} is {} A", fmt_amps(iz, Some(a)), d.tag, crate::devices::ratings::dataset::fmt_num(a)),
                    fix,
                );
            }
            Issue::TooHot { place, temp, max } => row(
                CheckKind::RatingTooHot,
                format!(
                    "{place} at {} \u{b0}C is not below {who}'s {} \u{b0}C",
                    crate::devices::ratings::dataset::fmt_num(*temp),
                    crate::devices::ratings::dataset::fmt_num(*max)
                ),
                show,
            ),
            Issue::LoadOver => {
                let (Some(l), Some(d), Some(a)) = (r.load.as_ref(), dev, i_n) else { continue };
                row(
                    CheckKind::RatingLoadOver,
                    format!(
                        "{who} load {} A is above {}'s {} A",
                        crate::devices::ratings::dataset::fmt_num(l.amps),
                        d.tag,
                        crate::devices::ratings::dataset::fmt_num(a)
                    ),
                    show,
                );
            }
            Issue::VoltDropOver => {
                let Some(v) = &r.voltdrop else { continue };
                let larger = v
                    .larger
                    .map(|(sz, p)| format!(" ({} mm\u{b2} would drop {} %)", crate::devices::ratings::dataset::fmt_num(sz), fmt_percent(p)))
                    .unwrap_or_default();
                row(
                    CheckKind::VoltdropOver,
                    format!(
                        "{who} drops {} % at {} A; limit {} %{larger}",
                        fmt_percent(v.percent),
                        crate::devices::ratings::dataset::fmt_num(v.amps),
                        crate::devices::ratings::dataset::fmt_num(v.limit)
                    ),
                    show,
                );
            }
            Issue::DeviceMissing(t) => row(
                CheckKind::RatingDeviceMissing,
                format!("{who} names {t} as its protection; it is not in the project"),
                Some(Fix::ClearProtectedBy(s)),
            ),
            Issue::TwoDevices(a, b) => {
                let used = i_n.map(|x| format!("; {} A ({a}) used", crate::devices::ratings::dataset::fmt_num(x))).unwrap_or_default();
                row(CheckKind::RatingTwoDevices, format!("{who} is fed by {a} and {b}{used}"), None);
            }
            // F6: the bound, said in words.
            Issue::NoDevice(true) => {
                row(CheckKind::RatingNoDevice, format!("{who}: trace stopped after {} nets; set Protected by", trace::MAX_TRACE_NETS), show)
            }
            Issue::NamedDevice(t) => {
                row(CheckKind::RatingNamedDevice, format!("{who} is protected by {t} by hand; {t} is not a protective device"), None)
            }
            Issue::NoTableChosen => row(CheckKind::RatingNoTable, format!("{who}: No rating table chosen"), Some(Fix::ChooseRatingTable)),
            Issue::NoTable(why) => {
                let fix = if r.basis == Basis::Table && has_datasheet(doc, s) { Some(Fix::SetBasis(s, Basis::Datasheet)) } else { None };
                row(CheckKind::RatingNoTable, format!("{who}: {why}"), fix);
            }
            Issue::BadValue(key, v) => row(CheckKind::RatingBadValue, format!("{who} has {key} '{v}', which ECAD does not read"), show),
            _ => {}
        }
    }
    out
}

/// Whether a subject's category or cable type has a rated current (the **Use Datasheet** fix).
pub(super) fn has_datasheet(doc: &Document, s: RatedSubject) -> bool {
    match s {
        RatedSubject::Wire(c) => sheet(doc, ecad_model::StyleCatalogue::Wire, wire::WireDraws::new(doc).category(doc, c).0.as_ref()).rated.is_some(),
        RatedSubject::Cable(k) => sheet(doc, ecad_model::StyleCatalogue::Cable, cable::of(doc, k).as_ref()).rated.is_some(),
    }
}

pub(super) fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 { format!("1 {one}") } else { format!("{n} {many}") }
}

/// The project-wide rows: the count kinds, each unread device once, and the
/// rating table's own rows. Built from [`Kept`]'s tallies; nothing is rescanned.
pub(crate) fn summary_rows(doc: &Document, k: &Kept) -> Vec<Check> {
    let t = k.tally_now();
    let mut out = Vec::new();
    let mut row = |kind: CheckKind, which: String, fix: Option<Fix>| out.push(Check { kind, page: None, at: None, object: Object::None, which, fix });
    if t.bundling_unset > 0 {
        let verb = if t.bundling_unset == 1 { "has" } else { "have" };
        row(
            CheckKind::RatingBundlingUnset,
            format!("{} {verb} no bundling factor (\u{d7} 1.00 used)", plural(t.bundling_unset, "panel wire", "panel wires")),
            Some(Fix::ChooseRatingTable),
        );
    }
    if t.no_device > 0 {
        row(
            CheckKind::RatingNoDevice,
            format!("{} no protective device upstream", plural(t.no_device, "rated wire or cable has", "rated wires and cables have")),
            Some(Fix::ShowRatedWires),
        );
    }
    // Rated subjects lacking a value, and protected wires with no rating data.
    let no_data = t.no_data + k.unrated();
    if no_data > 0 {
        row(
            CheckKind::RatingNoData,
            format!("{} no rating data", plural(no_data, "protected wire or cable has", "protected wires and cables have")),
            Some(Fix::OpenStandards(ecad_model::StyleCatalogue::Wire)),
        );
    }
    if t.assumed > 0 {
        row(
            CheckKind::RatingAssumed,
            format!("{} no cable type; Cu, 70 \u{b0}C, class 5 assumed", plural(t.assumed, "cable has", "cables have")),
            Some(Fix::OpenStandards(ecad_model::StyleCatalogue::Cable)),
        );
    }
    if t.no_current > 0 {
        row(CheckKind::RatingNoCurrent, format!("{} no load and no device; no volt drop", plural(t.no_current, "cable has", "cables have")), None);
    }
    for (c, text) in k.unread() {
        let tag = crate::devices::tags::tag_words(doc, c);
        let (page, at) =
            ecad_model::naming::numbering::rep_anchor(doc, ecad_model::Subject::Device(c)).map_or((None, None), |(p, a)| (Some(p), Some(a)));
        out.push(Check {
            kind: CheckKind::RatingDeviceUnread,
            page,
            at,
            object: Object::Device(c),
            which: if text.is_empty() { format!("{tag} has no rated current") } else { format!("{tag}'s rating '{text}' has no rated current") },
            fix: None,
        });
    }
    for (c, loc, v) in k.bad_boxes() {
        // The box stores an entry key; the row names the entry's code.
        let loc = if loc.is_empty() { String::new() } else { location_code(doc, loc) };
        let (page, at) =
            ecad_model::naming::numbering::rep_anchor(doc, ecad_model::Subject::Aid(c)).map_or((None, None), |(p, a)| (Some(p), Some(a)));
        out.push(Check {
            kind: CheckKind::RatingBadValue,
            page,
            at,
            object: Object::Aid(c),
            which: format!(
                "{} has panel temperature '{v}', which ECAD does not read (\u{2212}40 to 90 \u{b0}C)",
                if loc.is_empty() { "A location box" } else { loc.as_str() }
            ),
            fix: None,
        });
    }
    for i in k.table_rows() {
        let (kind, which) = match i {
            Issue::BadValue(_, w) | Issue::NoTable(w) => (CheckKind::RatingTableBadRow, w.clone()),
            Issue::NoData(w) => (CheckKind::RatingTableOdd, w.clone()),
            _ => continue,
        };
        out.push(Check {
            kind,
            page: None,
            at: None,
            object: Object::None,
            which,
            fix: Some(Fix::OpenStandards(ecad_model::StyleCatalogue::RatingTable)),
        });
    }
    out
}
