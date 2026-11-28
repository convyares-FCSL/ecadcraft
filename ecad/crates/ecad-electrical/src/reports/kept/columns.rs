//! The columns of the wire, cable and BOM reports, one type per report. Each column says its header (the
//! table, the CSV and the app read it), its key in the agent's JSON rows (its word) and its cell of a
//! row, so a table's headers, cells and keys all come from one list of columns ([`crate::reports::Table::of_columns`],
//! [`crate::reports::rows_json`]). A report that shows only some columns (the cable report's optional ones,
//! the BOM's settings, a sheet of a paged report) picks them from that list.

use std::borrow::Cow;

use ecad_model::Word;

use super::ReportModels;
use crate::reports::{BomRow, BomView, CableRow, Cell, EndCell, Group, ReportColumn, WireRow, ends_text};

ecad_model::word_enum! {
    /// A column of the wire report, by what it shows. Its word is its key in the agent's rows.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum WireColumn {
        WireNumber,
        From,
        FromLocation,
        To,
        ToLocation,
        CrossSection,
        Colour,
        Potential,
        Category,
        Cable,
        Page,
    }
}

impl WireColumn {
    /// The column's header.
    pub fn header(self) -> &'static str {
        match self {
            WireColumn::WireNumber => "Wire number",
            WireColumn::From => "From",
            WireColumn::FromLocation => "From location",
            WireColumn::To => "To",
            WireColumn::ToLocation => "To location",
            WireColumn::CrossSection => "Cross-section",
            WireColumn::Colour => "Colour",
            WireColumn::Potential => "Potential",
            WireColumn::Category => "Category",
            WireColumn::Cable => "Cable",
            WireColumn::Page => "Page",
        }
    }

    /// The cell of wire row `r`.
    pub(crate) fn cell(self, m: &ReportModels, r: &WireRow) -> Cell {
        let style = m.style;
        let end_loc = |e: &EndCell| match (&e.device, &e.location) {
            (Some(_), None) => Cell { text: "?".into(), flag: Some(format!("{} has no location.", e.text(style).unwrap_or_default())) },
            (_, Some(l)) => Cell::ok(l.clone()),
            (None, None) => Cell::default(),
        };
        match self {
            WireColumn::WireNumber => Cell::opt(&r.wire_number, || "The wire has no number.".into()),
            WireColumn::From => Cell::opt(&r.from.text(style), || "Loose end.".into()),
            WireColumn::FromLocation => end_loc(&r.from),
            WireColumn::To => Cell::opt(&r.to.text(style), || "Loose end.".into()),
            WireColumn::ToLocation => end_loc(&r.to),
            // The wire's own value or its category's.
            WireColumn::CrossSection => Cell::opt(&r.cross_section, || "No cross-section, and its category gives none.".into()),
            WireColumn::Colour => Cell::opt(&r.colour.as_deref().map(|c| ecad_model::connectivity::wiring::colour_text(m.colour_style, c)), || {
                "No colour, and its category gives none.".into()
            }),
            WireColumn::Potential => Cell::ok(r.potential.clone().unwrap_or_default()),
            WireColumn::Category => Cell::ok(r.category.as_ref().map(|c| c.1.clone()).unwrap_or_default()),
            WireColumn::Cable => Cell::ok(r.cable.clone()),
            WireColumn::Page => Cell::opt(&r.page.as_ref().map(|p| p.text(style)), || "The wire has no route.".into()),
        }
    }
}

impl ReportColumn for WireColumn {
    fn header(self) -> &'static str {
        WireColumn::header(self)
    }
    fn key(self) -> Cow<'static, str> {
        Cow::Borrowed(self.word())
    }
}

ecad_model::word_enum! {
    /// A column of the cable report, by what it shows: the nine every report has (`Tag` to `To`), then the
    /// optional ones, shown when a listed cable has something for them ([`ReportModels::cable_columns`]).
    /// Its word is its key in the agent's rows; `used_text` and `spare_text` are the cells as text, beside
    /// the counts `used` and `spare` each row gives.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum CableColumn {
        Tag,
        Type,
        Cores,
        FromLocation,
        ToLocation,
        #[strum(serialize = "used_text")]
        Used,
        #[strum(serialize = "spare_text")]
        Spare,
        From,
        To,
        Maker,
        Length,
        Designation,
        Glands,
        Shield,
        PlannedFrom,
        PlannedTo,
        Drawn,
    }
}

impl CableColumn {
    /// The column's header.
    pub fn header(self) -> &'static str {
        match self {
            CableColumn::Tag => "Tag",
            CableColumn::Type => "Type",
            CableColumn::Cores => "Cores",
            CableColumn::FromLocation => "From location",
            CableColumn::ToLocation => "To location",
            CableColumn::Used => "Used",
            CableColumn::Spare => "Spare",
            CableColumn::From => "From",
            CableColumn::To => "To",
            CableColumn::Maker => "Maker",
            CableColumn::Length => "Length",
            CableColumn::Designation => "Designation",
            CableColumn::Glands => "Glands",
            CableColumn::Shield => "Shield",
            CableColumn::PlannedFrom => "Planned from",
            CableColumn::PlannedTo => "Planned to",
            CableColumn::Drawn => "Drawn",
        }
    }

    /// Whether the report shows this column for `rows`: every report has the nine fixed ones; an optional
    /// one when any of `rows` has something to show in it.
    pub(crate) fn shown(self, rows: &[&CableRow]) -> bool {
        match self {
            CableColumn::Tag
            | CableColumn::Type
            | CableColumn::Cores
            | CableColumn::FromLocation
            | CableColumn::ToLocation
            | CableColumn::Used
            | CableColumn::Spare
            | CableColumn::From
            | CableColumn::To => true,
            CableColumn::Maker => rows.iter().any(|r| r.maker.is_some()),
            CableColumn::Length => rows.iter().any(|r| r.length.is_some()),
            CableColumn::Designation => rows.iter().any(|r| r.designation.is_some()),
            CableColumn::Glands => rows.iter().any(|r| r.glands.iter().any(Option::is_some)),
            CableColumn::Shield => rows.iter().any(|r| r.shield.is_some()),
            CableColumn::PlannedFrom | CableColumn::PlannedTo => rows.iter().any(|r| r.planned.iter().any(Option::is_some)),
            CableColumn::Drawn => rows.iter().any(|r| !r.drawn),
        }
    }

    /// The cell of cable row `r`. A planned end reads `(planned)`.
    pub(crate) fn cell(self, r: &CableRow) -> Cell {
        let loc = |v: &Option<String>| match v {
            Some(l) => Cell::ok(l.clone()),
            None => Cell { text: "?".into(), flag: Some(r.location_reason.clone().unwrap_or_default()) },
        };
        let dash = |v: &Option<String>| v.clone().unwrap_or_else(|| "\u{2014}".into());
        let planned = |i: usize| r.planned[i].as_ref().map(|p| format!("{p} (planned)")).unwrap_or_default();
        match self {
            CableColumn::Tag => Cell::opt(&r.tag, || "The cable has no tag.".into()),
            CableColumn::Type => Cell::opt(&r.kind, || "The cable has no type.".into()),
            CableColumn::Cores => Cell::ok(r.core_total().to_string()),
            CableColumn::FromLocation => loc(&r.from_location),
            CableColumn::ToLocation => loc(&r.to_location),
            CableColumn::Used => Cell::ok(r.count.used().to_string()),
            CableColumn::Spare => Cell::opt(&r.count.spare_text(), || "Set the cable's cores or type (7G1.5) to count spares.".into()),
            CableColumn::From => Cell::ok(ends_text(&r.from_location, &r.from_devices)),
            CableColumn::To => Cell::ok(ends_text(&r.to_location, &r.to_devices)),
            CableColumn::Maker => Cell::ok(r.maker.clone().unwrap_or_default()),
            CableColumn::Length => Cell::ok(r.length.clone().unwrap_or_default()),
            CableColumn::Designation => Cell::ok(r.designation.clone().unwrap_or_default()),
            CableColumn::Glands => Cell::ok(format!("{}, {}", dash(&r.glands[0]), dash(&r.glands[1]))),
            CableColumn::Shield => Cell::ok(r.shield.clone().unwrap_or_default()),
            CableColumn::PlannedFrom => Cell::ok(planned(0)),
            CableColumn::PlannedTo => Cell::ok(planned(1)),
            CableColumn::Drawn => Cell::ok(if r.drawn { "yes" } else { "no" }),
        }
    }
}

impl ReportColumn for CableColumn {
    fn header(self) -> &'static str {
        CableColumn::header(self)
    }
    fn key(self) -> Cow<'static, str> {
        Cow::Borrowed(self.word())
    }
    fn number(self) -> bool {
        self == CableColumn::Cores
    }
}

ecad_model::word_enum! {
    /// A column of a cable's core table.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum CoreColumn {
        Core,
        WireNumber,
        From,
        To,
        State,
    }
}

impl CoreColumn {
    /// The column's header.
    pub fn header(self) -> &'static str {
        match self {
            CoreColumn::Core => "Core",
            CoreColumn::WireNumber => "Wire number",
            CoreColumn::From => "From",
            CoreColumn::To => "To",
            CoreColumn::State => "State",
        }
    }

    /// The headers of every column, in order.
    pub fn headers() -> Vec<&'static str> {
        Self::ALL.iter().map(|c| c.header()).collect()
    }
}

ecad_model::word_enum! {
    /// A BOM column other than a rating, by what it shows, in the order the BOM shows them. Its word is its
    /// key in the agent's rows.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum BomField {
        /// By location: the row's location.
        Location,
        /// By part: the locations the part is in, with how many in each.
        Locations,
        Qty,
        /// `m` or `pcs` (the CSV, where `Qty` is a plain number).
        Unit,
        Tags,
        /// The CSV of the nested layout: `0` a main part, `1`… a part line, blank for an item.
        Line,
        Description,
        Maker,
        PartNumber,
        ArticleNumber,
    }
}

impl BomField {
    /// The column's header.
    pub fn header(self) -> &'static str {
        match self {
            BomField::Location => "Location",
            BomField::Locations => "Locations",
            BomField::Qty => "Qty",
            BomField::Unit => "Unit",
            BomField::Tags => "Tags",
            BomField::Line => "Line",
            BomField::Description => "Description",
            BomField::Maker => "Maker",
            BomField::PartNumber => "Part number",
            BomField::ArticleNumber => "Article number",
        }
    }

    /// Whether a BOM grouped by `group` in `view` shows this column; `csv`: the CSV form, whose `Qty` is a
    /// plain number beside a `Unit`, with a `Line` in the nested layout.
    fn shown(self, group: Group, view: &BomView, csv: bool) -> bool {
        match self {
            BomField::Location => group == Group::Location,
            BomField::Locations => group == Group::Part,
            BomField::Unit => csv,
            BomField::Line => csv && view.nested && group == Group::Location,
            BomField::ArticleNumber => view.article,
            BomField::Qty | BomField::Tags | BomField::Description | BomField::Maker | BomField::PartNumber => true,
        }
    }
}

/// A column of the BOM: a [`BomField`], or a rating column (a rating field word, `current`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BomColumn {
    Field(BomField),
    Rating(&'static str),
}

impl BomColumn {
    /// The columns of a BOM grouped by `group` in `view` (`csv`: the CSV form), in order: the fields it shows,
    /// then its rating columns.
    pub fn columns(group: Group, view: &BomView, csv: bool) -> Vec<BomColumn> {
        let fields = BomField::ALL.iter().copied().filter(|f| f.shown(group, view, csv)).map(BomColumn::Field);
        fields.chain(view.ratings.iter().map(|w| BomColumn::Rating(w))).collect()
    }

    /// The cell of BOM row `r`; `who` names the row in a `?`'s hover; `csv` the CSV form.
    pub(crate) fn cell(self, r: &BomRow, who: &str, csv: bool) -> Cell {
        match self {
            BomColumn::Field(BomField::Location | BomField::Locations) => Cell::opt(&r.location, || format!("{who} has no location.")),
            BomColumn::Field(BomField::Qty) => {
                let qty = if r.no_qty() {
                    String::new()
                } else if csv {
                    r.qty.to_string()
                } else {
                    r.qty_text()
                };
                if r.overflow { Cell { text: qty, flag: Some("The quantity is too large to add up.".into()) } } else { Cell::ok(qty) }
            }
            BomColumn::Field(BomField::Unit) => Cell::ok(if r.metres { "m" } else { "pcs" }),
            BomColumn::Field(BomField::Tags) => Cell::ok(r.tags.join(", ")),
            BomColumn::Field(BomField::Line) => Cell::ok(match (r.line, r.item) {
                (Some(n), _) => n.to_string(),
                (None, true) => String::new(),
                (None, false) => "0".into(),
            }),
            BomColumn::Field(BomField::Description) => {
                let description = if csv { r.description.clone() } else { r.description_text() };
                Cell::opt(&description, || format!("{who} has no description."))
            }
            BomColumn::Field(BomField::Maker) => Cell::opt(&r.maker, || format!("{who} has no maker.")),
            BomColumn::Field(BomField::PartNumber) => Cell::opt(&r.part_number, || format!("{who} has no part number.")),
            BomColumn::Field(BomField::ArticleNumber) => {
                let a = r.article_number.clone().unwrap_or_default();
                match &r.article_flag {
                    Some(why) => Cell { text: if a.is_empty() { "?".into() } else { a }, flag: Some(why.clone()) },
                    None => Cell::ok(a),
                }
            }
            BomColumn::Rating(w) => match r.ratings.iter().find(|(x, _, _)| *x == w) {
                Some((_, v, Some(why))) => Cell { text: v.clone(), flag: Some(why.clone()) },
                Some((_, v, None)) => Cell::ok(v.clone()),
                None => Cell::ok(String::new()),
            },
        }
    }
}

impl ReportColumn for BomColumn {
    fn header(self) -> &'static str {
        match self {
            BomColumn::Field(f) => f.header(),
            BomColumn::Rating(w) => crate::reports::bom::rating_column_label(w),
        }
    }
    fn key(self) -> Cow<'static, str> {
        match self {
            BomColumn::Field(f) => Cow::Borrowed(f.word()),
            BomColumn::Rating(w) => Cow::Owned(format!("{}{w}", ecad_model::objects::devices::RATING_OF_PREFIX)),
        }
    }
    fn number(self) -> bool {
        self == BomColumn::Field(BomField::Qty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reports::{Table, rows_json};

    /// The ids each report's rows carry (`wire_ids_json`, `cable_ids_json`, `bom_ids_json`), which no cell
    /// key may hide; `flags` is in every row.
    const WIRE_IDS: &[&str] = &["conductor", "flags"];
    const CABLE_IDS: &[&str] = &["cable", "used", "spare", "over", "from_devices", "to_devices", "flags"];
    const BOM_IDS: &[&str] = &["devices", "entries", "accessory", "strip", "kit", "flags"];

    /// Every list of columns a report can show: each column has one header and one key, no two alike, and no
    /// key hides a row id.
    fn distinct<C: ReportColumn>(what: &str, columns: &[C], ids: &[&str]) {
        let headers: Vec<_> = columns.iter().map(|c| c.header()).collect();
        let keys: Vec<_> = columns.iter().map(|c| c.key()).collect();
        for (i, k) in keys.iter().enumerate() {
            assert!(!keys[..i].contains(k), "{what}: key {k} twice");
            assert!(!ids.contains(&k.as_ref()), "{what}: key {k} hides a row id");
            assert!(!headers[..i].contains(&headers[i]), "{what}: header {} twice", headers[i]);
        }
    }

    #[test]
    fn every_report_column_has_its_own_header_and_key() {
        distinct("wire", WireColumn::ALL, WIRE_IDS);
        distinct("cable", CableColumn::ALL, CABLE_IDS);
        let ratings = vec!["current", "power"];
        for group in [Group::Location, Group::Part] {
            for (article, nested, csv) in [(false, false, false), (true, true, true), (true, false, true), (false, true, false)] {
                let view = BomView { article, ratings: ratings.clone(), nested };
                distinct("bom", &BomColumn::columns(group, &view, csv), BOM_IDS);
            }
        }
    }

    /// A table made from columns gives one cell per column in every row, and its rows key each cell by its
    /// column.
    #[test]
    fn a_table_of_columns_keys_each_cell_by_its_column() {
        let t = Table::of_columns(WireColumn::ALL, [1, 2], |c, n| Cell::ok(format!("{}{n}", c.word())));
        assert_eq!(t.headers, WireColumn::ALL.iter().map(|c| c.header()).collect::<Vec<_>>());
        let rows = rows_json(&t, WireColumn::ALL, &[]).unwrap();
        assert_eq!(rows.len(), 2);
        for c in WireColumn::ALL {
            assert_eq!(rows[1][c.word()], format!("{}2", c.word()));
        }
    }

    /// A table whose headers or row lengths are not its columns' gives no rows: a key never names
    /// another column's cell.
    #[test]
    fn rows_json_refuses_a_table_that_is_not_of_its_columns() {
        let t = Table::of_columns(WireColumn::ALL, [1], |_, _| Cell::ok("x"));
        let fewer = &WireColumn::ALL[1..];
        assert!(rows_json(&t, fewer, &[]).is_err(), "a column less");
        let mut swapped = WireColumn::ALL.to_vec();
        swapped.swap(0, 1);
        assert!(rows_json(&t, &swapped, &[]).is_err(), "two columns swapped");
        let mut short = t.clone();
        short.rows[0].pop();
        assert!(rows_json(&short, WireColumn::ALL, &[]).is_err(), "a row a cell short");
        assert!(rows_json(&t, WireColumn::ALL, &[]).is_ok());
    }
}
