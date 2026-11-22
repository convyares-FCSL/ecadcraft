//! The label rule (brief 032 §4).

use super::*;

/// Which routes on a page get a label (design 032 §4.1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum LabelRule {
    /// One label per net (and label text) per page.
    Net,
    /// Every wire whose longest segment is at least this long, millimetres.
    Each(f64),
}

/// One route as the label rule sees it.
#[derive(Debug, Clone)]
pub(crate) struct LabelCandidate {
    pub rep: RepresentationId,
    pub conductor: WireId,
    /// The route's net, if it has one.
    pub net: Option<ecad_model::NetKey>,
    pub text: String,
    /// Length of its longest segment.
    pub longest: f64,
}

/// The candidates (by index) that get a label under `rule` (design 032 §4.2): with
/// [`LabelRule::Net`] one per (net, label text), on the route whose longest segment is longest
/// (ties: lower conductor ID, then lower representation ID); with [`LabelRule::Each`] every route
/// long enough. In both, every net drawn on the page is labelled at least once. The result keeps
/// the candidates' order.
pub(crate) fn labelled(candidates: &[LabelCandidate], rule: LabelRule) -> Vec<usize> {
    // The better of two candidates for one label.
    let better =
        |a: &LabelCandidate, b: &LabelCandidate| a.longest > b.longest || (a.longest == b.longest && (a.conductor, a.rep) < (b.conductor, b.rep));
    let mut chosen = vec![false; candidates.len()];
    match rule {
        LabelRule::Net => {
            let mut best: BTreeMap<(Option<ecad_model::NetKey>, Option<WireId>, &str), usize> = BTreeMap::new();
            for (i, c) in candidates.iter().enumerate() {
                // A wire without a net is its own group.
                let key = (c.net, if c.net.is_none() { Some(c.conductor) } else { None }, c.text.as_str());
                match best.get(&key).and_then(|j| candidates.get(*j)) {
                    Some(b) if !better(c, b) => {}
                    _ => {
                        best.insert(key, i);
                    }
                }
            }
            for i in best.into_values() {
                if let Some(x) = chosen.get_mut(i) {
                    *x = true;
                }
            }
        }
        LabelRule::Each(min) => {
            for (i, c) in candidates.iter().enumerate() {
                if c.longest >= min
                    && let Some(x) = chosen.get_mut(i)
                {
                    *x = true;
                }
            }
        }
    }
    // Every net drawn on the page is labelled at least once: on its best route.
    let mut nets: BTreeMap<(Option<ecad_model::NetKey>, Option<WireId>), (bool, usize)> = BTreeMap::new();
    for (i, c) in candidates.iter().enumerate() {
        let key = (c.net, if c.net.is_none() { Some(c.conductor) } else { None });
        let on = chosen.get(i).copied().unwrap_or(false);
        let e = nets.entry(key).or_insert((on, i));
        e.0 |= on;
        if candidates.get(e.1).is_some_and(|b| better(c, b)) {
            e.1 = i;
        }
    }
    for (labelled, best) in nets.into_values() {
        if !labelled && let Some(x) = chosen.get_mut(best) {
            *x = true;
        }
    }
    chosen.iter().enumerate().filter(|(_, on)| **on).map(|(i, _)| i).collect()
}

/// The project's label rule word, `net` or `each` (the registered `wiring.label_rule`; a stored
/// value it does not know resolves as the default).
pub fn label_rule_word(doc: &Document) -> &'static str {
    crate::defaults::resolve(doc, &crate::defaults::wires::WIRING_LABEL_RULE, crate::defaults::Scope::project()).word()
}

/// The project's shortest labelled wire, millimetres (the registered `wiring.label_min_length`;
/// out of range resolves as the default).
pub fn label_min_length(doc: &Document) -> f64 {
    crate::defaults::resolve(doc, &crate::defaults::wires::WIRING_LABEL_MIN_LENGTH, crate::defaults::Scope::project()).number()
}

/// The project's label rule (design 032 §4.1).
pub(crate) fn label_rule(doc: &Document) -> LabelRule {
    if label_rule_word(doc) == ecad_model::connectivity::wiring::LABEL_RULE_EACH { LabelRule::Each(label_min_length(doc)) } else { LabelRule::Net }
}

/// The first terminal of `shown` (as `ecad_model::connectivity::derive::connection_point_positions` lists them) within
/// `tol` of `at`.
pub fn first_near(shown: &[(ConnectionPointId, RepresentationId, Pt)], at: Pt, tol: f64) -> Option<ConnectionPointId> {
    shown.iter().find(|(_, _, p)| p.near(at, tol)).map(|(t, _, _)| *t)
}

/// The terminal at each of many points on one document (`translate::connection_point_at`): the positions are worked out on
/// first use and kept, so an edit that moves many route ends lists them once, not once per end.
pub struct ShownOnce<'a> {
    doc: &'a Document,
    page: PageId,
    shown: Option<Vec<(ConnectionPointId, RepresentationId, Pt)>>,
}

impl<'a> ShownOnce<'a> {
    pub fn new(doc: &'a Document, page: PageId) -> Self {
        Self { doc, page, shown: None }
    }

    pub fn connection_point_at(&mut self, at: Pt, tol: f64) -> Option<ConnectionPointId> {
        let (doc, page) = (self.doc, self.page);
        first_near(self.shown.get_or_insert_with(|| ecad_model::connectivity::derive::connection_point_positions(doc, page)), at, tol)
    }
}
