//! The checks the kept models answer: boxes, strips, the BOM and their counts.

use super::*;

impl ReportModels {
    pub(super) fn set_box_checks(&mut self, doc: &Document, page: PageId) {
        if !doc.pages().contains_key(&page) {
            self.box_checks.remove(&page);
            return;
        }
        let boxes = locations::boxes_on(doc, page);
        let mut out = Vec::new();
        for b in &boxes {
            if locations::crosses_itself(&b.vertices) {
                out.push(Check {
                    kind: Kind::BoxCrosses,
                    page: Some(page),
                    at: b.vertices.first().copied(),
                    object: Object::Aid(b.aid),
                    which: b.location.clone().unwrap_or_else(|| "+?".into()),
                    fix: None,
                });
            }
        }
        for (i, a) in boxes.iter().enumerate() {
            for b in boxes.iter().skip(i + 1) {
                if let Some(at) = crossing(&a.vertices, &b.vertices) {
                    out.push(Check {
                        kind: Kind::BoxesOverlap,
                        page: Some(page),
                        at: Some(at),
                        object: Object::Aid(a.aid),
                        which: format!("{} and {}", a.location.as_deref().unwrap_or("+?"), b.location.as_deref().unwrap_or("+?")),
                        fix: None,
                    });
                }
            }
        }
        let old = self.box_checks.get(&page);
        if old.map_or(out.is_empty(), |o| *o == out) {
            return;
        }
        let tally = |v: &[Check]| {
            let w = v.iter().filter(|c| c.severity() == crate::checks::Severity::Warning).count();
            [w, v.len() - w]
        };
        let before = old.map_or([0, 0], |o| tally(o));
        let now = tally(&out);
        for i in 0..2 {
            if let (Some(n), Some(b), Some(a)) = (self.box_counts.get_mut(i), before.get(i), now.get(i)) {
                *n = n.saturating_sub(*b) + a;
            }
        }
        self.checks_changed += 1;
        if out.is_empty() {
            self.box_checks.remove(&page);
        } else {
            self.box_checks.insert(page, out);
        }
    }

    /// The location rows of the Checks list.
    pub fn checks(&self) -> Vec<Check> {
        let mut out: Vec<Check> = self.box_checks.values().flatten().cloned().collect();
        out.extend(self.continuation.iter().cloned());
        let (n, l) = (self.no_part_number, self.lines_no_part);
        if n > 0 || l > 0 {
            let devices = if n == 1 { "1 device".to_string() } else { format!("{n} devices") };
            let lines = if l == 1 { "1 part line".to_string() } else { format!("{l} part lines") };
            let which = match (n, l) {
                (_, 0) => format!("{devices} {} no part number", if n == 1 { "has" } else { "have" }),
                (0, _) => format!("{lines} {} no part number", if l == 1 { "has" } else { "have" }),
                _ => format!("{devices} and {lines} have no part number"),
            };
            out.push(Check { kind: Kind::NoPartNumber, page: None, at: None, object: Object::None, which, fix: Some(Fix::ShowReports) });
        }
        out.extend(self.bom_checks());
        let n = self.no_location;
        if n > 0 && n < self.devices.len() {
            let which = if n == 1 { "1 device has no location".to_string() } else { format!("{n} devices have no location") };
            out.push(Check { kind: Kind::NoLocation, page: None, at: None, object: Object::None, which, fix: Some(Fix::ShowReports) });
        }
        // `function-none`, as `location-none`.
        let n = self.no_function;
        if n > 0 && n < self.devices.len() {
            let which = if n == 1 { "1 device has no function".to_string() } else { format!("{n} devices have no function") };
            out.push(Check { kind: Kind::NoFunction, page: None, at: None, object: Object::None, which, fix: Some(Fix::ShowReports) });
        }
        out.extend(self.strip_checks());
        out.extend(self.plc.checks());
        out
    }

    /// The Checks rows, from the kept strip and cable rows.
    pub fn strip_checks(&self) -> Vec<Check> {
        let mut out = Vec::new();
        for s in self.strips.strips.values() {
            // Per type; with no type codes, the row for the whole strip.
            for (tc, offered) in s.over_offer() {
                let legacy = s.offer.is_some_and(|o| o.counts.iter().skip(2).all(|n| *n == 0)) && s.types.iter().all(|t| t.ty.legacy());
                let hi = if legacy { s.highest() } else { s.highest_of(tc.ty) };
                let which = if legacy {
                    format!("{}: {} terminals drawn, strip offers {offered}", s.tag, s.drawn)
                } else {
                    format!("{}: {} {} terminals drawn, strip offers {offered}", s.tag, tc.drawn, tc.ty.word)
                };
                out.push(Check {
                    kind: Kind::TooManyTerminalBlocks,
                    page: hi.and_then(|r| r.go).map(|(p, _)| p),
                    at: hi.and_then(|r| r.go).map(|(_, a)| a),
                    object: hi.and_then(|r| r.connection_point).map_or(Object::Device(s.component), Object::ConnectionPoint),
                    which,
                    fix: Some(Fix::OfferDrawn(s.component)),
                });
            }
            for (label, t) in &s.twice {
                let row = s.row_of(*t);
                let go = row.and_then(|r| r.go);
                // A level of a multi-level position drawn twice goes to the next free one.
                let multi = row.is_some_and(|r| r.ty.multi() || r.ty.breaks);
                out.push(Check {
                    kind: if multi { Kind::LevelTwice } else { Kind::TerminalBlockTwice },
                    page: go.map(|(p, _)| p),
                    at: go.map(|(_, a)| a),
                    object: Object::ConnectionPoint(*t),
                    which: format!("{}:{label} is drawn twice", s.tag),
                    fix: Some(if multi { Fix::MoveToNextFree(*t) } else { Fix::RenumberStrip(s.component) }),
                });
            }
            for (pos, t) in &s.mixed {
                let row = s.row_of(*t);
                let go = row.and_then(|r| r.go);
                let first = s.rows.iter().find(|r| r.label == *pos).map_or("?", |r| r.ty.word);
                let other = s.other_type_word(*t);
                out.push(Check {
                    kind: Kind::PositionMixed,
                    page: go.map(|(p, _)| p),
                    at: go.map(|(_, a)| a),
                    object: Object::ConnectionPoint(*t),
                    which: format!("{}:{pos} mixes {first} and {other} symbols", s.tag),
                    fix: Some(Fix::MoveToNewPosition(*t)),
                });
            }
            for (n, items) in &s.level_missing {
                let go = s.rows.iter().find(|r| r.go.is_some()).and_then(|r| r.go);
                let first = items.first().cloned().unwrap_or_default();
                let pos = ecad_model::connectivity::jumper_bars::split_level(&first).map(|(p, _)| p.to_string()).unwrap_or_default();
                let levels = s.rows.iter().filter(|r| r.label == pos).count();
                out.push(Check {
                    kind: Kind::LevelMissing,
                    page: go.map(|(p, _)| p),
                    at: go.map(|(_, a)| a),
                    object: Object::Device(s.component),
                    which: format!(
                        "{}: link {n} names {}, but {}:{pos} has {levels} level{}",
                        s.tag,
                        items.join(", "),
                        s.tag,
                        if levels == 1 { "" } else { "s" }
                    ),
                    fix: Some(Fix::LevelMissing(s.component, u16::try_from(*n).unwrap_or(u16::MAX))),
                });
            }
            out.extend(accessory_checks(s));
        }
        // Fuse terminals that are devices of their own, and fuse positions with no
        // rating: one Info row each.
        let fuses = self.strips.fuse_devices.len();
        if fuses > 0 {
            let which = if fuses == 1 {
                "1 fuse terminal is a device of its own".to_string()
            } else {
                format!("{fuses} fuse terminals are devices of their own")
            };
            out.push(Check { kind: Kind::FuseDevice, page: None, at: None, object: Object::None, which, fix: Some(Fix::FuseDevices) });
        }
        let unrated: usize = self.strips.strips.values().map(|s| s.no_rating).sum();
        if unrated > 0 {
            let which = if unrated == 1 { "1 fuse position has no rating".to_string() } else { format!("{unrated} fuse positions have no rating") };
            out.push(Check { kind: Kind::NoFuseRating, page: None, at: None, object: Object::None, which, fix: Some(Fix::ShowReports) });
        }
        for c in &self.jumper_bars_off_strip {
            let tag = self.devices.get(c).map_or_else(|| "?".to_string(), |d| d.tag.clone());
            out.push(Check {
                kind: Kind::JumperBarOnNonStrip,
                page: None,
                at: None,
                object: Object::Device(*c),
                which: format!("{tag} has link keys but is not a terminal strip"),
                fix: Some(Fix::JumperBarOnNonStrip(*c)),
            });
        }
        if self.strips.acc_no_part > 0 {
            let n = self.strips.acc_no_part;
            let which =
                if n == 1 { "1 strip accessory has no part number".to_string() } else { format!("{n} strip accessories have no part number") };
            out.push(Check { kind: Kind::NoAccessoryPart, page: None, at: None, object: Object::None, which, fix: Some(Fix::ShowReports) });
        }
        let mut unknown = 0;
        for r in self.cables.values() {
            let tag = r.tag.clone().unwrap_or_else(|| "?".into());
            let Some(offer) = &r.count.offer else {
                // `cable-not-drawn` covers a cable not drawn yet.
                unknown += usize::from(r.drawn);
                for i in &r.count.twice {
                    out.push(self.core_twice(r, &tag, *i));
                }
                continue;
            };
            if let Some(first) = r.count.over.first().and_then(|i| r.members.get(*i)) {
                let go = self.wire(*first).and_then(|w| w.go);
                let named = match (&r.kind, &r.count.offer_text) {
                    (Some(k), _) => k.clone(),
                    (None, Some(t)) => t.clone(),
                    (None, None) => String::new(),
                };
                out.push(Check {
                    kind: Kind::TooManyCores,
                    page: go.map(|(p, _)| p),
                    at: go.map(|(_, a)| a),
                    object: Object::Wire(*first),
                    which: format!("{tag}: {} cores used, cable offers {} ({named})", r.members.len(), offer.len()),
                    fix: Some(Fix::SetCores(r.cable)),
                });
            }
            for i in &r.count.twice {
                out.push(self.core_twice(r, &tag, *i));
            }
        }
        if unknown > 0 {
            let which = if unknown == 1 { "1 cable has no core count".to_string() } else { format!("{unknown} cables have no core count") };
            out.push(Check { kind: Kind::NoCoreCount, page: None, at: None, object: Object::None, which, fix: Some(Fix::ShowCables) });
        }
        out
    }

    pub(super) fn core_twice(&self, r: &CableRow, tag: &str, i: usize) -> Check {
        let w = r.members.get(i).copied();
        let go = w.and_then(|w| self.wire(w)).and_then(|x| x.go);
        let label = w.and_then(|w| self.wire(w)).map(|x| x.cable.clone()).unwrap_or_default();
        let core = label.rsplit(" core ").next().unwrap_or("?").to_string();
        Check {
            kind: Kind::CoreTwice,
            page: go.map(|(p, _)| p),
            at: go.map(|(_, a)| a),
            object: w.map_or(Object::None, Object::Wire),
            which: format!("{tag}: core {core} is used by two wires"),
            fix: w.map(Fix::NextFreeCore),
        }
    }

    /// The BOM rows of the Checks list, from the kept counts and sets.
    pub fn bom_checks(&self) -> Vec<Check> {
        let mut out = Vec::new();
        let n = self.items_no_part;
        if n > 0 {
            let which = if n == 1 { "1 item has no part number".to_string() } else { format!("{n} items have no part number") };
            out.push(Check { kind: Kind::ItemNoPartNumber, page: None, at: None, object: Object::None, which, fix: Some(Fix::ShowReports) });
        }
        let gaps = self.fuse_gap_count();
        if gaps > 0 {
            let which = if gaps == 1 { "1 fuse has no insert".to_string() } else { format!("{gaps} fuses have no insert") };
            out.push(Check { kind: Kind::FuseNoInsert, page: None, at: None, object: Object::None, which, fix: Some(Fix::ShowReports) });
        }
        for pk in &self.article_conflicts {
            let Some(set) = self.articles.get(pk) else { continue };
            let Some(first) = self.by_part.get(pk).and_then(|s| s.iter().next()).and_then(|(_, _, c, n)| self.entries.get(&(*c, *n))) else {
                continue;
            };
            let mut values: Vec<(&String, &usize)> = set.iter().collect();
            values.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
            let words: Vec<String> = values.iter().map(|(a, n)| format!("{a} ({n})")).collect();
            let pn = first.part_number.clone().unwrap_or_default();
            let count = if values.len() == 2 { "two".to_string() } else { values.len().to_string() };
            out.push(Check {
                kind: Kind::ArticleConflict,
                page: None,
                at: None,
                object: first.owner.device().map_or(Object::None, Object::Device),
                which: format!("{pn} has {count} article numbers: {}", words.join(", ")),
                fix: Some(Fix::UseArticle(first.owner, first.line)),
            });
        }
        for (c, n) in &self.for_missing {
            let Some(e) = self.entries.get(&(Owner::Device(*c), *n)) else { continue };
            let main = self.devices.get(c).and_then(|d| d.goto);
            let what = if self.strips.get(*c).is_some() { "position" } else { "fuse" };
            let names = e.missing.join(", ");
            let plural = if e.missing.len() == 1 { "" } else { "s" };
            out.push(Check {
                kind: Kind::PartForMissing,
                page: main.map(|(_, p, _)| p),
                at: main.and_then(|(_, _, a)| a),
                object: Object::Device(*c),
                which: format!("{} line {n} names {what}{plural} {names}, which {} does not have", e.tag, e.tag),
                fix: Some(Fix::DropMissingFor(*c, *n)),
            });
        }
        for (c, keys) in &self.bad_part_keys {
            let tag = self.devices.get(c).map_or_else(|| "?".to_string(), |d| d.tag.clone());
            let main = self.devices.get(c).and_then(|d| d.goto);
            let first = keys.first().cloned().unwrap_or_default();
            let more = if keys.len() > 1 { format!(" (and {} more)", keys.len() - 1) } else { String::new() };
            let which = format!("{tag} has {first}{more}, which ECAD does not read");
            out.push(Check {
                kind: Kind::BadPartKey,
                page: main.map(|(_, p, _)| p),
                at: main.and_then(|(_, _, a)| a),
                object: Object::Device(*c),
                which,
                fix: None,
            });
        }
        out
    }

    /// Uncovered fuses, counted only once the project lists any fuse insert.
    pub(super) fn fuse_gap_count(&self) -> usize {
        if self.inserts.is_empty() { 0 } else { self.fuse_gaps.values().sum() }
    }

    /// The check rows by severity: (errors, warnings, info), as [`ReportModels::checks`] would give
    /// them. O(1): kept by every change (the badge).
    pub fn check_counts(&self) -> (usize, usize, usize) {
        let info = self.box_counts[1]
            + usize::from(self.strips.acc_no_part > 0)
            + usize::from(self.no_part_number > 0 || self.lines_no_part > 0)
            + usize::from(self.items_no_part > 0)
            + usize::from(self.fuse_gap_count() > 0)
            + self.bad_part_keys.len()
            + usize::from(self.no_location > 0 && self.no_location < self.devices.len())
            + usize::from(self.no_function > 0 && self.no_function < self.devices.len())
            + usize::from(self.cable_checks[2] > 0);
        // The strips' and cables' rows, kept with them.
        let warnings = self.box_counts[0]
            + self.strips.check_warnings
            + self.cable_checks[0]
            + self.cable_checks[1]
            + self.jumper_bars_off_strip.len()
            + self.article_conflicts.len()
            + self.for_missing.len();
        (self.strips.check_errors, warnings, info)
    }

    /// What the check rows depend on besides [`ReportModels::checks_changed`]: the three count rows.
    pub fn checks_key(&self) -> (u64, usize, usize, usize, usize) {
        (self.checks_changed, self.no_part_number, self.no_location, self.no_function, self.devices.len())
    }
}
