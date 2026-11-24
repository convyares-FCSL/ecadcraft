//! The device checks: a device's tag and parts, the designations list.

use super::*;
/// The project's list of functions and locations ([`designation_rows`]).
pub(crate) struct DesignationRows;

impl CheckSource for DesignationRows {
    fn key(&self) -> Key {
        Key::Designations
    }
    fn touched(&self, scope: &Scope) -> bool {
        scope.designations
    }
    fn checks(&self, doc: &Document, _registry: &ecad_model::registry::data::Registry, _look: &dyn ecad_model::look::Look) -> Vec<Check> {
        designation_rows(doc)
    }
}

/// The checks of component `c`, reading the book and grids through `places`, shared across a Checks
/// pass so the book is built at most once.
pub(crate) fn component_checks_in(places: &mut Places<'_>, c: DeviceId, m: &dyn Look) -> Vec<Check> {
    let doc = places.doc();
    let mut out = Vec::new();
    designation_checks(places, c, &mut out);
    // A provisional tag, and a tag a cable holds too.
    if let Some(device) = doc.devices().get(&c)
        && let (Some(key), Some(t)) = (device.tag_key(), device.tag_shown())
        && (device.is_provisional() || !doc.cables_with(&key).is_empty())
    {
        // Located only when there is a row to show (a build visits every device).
        let (page, at) = crate::symbols::reps::goto_place_in(places, c).map_or((None, None), |(p, a)| (Some(p), a));
        if device.is_provisional() {
            out.push(Check { kind: Kind::ProvisionalTag, page, at, object: Object::Device(c), which: t.clone(), fix: Some(Fix::Finalise) });
        }
        if !doc.cables_with(&key).is_empty() {
            out.push(Check {
                kind: Kind::DuplicateTag,
                page,
                at,
                object: Object::Device(c),
                which: format!("A cable and a device share {t}"),
                fix: None,
            });
        }
    }
    // Drawn beyond the part's offer, ringed at the first such pole.
    for (which, page, at) in crate::references::devices::read::count_checks(doc, c) {
        out.push(Check { kind: Kind::TooManyContacts, page, at, object: Object::Device(c), which, fix: None });
    }
    // Checks read only the typed rating; a note is not a rating.
    if let Some(d) = doc.devices().get(&c)
        && d.has_no_rating()
    {
        let (page, at) = crate::symbols::reps::goto_place_in(places, c).map_or((None, None), |(p, a)| (Some(p), a));
        let tag = crate::devices::tags::tag_words(doc, c);
        let which = match &d.rating_note {
            Some(n) => format!("{tag} has no rating ('{n}' is a note)"),
            None => format!("{tag} has no rating"),
        };
        out.push(Check { kind: Kind::NoRating, page, at, object: Object::Device(c), which, fix: None });
    }
    // A motor's rating against the supply typed on its potentials.
    if let Some(m) = crate::devices::supply::motor_of(doc, c)
        && let Some(fed) = crate::devices::supply::fed(doc, c)
        && let Some(which) =
            crate::devices::supply::motor_mismatch(&crate::devices::tags::tag_words(doc, c), m, &fed, crate::devices::supply::tolerance(doc))
    {
        let (page, at) = crate::symbols::reps::goto_place_in(places, c).map_or((None, None), |(p, a)| (Some(p), a));
        out.push(Check { kind: Kind::SupplyMismatch, page, at, object: Object::Device(c), which, fix: None });
    }
    // Data stored from the symbol at placement, against the symbol drawn now.
    if let Some((stored, drawn)) = crate::symbols::rules::bank_ways_mismatch(doc, c) {
        let (page, at) = crate::symbols::reps::goto_place_in(places, c).map_or((None, None), |(p, a)| (Some(p), a));
        let which = format!("{} has {stored} ways; its symbol draws {drawn}", crate::devices::tags::tag_words(doc, c));
        out.push(Check { kind: Kind::SymbolMismatch, page, at, object: Object::Device(c), which, fix: None });
    }
    for (t, stored) in crate::symbols::rules::pe_mismatches(doc, c) {
        let (page, at) = crate::symbols::reps::goto_place_in(places, c).map_or((None, None), |(p, a)| (Some(p), a));
        let point = format!("{}:{}", crate::devices::tags::tag_words(doc, c), crate::devices::merge::label_of(doc, t));
        let which = if stored {
            format!("{point} is a protective earth point; its symbol draws none")
        } else {
            format!("{point} is not a protective earth point; its symbol draws one")
        };
        out.push(Check { kind: Kind::SymbolMismatch, page, at, object: Object::Device(c), which, fix: None });
    }
    // A connector's mating checks.
    out.extend(crate::connectors::checks(doc, c));
    // A table the user placed over the title block stays there.
    if let Some((which, page, at)) = crate::references::devices::read::title_check(doc, c, m) {
        out.push(Check {
            kind: Kind::TableOverTitle,
            page: Some(page),
            at: Some(at),
            object: Object::Device(c),
            which,
            fix: Some(Fix::MoveTable(c)),
        });
    }
    out
}

/// The rows for component `c`: a stored function or location naming no entry
/// (devices and boxes); for a device, a value none of its symbols' contexts has, and two heads.
/// O(its representations and their pages' boxes).
pub(super) fn designation_checks(places: &mut Places<'_>, c: DeviceId, out: &mut Vec<Check>) {
    use crate::locations::designations::{self as d, Aspect};
    let doc = places.doc();
    let Some(comp) = doc.devices().get(&c) else { return };
    if !crate::locations::is_device(doc, c) {
        return;
    }
    let who = crate::devices::tags::tag_words(doc, c);
    let (page, at) = crate::symbols::reps::goto_place_in(places, c).map_or((None, None), |(p, a)| (Some(p), a));
    for (k, v) in d::device_keys(comp) {
        let Some(a) = Aspect::of_property(&k) else { continue };
        if d::code_of(doc, a, &v).is_none() {
            out.push(Check {
                kind: Kind::DesignationMissing,
                page,
                at,
                object: Object::Device(c),
                which: format!("{who}'s {} is no longer in the list", a.word()),
                fix: Some(Fix::SetDesignation(c, a)),
            });
        }
    }
    let reps: Vec<&ecad_model::Representation> = doc.representations_of(Subject::Device(c)).map(|(_, r)| r).collect();
    if reps.is_empty() {
        return;
    }
    let mut cache = crate::locations::BoxCache::default();
    for a in Aspect::ALL {
        let own = crate::locations::resolve_in(doc, c, a);
        let (Some(key), Some(code)) = (own.key.clone(), own.value.clone()) else { continue };
        let contexts: Vec<crate::locations::Located> = reps.iter().map(|r| crate::locations::rep_context_in(doc, a, r, &mut cache)).collect();
        if contexts.iter().any(|x| x.key.as_deref() == Some(key.as_str())) {
            continue;
        }
        // **Use +CP1** when every symbol is in one context that has a value.
        let shared = contexts.first().filter(|f| f.value.is_some() && contexts.iter().all(|x| x.key == f.key)).and_then(|f| f.value.clone());
        let use_words = shared.map(|s| format!(" (all of them are in {s})")).unwrap_or_default();
        out.push(Check {
            kind: if a == Aspect::Location { Kind::LocationUnused } else { Kind::FunctionUnused },
            page,
            at,
            object: Object::Device(c),
            which: format!("{who} is {code}, but none of its symbols is in {code}{use_words}"),
            fix: contexts.first().filter(|f| f.value.is_some() && contexts.iter().all(|x| x.key == f.key)).map(|_| Fix::UseContext(c, a)),
        });
    }
    // A value set by hand, and a box of another value holding one of the
    // device's symbols: the error goes on every placed symbol of the device, with one message.
    for a in Aspect::ALL {
        let Some(own) = d::device_key(comp, a).filter(|_| d::device_fixed(comp, a)) else { continue };
        let boxed = |r: &ecad_model::Representation, cache: &mut crate::locations::BoxCache| {
            let at = crate::locations::anchor(doc, r)?;
            crate::locations::innermost_in(cache.on(doc, r.page), a, at).and_then(|b| b.key(a)).map(str::to_string)
        };
        if !reps.iter().any(|r| boxed(r, &mut cache).is_some_and(|k| k != own)) {
            continue;
        }
        let words = if a == Aspect::Location { Kind::TiedLocations } else { Kind::TiedFunctions };
        for r in &reps {
            out.push(Check {
                kind: words,
                page: Some(r.page),
                at: crate::locations::anchor(doc, r),
                object: Object::Device(c),
                which: format!("{who}: {}", words.words()),
                fix: Some(Fix::SetDesignation(c, a)),
            });
        }
    }
    let heads = crate::symbols::reps::heads(doc, c);
    if let [_, second, ..] = heads.as_slice()
        && let Some(r) = doc.representations().get(second)
    {
        let mut xp = crate::references::devices::places::Places::new(doc);
        let wheres: Vec<String> = heads
            .iter()
            .filter_map(|h| doc.representations().get(h).map(|x| xp.text(x.page, crate::references::devices::classify::anchor(doc, x))))
            .collect();
        out.push(Check {
            kind: Kind::TwoHeads,
            page: Some(r.page),
            at: crate::references::devices::classify::anchor(doc, r),
            object: Object::Device(c),
            which: format!("{who} has two {}s ({})", crate::devices::tags::head_noun(doc, *second), wheres.join(", ")),
            fix: None,
        });
    }
}

/// The rows of the project's list: two entries of one aspect sharing a code.
pub fn designation_rows(doc: &Document) -> Vec<Check> {
    use crate::locations::designations::{self as d, Aspect};
    let mut out = Vec::new();
    for a in Aspect::ALL {
        let mut seen: BTreeMap<String, usize> = BTreeMap::new();
        for e in d::entries(doc, a) {
            *seen.entry(e.code.to_uppercase()).or_default() += 1;
        }
        for (code, n) in seen {
            if n > 1 {
                out.push(Check {
                    kind: Kind::DesignationDuplicate,
                    page: None,
                    at: None,
                    object: Object::None,
                    which: format!("Two {} are called {code}", a.plural()),
                    fix: Some(Fix::ShowDesignations),
                });
            }
        }
    }
    out
}
