//! The report tables: the BOM, the wire and cable lists, the terminal block plan and the core table.

use super::*;

impl ReportModels {
    /// The BOM as a table (`Location` or `Locations` first), as the window shows it, in the
    /// project's report settings.
    pub fn bom_table(&self, group: Group) -> Table {
        self.bom_table_with(group, self.view, false)
    }

    /// The BOM as a table in `view`. `csv` (design 047 §6.4): `Qty` a plain number and a `Unit`
    /// column, a `Line` column in the nested layout; otherwise `Qty` reads `5.6 m`.
    pub fn bom_table_with(&self, group: Group, view: BomView, csv: bool) -> Table {
        let first = if group == Group::Location { "Location" } else { "Locations" };
        let nested = view.nested && group == Group::Location;
        let mut headers = vec![first, "Qty"];
        if csv {
            headers.push("Unit");
        }
        headers.push("Tags");
        if csv && nested {
            headers.push("Line");
        }
        headers.extend(["Description", "Maker", "Part number"]);
        if view.article {
            headers.push("Article number");
        }
        let mut t = Table { headers, rows: Vec::new() };
        for r in self.bom_with(group, nested) {
            let who = self.row_who(&r);
            let mut cells = vec![Cell::opt(&r.location, || format!("{who} has no location."))];
            let qty = if r.no_qty() {
                String::new()
            } else if csv {
                r.qty.to_string()
            } else {
                r.qty_text()
            };
            cells.push(if r.overflow { Cell { text: qty, flag: Some("The quantity is too large to add up.".into()) } } else { Cell::ok(qty) });
            if csv {
                cells.push(Cell::ok(if r.metres { "m" } else { "pcs" }));
            }
            cells.push(Cell::ok(r.tags.join(", ")));
            if csv && nested {
                // `0` a main part (or a strip's own row), `1`… a line, blank for an item.
                let line = match (r.line, r.item) {
                    (Some(n), _) => n.to_string(),
                    (None, true) => String::new(),
                    (None, false) => "0".into(),
                };
                cells.push(Cell::ok(line));
            }
            let description = if csv { r.description.clone() } else { r.description_text() };
            cells.push(Cell::opt(&description, || format!("{who} has no description.")));
            cells.push(Cell::opt(&r.maker, || format!("{who} has no maker.")));
            cells.push(Cell::opt(&r.part_number, || format!("{who} has no part number.")));
            if view.article {
                let a = r.article_number.clone().unwrap_or_default();
                cells.push(match &r.article_flag {
                    Some(why) => Cell { text: if a.is_empty() { "?".into() } else { a }, flag: Some(why.clone()) },
                    None => Cell::ok(a),
                });
            }
            t.rows.push(cells);
        }
        t
    }

    /// Who a row's `?` hover names: the first tag, `Line 2 of -K03` for a nested line, or the item.
    pub(super) fn row_who(&self, r: &BomRow) -> String {
        if let (Some(n), Some(c)) = (r.line, r.devices.first()) {
            let tag = self.devices.get(c).map_or("?", |d| d.tag.as_str());
            return format!("Line {n} of {tag}");
        }
        match r.tags.first() {
            Some(t) => t.clone(),
            None if r.item => format!("The item {}", r.description.as_deref().unwrap_or("")).trim_end().to_string(),
            None => String::new(),
        }
    }

    /// The wire report as a table.
    pub fn wire_table(&self) -> Table {
        let mut t = Table {
            headers: vec![
                "Wire number",
                "From",
                "From location",
                "To",
                "To location",
                "Cross-section",
                "Colour",
                "Potential",
                "Category",
                "Cable",
                "Page",
            ],
            rows: Vec::new(),
        };
        for r in self.wires() {
            let style = self.style;
            let end_loc = |e: &EndCell| match (&e.device, &e.location) {
                (Some(_), None) => Cell { text: "?".into(), flag: Some(format!("{} has no location.", e.text(style).unwrap_or_default())) },
                (_, Some(l)) => Cell::ok(l.clone()),
                (None, None) => Cell::default(),
            };
            t.rows.push(vec![
                Cell::opt(&r.wire_number, || "The wire has no number.".into()),
                Cell::opt(&r.from.text(style), || "Loose end.".into()),
                end_loc(&r.from),
                Cell::opt(&r.to.text(style), || "Loose end.".into()),
                end_loc(&r.to),
                // Brief 050b (design 050 §5.6): the wire's own value or its category's.
                Cell::opt(&r.cross_section, || "No cross-section, and its category gives none.".into()),
                Cell::opt(&r.colour.as_deref().map(|c| ecad_model::connectivity::wiring::colour_text(self.colour_style, c)), || {
                    "No colour, and its category gives none.".into()
                }),
                Cell::ok(r.potential.clone().unwrap_or_default()),
                Cell::ok(r.category.as_ref().map(|c| c.1.clone()).unwrap_or_default()),
                Cell::ok(r.cable.clone()),
                Cell::opt(&r.page.as_ref().map(|p| p.text(style)), || "The wire has no route.".into()),
            ]);
        }
        t
    }

    /// The cable report as a table; `Maker`, `Length` and `Designation` follow when any cable
    /// has them. Brief 059: cables not drawn are listed (design 059 §4.5, Q3); see
    /// [`ReportModels::cable_table_with`].
    pub fn cable_table(&self) -> Table {
        self.cable_table_with(true)
    }

    /// The optional columns of the cable report, in order: `Maker`, `Length` and `Designation`
    /// when any listed cable has them (brief 059, design 059 §4.5), then `Glands`, `Shield`,
    /// `Planned from` and `Planned to` when any has them, and `Drawn` when one is not drawn.
    pub fn cable_columns(&self, include_not_drawn: bool) -> Vec<CableColumn> {
        let mut rows = self.cables();
        if !include_not_drawn {
            rows.retain(|r| r.drawn);
        }
        CableColumn::ALL.into_iter().filter(|c| c.shown(&rows)).collect()
    }

    /// The cable report, with or without the cables not drawn yet (brief 059, Q3: the choice made
    /// when the report is generated). Its optional columns are [`ReportModels::cable_columns`]; a
    /// planned end reads `(planned)`.
    pub fn cable_table_with(&self, include_not_drawn: bool) -> Table {
        let cols = self.cable_columns(include_not_drawn);
        let mut rows = self.cables();
        if !include_not_drawn {
            rows.retain(|r| r.drawn);
        }
        let mut t = Table { headers: vec!["Tag", "Type", "Cores", "From location", "To location", "Used", "Spare", "From", "To"], rows: Vec::new() };
        t.headers.extend(cols.iter().map(|c| c.header()));
        for r in rows {
            let why = r.location_reason.clone();
            let loc = |v: &Option<String>| match v {
                Some(l) => Cell::ok(l.clone()),
                None => Cell { text: "?".into(), flag: Some(why.clone().unwrap_or_default()) },
            };
            let mut row = vec![
                Cell::opt(&r.tag, || "The cable has no tag.".into()),
                Cell::opt(&r.kind, || "The cable has no type.".into()),
                Cell::ok(r.core_total().to_string()),
                loc(&r.from_location),
                loc(&r.to_location),
                Cell::ok(r.count.used().to_string()),
                Cell::opt(&r.count.spare_text(), || "Set the cable's cores or type (7G1.5) to count spares.".into()),
                Cell::ok(ends_text(&r.from_location, &r.from_devices)),
                Cell::ok(ends_text(&r.to_location, &r.to_devices)),
            ];
            row.extend(cols.iter().map(|c| c.cell(r)));
            t.rows.push(row);
        }
        t
    }
}

impl ReportModels {
    /// The terminal plan as a table (Reports ▸ Terminals, CSV): every strip, or `strip` only;
    /// one line per position and conductor (design 028 §5.1), with where each line goes.
    pub fn terminal_block_table(&self, strip: Option<DeviceId>) -> (Table, Vec<Option<(PageId, Pt)>>) {
        self.terminal_block_table_with(strip, true)
    }

    /// [`ReportModels::terminal_block_table`], with or without the accessory gap rows (brief 030: the
    /// Terminals tab's Accessories check box; view only).
    pub fn terminal_block_table_with(&self, strip: Option<DeviceId>, accessories: bool) -> (Table, Vec<Option<(PageId, Pt)>>) {
        let style = self.style;
        let mut t = Table {
            headers: vec!["Strip", "Inside target", "Inside wire", "Terminal", "Bridge", "Outside wire", "Cable", "Core", "Outside target", "Place"],
            rows: Vec::new(),
        };
        let mut go = Vec::new();
        let strips: Vec<&crate::terminal_strips::StripData> = match strip {
            Some(c) => self.strips.get(c).into_iter().collect(),
            None => self.strips.sorted(),
        };
        for s in strips {
            for item in s.plan(accessories) {
                let (ri, r) = match item {
                    crate::terminal_strips::PlanItem::Term(i, r) => (i, r),
                    crate::terminal_strips::PlanItem::Gap { gap, acc } => {
                        let pe_before = gap_pe(s, gap);
                        let words = if acc == crate::terminal_strips::accessories::Acc::StripLabel {
                            format!("strip label {}", s.tag)
                        } else {
                            acc.words().to_string()
                        };
                        let mut row = vec![Cell::ok(s.tag.clone()), Cell::ok(words), Cell::default(), Cell::ok(acc.mark())];
                        row.extend(std::iter::repeat_n(Cell::default(), 5));
                        row.push(Cell::opt(&s.acc.gap_part(acc, pe_before).cloned(), || "No part number for this accessory.".into()));
                        t.rows.push(row);
                        go.push(None);
                        continue;
                    }
                };
                let marker = if s.acc.markers.contains(&r.label) { " \u{25ad}" } else { "" };
                // Brief 036: a PE mark only on a one-level PE terminal (an L/N/PE level shows `1.PE`).
                let pe_mark = r.pe && !r.ty.multi();
                if r.connection_point.is_none() {
                    let mut row = vec![Cell::ok(s.tag.clone()), Cell::ok(r.state()), Cell::default()];
                    row.push(Cell::ok(if pe_mark { format!("{} PE{marker}", r.shown) } else { format!("{}{marker}", r.shown) }));
                    row.push(Cell::ok(s.bridge_cell(ri)));
                    row.extend(std::iter::repeat_n(Cell::default(), 5));
                    t.rows.push(row);
                    go.push(None);
                    continue;
                }
                let lines = r.inside.len().max(r.outside.len()).max(1);
                for i in 0..lines {
                    let (inside, outside) = (r.inside.get(i), r.outside.get(i));
                    let side = |c: Option<&crate::terminal_strips::SideCell>,
                                f: fn(&crate::terminal_strips::SideCell) -> Option<String>,
                                why: &str| match c {
                        None => Cell::default(),
                        Some(c) => Cell::opt(&f(c), || why.to_string()),
                    };
                    let label = if i == 0 { format!("{}{marker}", r.shown) } else { format!("{}\u{2032}", r.shown) };
                    let cable = outside.and_then(|c| c.cable.as_ref());
                    t.rows.push(vec![
                        Cell::ok(s.tag.clone()),
                        side(inside, |c| c.target.clone(), "A loose far end."),
                        side(inside, |c| c.wire.clone(), "The wire has no number."),
                        Cell::ok(if pe_mark { format!("{label} PE") } else { label }),
                        Cell::ok(if i == 0 { s.bridge_cell(ri) } else { String::new() }),
                        side(outside, |c| c.wire.clone(), "The wire has no number."),
                        Cell::ok(cable.map_or_else(String::new, |(_, t)| t.clone().unwrap_or_else(|| "?".into()))),
                        match (cable, outside) {
                            (Some(_), Some(c)) => Cell::opt(&c.core, || "The core has no label.".into()),
                            _ => Cell::default(),
                        },
                        side(outside, |c| c.target.clone(), "A loose far end."),
                        Cell::ok(if i == 0 { r.place.as_ref().map(|p| p.text(style)).unwrap_or_default() } else { String::new() }),
                    ]);
                    go.push(r.go);
                }
            }
        }
        (t, go)
    }

    /// The terminal plan as the release writes it (027's `terminal-block-plan` slot): one line per
    /// position and conductor of every strip in tag order, columns named by `stripplan`'s keys
    /// (design 028 §10); an absent value is an empty cell.
    pub fn terminal_block_plan_csv(&self) -> Table {
        let style = self.style;
        let mut t = Table {
            headers: vec![
                "tag",
                "location",
                "offered",
                "label",
                "type",
                "state",
                "inside_target",
                "inside_wire",
                "bridge_group",
                "outside_wire",
                "cable",
                "core",
                "outside_target",
                "place",
                // Brief 030 (design 030 §9.3).
                "kind",
                "accessory",
                "gap",
                "marker",
                "jumper_bars",
            ],
            rows: Vec::new(),
        };
        let opt = |v: Option<&String>| Cell::ok(v.cloned().unwrap_or_default());
        for s in self.strips.sorted() {
            for item in s.plan(true) {
                let r = match item {
                    crate::terminal_strips::PlanItem::Term(_, r) => r,
                    crate::terminal_strips::PlanItem::Gap { gap, acc } => {
                        let mut row =
                            vec![Cell::ok(s.tag.clone()), opt(s.location.as_ref()), Cell::ok(s.offer.map(|o| o.text()).unwrap_or_default())];
                        row.extend([Cell::default(), Cell::default(), Cell::ok("accessory")]);
                        row.extend(std::iter::repeat_n(Cell::default(), 7));
                        row.push(Cell::ok(s.acc.gap_part(acc, gap_pe(s, gap)).cloned().unwrap_or_else(|| "?".into())));
                        row.extend([Cell::ok("accessory"), Cell::ok(acc.key()), Cell::ok(gap), Cell::default(), Cell::default()]);
                        t.rows.push(row);
                        continue;
                    }
                };
                let marker = if s.acc.markers.contains(&r.label) { "yes" } else { "" };
                let jumper_bars: Vec<&str> = s.acc.jumper_bars.iter().filter(|l| l.labels.contains(&r.label)).map(|l| l.text.as_str()).collect();
                let lines = r.inside.len().max(r.outside.len()).max(1);
                for i in 0..lines {
                    let (inside, outside) = (r.inside.get(i), r.outside.get(i));
                    let label = if i == 0 { r.shown.clone() } else { format!("{}\u{2032}", r.shown) };
                    t.rows.push(vec![
                        Cell::ok(s.tag.clone()),
                        opt(s.location.as_ref()),
                        Cell::ok(s.offer.map(|o| o.text()).unwrap_or_default()),
                        Cell::ok(label),
                        Cell::ok(match r.ty.code {
                            "ft" => "feed-through",
                            "pe" => "pe",
                            c => c,
                        }),
                        Cell::ok(r.state()),
                        opt(inside.and_then(|c| c.target.as_ref())),
                        opt(inside.and_then(|c| c.wire.as_ref())),
                        Cell::ok(r.bridge_group.filter(|_| i == 0).map(|g| g.to_string()).unwrap_or_default()),
                        opt(outside.and_then(|c| c.wire.as_ref())),
                        Cell::ok(outside.and_then(|c| c.cable.as_ref()).map(|(_, t)| t.clone().unwrap_or_else(|| "?".into())).unwrap_or_default()),
                        opt(outside.and_then(|c| c.core.as_ref())),
                        opt(outside.and_then(|c| c.target.as_ref())),
                        Cell::ok(if i == 0 { r.place.as_ref().map(|p| p.text(style)).unwrap_or_default() } else { String::new() }),
                        Cell::ok("terminal_block"),
                        Cell::default(),
                        Cell::default(),
                        Cell::ok(marker),
                        Cell::ok(jumper_bars.join("; ")),
                    ]);
                }
            }
        }
        t
    }

    /// The cable schedule as the release writes it (027's `cable-schedule` slot): every cable in
    /// tag order with its cores (`cablecores`' keys: `label`, `state`, `wire`, `from`, `to`), an
    /// over core after the offered ones; an absent value is an empty cell.
    pub fn cable_schedule_csv(&self) -> Table {
        let mut t = Table { headers: vec!["cable", "type", "offer", "label", "state", "wire", "from", "to"], rows: Vec::new() };
        for r in self.cables() {
            let head = [
                Cell::ok(r.tag.clone().unwrap_or_default()),
                Cell::ok(r.kind.clone().unwrap_or_default()),
                Cell::ok(r.count.offer_text.clone().unwrap_or_default()),
            ];
            for row in self.core_table(r.cable).rows {
                let mut line = head.to_vec();
                // core_table: Core, Wire number, From, To, State; a spare core has no From.
                let get = |i: usize| row.get(i).map(|c| c.text.clone()).unwrap_or_default();
                let state = get(4);
                let from = if state == "spare" { String::new() } else { get(2) };
                line.extend([Cell::ok(get(0)), Cell::ok(state), Cell::ok(get(1)), Cell::ok(from), Cell::ok(get(3))]);
                t.rows.push(line);
            }
        }
        t
    }

    /// Brief 032 §3.5: the From and To of core `i` of cable row `r` (a core chain runs from its
    /// first member's outer end to its last member's), in reference style `style`.
    pub fn chain_ends(&self, r: &CableRow, i: usize, style: ecad_model::ReferenceStyle) -> (Option<String>, Option<String>) {
        let w = r.members.get(i).and_then(|w| self.wires.get(w));
        let tail = r.tails.get(i).filter(|t| Some(**t) != r.members.get(i).copied()).and_then(|t| self.wires.get(t));
        match (w, tail) {
            (Some(h), Some(t)) => {
                let outer = |x: &WireRow, first: bool| match (x.from.continuation, x.to.continuation) {
                    (true, false) => x.to.text(style),
                    (false, true) => x.from.text(style),
                    _ if first => x.from.text(style),
                    _ => x.to.text(style),
                };
                (outer(h, true), outer(t, false))
            }
            _ => (w.and_then(|w| w.from.text(style)), w.and_then(|w| w.to.text(style))),
        }
    }

    /// One cable's core list (design 028 §6.3): core, wire number, from, to, state; spare cores
    /// show `spare` in From.
    pub fn core_table(&self, k: CableId) -> Table {
        let mut t = Table { headers: vec!["Core", "Wire number", "From", "To", "State"], rows: Vec::new() };
        let Some(r) = self.cables.get(&k) else { return t };
        let style = self.style;
        let member = |i: usize, label: Option<String>, state: &str| {
            let w = r.members.get(i).and_then(|w| self.wires.get(w));
            let (from, to) = self.chain_ends(r, i, style);
            vec![
                Cell::opt(&label, || "The core has no label.".into()),
                Cell::ok(w.and_then(|w| w.wire_number.clone()).unwrap_or_default()),
                Cell::ok(from.unwrap_or_default()),
                Cell::ok(to.unwrap_or_default()),
                Cell::ok(state),
            ]
        };
        match &r.count.offer {
            Some(offer) => {
                for label in offer {
                    match r.count.matched.iter().position(|m| m.as_deref() == Some(label.as_str())) {
                        Some(i) => t.rows.push(member(i, Some(label.clone()), "used")),
                        None => t.rows.push(vec![Cell::ok(label.clone()), Cell::default(), Cell::ok("spare"), Cell::default(), Cell::ok("spare")]),
                    }
                }
                for i in &r.count.over {
                    t.rows.push(member(*i, r.member_cores.get(*i).cloned().flatten(), "over"));
                }
            }
            None => {
                for i in 0..r.members.len() {
                    t.rows.push(member(i, r.member_cores.get(i).cloned().flatten(), "used"));
                }
            }
        }
        t
    }
}
