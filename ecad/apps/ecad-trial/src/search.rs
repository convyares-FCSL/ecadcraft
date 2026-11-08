//! The symbol search (brief 018, design 018 §5.1): one text field that finds starter symbols by
//! name, alias or tag prefix, and devices already in the project by tag. A row starts Place
//! Symbol with it (a symbol) or adds a symbol to it (a device).
//!
//! CADCraft's Tool Sets panel hosts buttons only, so the field lives in a small window of the
//! trial's own, opened from the ECAD tab's Components group (**Find Symbol**); no CADCraft change.
//! The same search is typed at the `place` prompt.
//!
//! It only reads the project, under the handle's short lock, and only when the query or the
//! project's head changed.

use ecad_cadcraft::devices;
use ecad_cadcraft::symbols;
use ecad_model::{Document, Subject};
use eframe::egui;

/// The text field's element name (design 018 §5.1).
pub const FIELD_ID: &str = "ecad.symbolsearch";
/// The results list's element name.
pub const RESULTS_ID: &str = "ecad.symbolresults";
/// The field's visible label (headless tests find it by this).
pub const FIELD_LABEL: &str = "Find symbol or device";
/// The window's title.
pub const TITLE: &str = "Find Symbol";
/// Most rows shown before `and N more`.
pub const MAX_ROWS: usize = 10;

/// What a result row starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pick {
    /// Place Symbol with this starter symbol.
    Symbol(&'static str),
    /// Place Symbol adding a symbol to the device with this tag.
    Device(String),
}

/// One result row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub text: String,
    pub pick: Pick,
}

/// The rows for `query` (devices first, then symbols), how many more there are, and the "nothing"
/// line when nothing matches.
pub fn rows(doc: Option<&Document>, query: &str) -> (Vec<Row>, usize, Option<String>) {
    let q: String = query.trim().chars().take(symbols::MAX_QUERY).collect();
    let mut out = Vec::new();
    if let Some(doc) = doc
        && !q.is_empty()
    {
        for c in devices::devices(doc, Some(&q)) {
            let tag = devices::tag_words(doc, c);
            let n = doc.representations_of(Subject::Component(c)).count();
            let pages = devices::pages_words(doc, c);
            out.push(Row {
                text: format!(
                    "{tag:<6} {:<26} {n} symbol{}, {pages}  (device)",
                    clip(&devices::description(doc, c), 26),
                    if n == 1 { "" } else { "s" }
                ),
                pick: Pick::Device(tag),
            });
        }
    }
    for s in symbols::search(&q) {
        let prefix = doc.map_or_else(|| s.prefix.to_string(), |d| devices::prefix_for(d, s));
        out.push(Row {
            text: format!("{:<6} {:<26} -{prefix:<3} {}  (symbol)", s.key, s.name, s.labels().join("/")),
            pick: Pick::Symbol(s.key),
        });
    }
    let more = out.len().saturating_sub(MAX_ROWS);
    out.truncate(MAX_ROWS);
    let nothing = out.is_empty().then(|| format!("Nothing matches \"{q}\"."));
    (out, more, nothing)
}

fn clip(t: &str, n: usize) -> String {
    if t.chars().count() <= n {
        t.to_string()
    } else {
        format!("{}…", t.chars().take(n.saturating_sub(1)).collect::<String>())
    }
}

/// The search window's state.
#[derive(Debug, Default)]
pub struct SymbolSearch {
    pub open: bool,
    pub query: String,
    /// The highlighted row (Up/Down, Enter).
    pub selected: usize,
    /// Give the field the keyboard on the next frame.
    pub focus: bool,
    /// The rows for (query, project head), recomputed only when either changes.
    cache: Option<((String, u64), (Vec<Row>, usize, Option<String>))>,
}

impl SymbolSearch {
    /// Open (focusing the field) or close.
    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.focus = self.open;
    }

    /// The rows for the current query, read with `doc` (`head` decides whether the cached ones
    /// still hold).
    pub fn rows_for(
        &mut self,
        head: u64,
        doc: impl FnOnce() -> Option<(Vec<Row>, usize, Option<String>)>,
    ) -> (Vec<Row>, usize, Option<String>) {
        let key = (self.query.clone(), head);
        if let Some((k, v)) = &self.cache
            && *k == key
        {
            return v.clone();
        }
        let v = doc().unwrap_or_else(|| rows(None, &self.query));
        self.cache = Some((key, v.clone()));
        v
    }

    /// Draw the window; returns the row the user chose.
    pub fn show(&mut self, ctx: &egui::Context, rows: &(Vec<Row>, usize, Option<String>)) -> Option<Pick> {
        if !self.open {
            return None;
        }
        let mut chosen = None;
        let mut open = self.open;
        egui::Window::new(TITLE)
            .id(egui::Id::unique("ecad.symbolsearch.window"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_pos(egui::pos2(260.0, 120.0))
            .show(ctx, |ui| {
                let label = ui.label(FIELD_LABEL);
                let field = egui::TextEdit::singleline(&mut self.query)
                    .id_salt(FIELD_ID)
                    .hint_text("Find symbol or device…")
                    .char_limit(symbols::MAX_QUERY)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(420.0);
                let r = ui.add(field).labelled_by(label.id);
                if std::mem::take(&mut self.focus) {
                    r.request_focus();
                }
                if r.changed() {
                    self.selected = 0;
                }
                let (list, more, nothing) = rows;
                let keys = ui.input(|i| {
                    (
                        i.key_pressed(egui::Key::ArrowDown),
                        i.key_pressed(egui::Key::ArrowUp),
                        i.key_pressed(egui::Key::Escape),
                    )
                });
                if r.has_focus() || r.lost_focus() {
                    if keys.0 && self.selected + 1 < list.len() {
                        self.selected += 1;
                    }
                    if keys.1 {
                        self.selected = self.selected.saturating_sub(1);
                    }
                    if keys.2 {
                        self.query.clear();
                    }
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        chosen = list.get(self.selected).map(|row| row.pick.clone());
                    }
                }
                ui.push_id(RESULTS_ID, |ui| {
                    for (i, row) in list.iter().enumerate() {
                        let text = egui::RichText::new(&row.text).monospace();
                        if ui.selectable_label(i == self.selected, text).clicked() {
                            chosen = Some(row.pick.clone());
                        }
                    }
                    if *more > 0 {
                        ui.weak(format!("and {more} more"));
                    }
                    if let Some(n) = nothing {
                        ui.weak(n);
                    }
                });
            });
        self.open = open;
        chosen
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_without_a_project_list_symbols() {
        let (r, more, nothing) = rows(None, "");
        assert_eq!(r.len(), MAX_ROWS);
        assert_eq!(more, symbols::STARTERS.len() - MAX_ROWS);
        assert!(nothing.is_none());
        let (r, _, _) = rows(None, "-Q");
        assert_eq!(r.iter().map(|x| x.pick.clone()).collect::<Vec<_>>(), [Pick::Symbol("mcb1"), Pick::Symbol("mcb2")]);
        let (r, _, nothing) = rows(None, "xyz");
        assert!(r.is_empty());
        assert_eq!(nothing.as_deref(), Some("Nothing matches \"xyz\"."));
    }
}
