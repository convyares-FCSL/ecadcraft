//! The BOM, wire and cable rows the models hold, read by group and by device.

use super::*;

impl ReportModels {
    /// How many sheets the BOM-by-location page needs: from the rows per
    /// location only (O(locations)), never the BOM rows themselves.
    pub fn bom_sheets(&self) -> usize {
        let per = crate::pages::generated::rows_per_sheet();
        let (mut sheets, mut used) = (1, 0);
        // The strips' rows per location, kept with them (O(locations), never the rows).
        // The nested layout's counts are kept too (main rows, then each part line).
        let mut rows = if self.view.nested {
            let mut r = self.loc_nested_rows.clone();
            for (k, n) in &self.loc_lines {
                *r.entry(k.clone()).or_default() += n;
            }
            r
        } else {
            self.loc_rows.clone()
        };
        for (loc, n) in &self.strips.bom_rows {
            *rows.entry(loc_key(loc.as_deref())).or_default() += n;
        }
        // The connectors' half-row groups per location, kept.
        for (k, n) in &self.half_loc_rows {
            *rows.entry(k.clone()).or_default() += n;
        }
        for n in rows.values() {
            if used == per {
                sheets += 1;
                used = 0;
            }
            used += 1;
            let mut left = *n;
            while left > 0 {
                if used == per {
                    sheets += 1;
                    used = 1;
                }
                let take = left.min(per - used);
                used += take;
                left -= take;
            }
        }
        sheets
    }

    /// The `jumper_bar.<n>` keys a device that is not a strip carries (kept per device, read on demand).
    pub fn jumper_bar_keys_off_strip(&self, c: DeviceId) -> Vec<String> {
        if !self.jumper_bars_off_strip.contains(&c) {
            return Vec::new();
        }
        self.jumper_bar_keys.get(&c).cloned().unwrap_or_default()
    }

    /// One device's row.
    pub fn device(&self, c: DeviceId) -> Option<&DeviceRow> {
        self.devices.get(&c)
    }

    /// How many devices.
    pub fn device_count(&self) -> usize {
        self.devices.len()
    }

    /// A strip's BOM rows: one row per terminal type, Qty
    /// in positions (a three-level terminal is one part), in catalogue order.
    pub(super) fn strip_bom(&self, s: &crate::terminal_strips::strip_model::StripData, location: Option<String>) -> Vec<BomRow> {
        let mut out = Vec::new();
        for t in &s.types {
            if t.qty() == 0 {
                continue;
            }
            out.push(BomRow {
                location: location.clone(),
                qty: Qty::count(t.qty()),
                tags: vec![s.tag.clone()],
                devices: vec![s.component],
                description: Some(t.description.clone()),
                maker: t.maker.clone(),
                part_number: t.part_number.clone(),
                article_number: t.article_number.clone(),
                ..BomRow::default()
            });
        }
        // One row per accessory part, only for what is placed.
        for a in &s.acc.bom {
            out.push(BomRow {
                location: location.clone(),
                qty: Qty::count(a.qty),
                tags: vec![s.tag.clone()],
                devices: vec![s.component],
                description: Some(a.description.clone()),
                maker: a.maker.clone(),
                part_number: a.part_number.clone(),
                article_number: a.article_number.clone(),
                accessory: Some(a.accessory),
                ..BomRow::default()
            });
        }
        out
    }

    /// The row a set of entries makes.
    pub(super) fn entry_row(&self, set: &BTreeSet<EntrySort>, location: Option<String>) -> BomRow {
        let es: Vec<&Entry> = set.iter().filter_map(|(_, _, c, n)| self.entries.get(&(*c, *n))).collect();
        let Some(first) = es.first() else { return BomRow { location, ..BomRow::default() } };
        let (mut qty, mut overflow) = (Qty(0), false);
        for e in &es {
            match e.total().and_then(|t| qty.checked_add(t)) {
                Some(q) => qty = q,
                None => overflow = true,
            }
        }
        let mut tags: Vec<String> = es.iter().flat_map(|e| e.tags()).collect();
        if es.iter().any(|e| e.per_sub()) {
            tags.sort_by(|a, b| locations::natural_key(a).cmp(&locations::natural_key(b)).then(a.cmp(b)));
        }
        tags.dedup();
        let mut devices: Vec<DeviceId> = Vec::new();
        for c in es.iter().filter_map(|e| e.owner.device()) {
            if !devices.contains(&c) {
                devices.push(c);
            }
        }
        // The most common article number (ties: the first in tag order); flagged when two differ.
        let mut counts: Vec<(&str, usize, &Entry)> = Vec::new();
        for e in &es {
            let Some(a) = e.article_number.as_deref() else { continue };
            match counts.iter_mut().find(|(x, _, _)| *x == a) {
                Some(c) => c.1 += 1,
                None => counts.push((a, 1, e)),
            }
        }
        let best = counts.iter().enumerate().max_by(|(i, a), (j, b)| a.1.cmp(&b.1).then(j.cmp(i))).map(|(_, x)| *x);
        let article_flag = match (best, counts.iter().find(|x| Some(x.0) != best.map(|b| b.0))) {
            (Some((a, _, ea)), Some((b, _, eb))) => {
                let who = |e: &Entry| if e.is_item() { e.description.clone().unwrap_or_else(|| "An item".into()) } else { e.tag.clone() };
                Some(format!("{} and {} have different article numbers ({a}, {b}).", who(ea), who(eb)))
            }
            _ => None,
        };
        let ratings = rating_cells(&es);
        BomRow {
            location,
            qty,
            metres: first.metres,
            overflow,
            tags,
            description: first.description.clone(),
            maker: first.maker.clone(),
            part_number: first.part_number.clone(),
            article_number: best.map(|b| b.0.to_string()),
            article_flag,
            ratings,
            entries: es.iter().map(|e| (e.owner, e.line)).collect(),
            item: es.iter().all(|e| e.is_item()),
            devices,
            ..BomRow::default()
        }
    }

    /// Nested layout: the rows of `c`'s part lines, unmerged, in line order.
    /// The part lines of connector `c` under its half tagged `tag` (nested layout).
    pub(super) fn line_rows_of_half(&self, c: DeviceId, tag: &str, location: &Option<String>) -> Vec<BomRow> {
        let o = Owner::Device(c);
        let mine: BTreeSet<u8> = self.entries.range((o, 1)..=(o, u8::MAX)).filter(|(_, e)| e.tag == tag).map(|(k, _)| k.1).collect();
        self.line_rows(c, location).into_iter().filter(|r| r.line.is_some_and(|n| mine.contains(&n))).collect()
    }

    pub(super) fn line_rows(&self, c: DeviceId, location: &Option<String>) -> Vec<BomRow> {
        self.entries
            .range((Owner::Device(c), 1)..=(Owner::Device(c), u8::MAX))
            .map(|(_, e)| {
                let (qty, overflow) = e.total().map_or((e.qty, true), |q| (q, false));
                BomRow {
                    location: location.clone(),
                    qty,
                    metres: e.metres,
                    overflow,
                    tags: if e.per_sub() { e.tags() } else { Vec::new() },
                    devices: vec![c],
                    description: e.description.clone(),
                    maker: e.maker.clone(),
                    part_number: e.part_number.clone(),
                    article_number: e.article_number.clone(),
                    entries: vec![(Owner::Device(c), e.line)],
                    line: Some(e.line),
                    kit: e.kit.clone(),
                    ..BomRow::default()
                }
            })
            .collect()
    }

    /// The BOM rows: devices, and each strip per terminal type;
    /// Part lines and items, in the project's layout (`bom.lines`).
    pub fn bom(&self, group: Group) -> Vec<BomRow> {
        self.bom_with(group, self.view.nested)
    }

    /// One wire row per distinct (part number, colour,
    /// cross-section) among wires whose category has a part number; Qty in metres, the wires ×
    /// the category's allowance rounded up to 0.5 m (an estimate); with no allowance, no Qty.
    pub fn wire_bom(&self) -> Vec<BomRow> {
        let mut groups: BTreeMap<(String, String, String), (usize, f64, Option<String>, Option<String>)> = BTreeMap::new();
        for w in self.wires.values() {
            let Some((pn, maker, description, allowance)) = &w.part else { continue };
            let key = (pn.clone(), w.colour.clone().unwrap_or_default(), w.cross_section.clone().unwrap_or_default());
            let g = groups.entry(key).or_insert((0, *allowance, maker.clone(), description.clone()));
            g.0 += 1;
        }
        groups
            .into_iter()
            .map(|((pn, colour, cs), (n, allowance, maker, description))| {
                let metres = (n as f64 * allowance * 2.0).ceil() / 2.0;
                let qty = if metres.is_finite() && metres > 0.0 { Qty((metres * 1000.0).round().min(u64::MAX as f64 / 2.0) as u64) } else { Qty(0) };
                let what = [
                    Some(description.unwrap_or_else(|| "Wire".into())),
                    (!cs.is_empty()).then_some(cs),
                    (!colour.is_empty()).then(|| ecad_model::connectivity::wiring::colour_text(self.colour_style, &colour)),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" ");
                let note = if qty.0 == 0 { format!("{n} wires, no allowance") } else { format!("{n} wires, estimate") };
                BomRow {
                    location: None,
                    qty,
                    metres: true,
                    overflow: false,
                    tags: Vec::new(),
                    devices: Vec::new(),
                    description: Some(format!("{what} ({note})")),
                    maker,
                    part_number: Some(pn),
                    article_number: None,
                    article_flag: None,
                    ratings: Vec::new(),
                    entries: Vec::new(),
                    item: true,
                    line: None,
                    kit: None,
                    accessory: None,
                }
            })
            .collect()
    }

    /// The BOM rows in the flat or nested layout (nested is By location only).
    pub fn bom_with(&self, group: Group, nested: bool) -> Vec<BomRow> {
        let mut rows = self.bom_rows_with(group, nested);
        rows.extend(self.wire_bom());
        rows
    }

    pub(super) fn bom_rows_with(&self, group: Group, nested: bool) -> Vec<BomRow> {
        match group {
            Group::Location => {
                let index = if nested { &self.nested } else { &self.by_location };
                let mut v: Vec<(LocKey, EntrySort, Vec<BomRow>)> = index
                    .iter()
                    .filter_map(|((lk, pk), set)| {
                        let first = set.iter().next()?.clone();
                        let loc = (!lk.0).then(|| lk.2.clone());
                        let mut rows = vec![self.entry_row(set, loc.clone())];
                        // A device row of its own is followed by its lines.
                        if nested && let Some((Owner::Device(c), 0)) = pk.3 {
                            rows.extend(self.line_rows(c, &loc));
                        }
                        Some((lk.clone(), first, rows))
                    })
                    .collect();
                for s in self.strips.strips.values() {
                    let mut rows = self.strip_bom(s, s.location.clone());
                    if nested {
                        rows.extend(self.line_rows(s.component, &s.location));
                    }
                    v.push((loc_key(s.location.as_deref()), (false, tag_key(&s.tag), Owner::Device(s.component), 0), rows));
                }
                // One row per drawn half, grouped like devices; nested,
                // each half's row is followed by its part lines (`part.<n>.half`).
                self.half_groupings.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                for r in group_halves(self.connectors.values().flatten(), Group::Location) {
                    let first = r.tags.first().map_or_else(|| tag_key(""), |t| tag_key(t));
                    let sort = (false, first, Owner::Device(r.devices.first().copied().unwrap_or(DeviceId(0))), 0);
                    let mut rows = Vec::new();
                    if nested && r.part_number.is_none() {
                        for (c, tag) in r.devices.iter().zip(&r.tags) {
                            rows.extend(self.line_rows_of_half(*c, tag, &r.location));
                        }
                    }
                    rows.insert(0, r.clone());
                    v.push((loc_key(r.location.as_deref()), sort, rows));
                }
                v.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
                v.into_iter().flat_map(|(_, _, r)| r).collect()
            }
            Group::Part => {
                let mut v: Vec<(EntrySort, BomRow)> = self
                    .by_part
                    .values()
                    .filter_map(|set| {
                        let first = set.iter().next()?.clone();
                        let mut counts: BTreeMap<LocKey, usize> = BTreeMap::new();
                        for (_, _, c, n) in set {
                            if let Some(e) = self.entries.get(&(*c, *n)) {
                                *counts.entry(e.loc_key()).or_default() += 1;
                            }
                        }
                        let locs: Vec<String> =
                            counts.iter().map(|(k, n)| format!("{} \u{d7}{n}", if k.0 { "?".to_string() } else { k.2.clone() })).collect();
                        Some((first, self.entry_row(set, Some(locs.join(", ")))))
                    })
                    .collect();
                for s in self.strips.strips.values() {
                    let loc = s.location.clone().unwrap_or_else(|| "?".into());
                    for r in self.strip_bom(s, Some(format!("{loc} \u{d7}1"))) {
                        v.push(((false, tag_key(&s.tag), Owner::Device(s.component), 0), r));
                    }
                }
                // Half rows grouped by part, their locations counted.
                self.half_groupings.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                for r in group_halves(self.connectors.values().flatten(), Group::Part) {
                    let first = r.tags.first().map_or_else(|| tag_key(""), |t| tag_key(t));
                    v.push(((false, first, Owner::Device(r.devices.first().copied().unwrap_or(DeviceId(0))), 0), r));
                }
                v.sort_by(|a, b| a.0.cmp(&b.0));
                v.into_iter().map(|(_, r)| r).collect()
            }
        }
    }

    /// One BOM entry.
    pub fn entry(&self, c: Owner, line: u8) -> Option<&Entry> {
        self.entries.get(&(c, line))
    }

    /// The entries of device `c` (main part first, then its part lines).
    pub fn entries_of(&self, c: DeviceId) -> impl Iterator<Item = &Entry> {
        let o = Owner::Device(c);
        self.entries.range((o, 0)..=(o, u8::MAX)).map(|(_, e)| e)
    }

    /// The article numbers of part key of entry `(c, line)`, most common first.
    pub fn article_values(&self, c: Owner, line: u8) -> Vec<(String, usize)> {
        let Some(pk) = self.entries.get(&(c, line)).map(Entry::part_key) else { return Vec::new() };
        let mut v: Vec<(String, usize)> = self.articles.get(&pk).map(|m| m.iter().map(|(a, n)| (a.clone(), *n)).collect()).unwrap_or_default();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v
    }

    /// The wire rows, sorted by wire number naturally, then conductor ID.
    pub fn wires(&self) -> Vec<&WireRow> {
        let mut v: Vec<&WireRow> = self.wires.values().collect();
        v.sort_by(|a, b| {
            let ka = loc_key(a.wire_number.as_deref());
            let kb = loc_key(b.wire_number.as_deref());
            ka.cmp(&kb).then(a.conductor.cmp(&b.conductor))
        });
        v
    }

    /// The cable rows, sorted by tag naturally.
    pub fn cables(&self) -> Vec<&CableRow> {
        self.sorts.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut v: Vec<&CableRow> = self.cables.values().collect();
        v.sort_by(|a, b| loc_key(a.tag.as_deref()).cmp(&loc_key(b.tag.as_deref())).then(a.cable.cmp(&b.cable)));
        v
    }

    /// One wire's row.
    pub fn wire(&self, w: WireId) -> Option<&WireRow> {
        self.wires.get(&w)
    }
}

/// Per rating field word the entries `es` have: the most common value (ties: the first in tag
/// order), flagged when two differ, as the article number is.
fn rating_cells(es: &[&Entry]) -> Vec<(&'static str, String, Option<String>)> {
    let mut words: Vec<&'static str> = Vec::new();
    for (w, _) in es.iter().flat_map(|e| e.ratings.iter()) {
        if !words.contains(w) {
            words.push(w);
        }
    }
    words
        .into_iter()
        .filter_map(|w| {
            let mut counts: Vec<(&str, usize, &Entry)> = Vec::new();
            for e in es {
                let Some((_, v)) = e.ratings.iter().find(|(x, _)| *x == w) else { continue };
                match counts.iter_mut().find(|(x, _, _)| *x == v.as_str()) {
                    Some(c) => c.1 += 1,
                    None => counts.push((v.as_str(), 1, e)),
                }
            }
            let (best, _, eb) = *counts.iter().enumerate().max_by(|(i, a), (j, b)| a.1.cmp(&b.1).then(j.cmp(i))).map(|(_, x)| x)?;
            let flag = counts.iter().find(|x| x.0 != best).map(|(other, _, eo)| {
                let label = crate::reports::bom::rating_column_label(w).to_lowercase();
                format!("{} and {} have a different {label} ({best}, {other}).", eb.tag, eo.tag)
            });
            Some((w, best.to_string(), flag))
        })
        .collect()
}
