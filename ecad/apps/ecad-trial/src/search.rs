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
//!
//! Brief 041 (Ctrl+K): the same field also finds pages (by number or title) and actions (the
//! action table, by label and search words). Picking one of those runs it at once: a page is
//! shown, an action runs on the selection or starts its tool ([`SymbolSearch::ran`]).

use ecad_cadcraft::devices;
use ecad_cadcraft::symbols;
use ecad_model::{ComponentId, Document, Subject};
use eframe::egui;

/// The text field's element name (design 018 §5.1).
pub const FIELD_ID: &str = "ecad.symbolsearch";
/// The results list's element name.
pub const RESULTS_ID: &str = "ecad.symbolresults";
/// The second step's list (design 018 §5.1, owner's trial of 2026-11-08).
pub const CHOICES_ID: &str = "ecad.symbolchoices";
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
    /// Brief 041: show this page.
    Page(ecad_model::PageId),
    /// Brief 041: run this action (its id in the action table).
    Action(&'static str),
}

/// Most pages and most actions Ctrl+K lists before the symbols.
pub const MAX_PAGES: usize = 3;
pub const MAX_ACTIONS: usize = 4;

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
        let ql = q.to_lowercase();
        let pages = ecad_cadcraft::tree_rows(doc, |_| false).into_iter().filter_map(|r| match r {
            ecad_cadcraft::TreeRow::Page(p) => Some(p),
            ecad_cadcraft::TreeRow::Folder { .. } => None,
        });
        for p in pages.filter(|p| p.number.to_lowercase() == ql || p.title.to_lowercase().contains(&ql)).take(MAX_PAGES) {
            out.push(Row { text: format!("{:<6} {:<26}  (page)", p.number, clip(&p.title, 26)), pick: Pick::Page(p.id) });
        }
    }
    for a in ecad_cadcraft::actions::search(&q, MAX_ACTIONS) {
        let key = a.key.map(|k| format!(" {}", k.words())).unwrap_or_default();
        out.push(Row { text: format!("{:<33}{key}  (action)", a.label), pick: Pick::Action(a.id) });
    }
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
        let prefix = doc.map_or_else(|| s.prefix().to_string(), |d| devices::prefix_for(d, s));
        out.push(Row { text: format!("{:<6} {:<26} -{prefix:<3} {}  (symbol)", s.key, s.name, s.labels().join("/")), pick: Pick::Symbol(s.key) });
    }
    let more = out.len().saturating_sub(MAX_ROWS);
    out.truncate(MAX_ROWS);
    let nothing = out.is_empty().then(|| format!("Nothing matches \"{q}\"."));
    (out, more, nothing)
}

fn clip(t: &str, n: usize) -> String {
    if t.chars().count() <= n { t.to_string() } else { format!("{}…", t.chars().take(n.saturating_sub(1)).collect::<String>()) }
}

/// What a click in the second step places (owner's trial of 2026-11-08: no Enter needed): a
/// symbol, as a new device (`device: None`) or as another symbol of the device with that tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub text: String,
    pub symbol: &'static str,
    pub device: Option<String>,
}

/// Most choices the second step lists.
pub const MAX_CHOICES: usize = 10;

/// The second step for `pick` (owner's trial, 2026-11-08). A symbol: "New device (-K02)" first,
/// then the devices it can join (those of its own prefix first, and among them those with a main
/// symbol such as a coil first). A device: the symbols it can take, its own prefix's first. Only
/// what joining would accept is listed.
pub fn choices(p: &ecad_cadcraft::Project, pick: &Pick) -> Vec<Choice> {
    let doc = p.ledger.document();
    let none = std::collections::BTreeSet::new();
    match pick {
        Pick::Symbol(key) => {
            let Some(s) = symbols::by_key(key) else { return Vec::new() };
            let next = devices::next_tag_preview(p, s).unwrap_or_else(|| "new tag".into());
            let mut out = vec![Choice { text: format!("New device ({next})"), symbol: s.key, device: None }];
            let prefix = devices::prefix_for(doc, s);
            let mut found: Vec<(u8, u8, ComponentId, String)> = devices::devices(doc, None)
                .into_iter()
                .filter_map(|c| {
                    let labels = devices::join_labels(doc, c, s, &none).ok()?;
                    let tag = devices::tag_words(doc, c);
                    let own = ecad_model::tag::parse(&tag).is_some_and(|(px, _)| px == prefix);
                    let main = doc
                        .representations_of(Subject::Component(c))
                        .filter_map(|(_, r)| devices::rep_starter(doc, r))
                        .any(|x| matches!(x.kind, symbols::Kind::Fixed { exclusive: true }));
                    let text = format!("{tag:<6} {:<26} {}", clip(&devices::description(doc, c), 26), labels.join("/"));
                    Some((u8::from(!own), u8::from(!main), c, text))
                })
                .collect();
            found.sort_by_key(|(a, b, _, _)| (*a, *b));
            out.extend(found.into_iter().take(MAX_CHOICES - 1).map(|(_, _, c, text)| Choice {
                text,
                symbol: s.key,
                device: Some(devices::tag_words(doc, c)),
            }));
            out
        }
        Pick::Device(tag) => {
            let Some(c) = devices::holder_of(doc, tag, None) else { return Vec::new() };
            let own = ecad_model::tag::parse(tag).map(|(px, _)| px).unwrap_or_default();
            let mut found: Vec<(u8, Choice)> = symbols::STARTERS
                .iter()
                .filter_map(|s| {
                    let labels = devices::join_labels(doc, c, s, &none).ok()?;
                    let text = format!("{:<28} {}", s.name, labels.join("/"));
                    let rank = u8::from(devices::prefix_for(doc, s) != own);
                    Some((rank, Choice { text, symbol: s.key, device: Some(tag.clone()) }))
                })
                .collect();
            found.sort_by_key(|(rank, _)| *rank);
            found.into_iter().take(MAX_CHOICES).map(|(_, c)| c).collect()
        }
        Pick::Page(_) | Pick::Action(_) => Vec::new(),
    }
}

/// Which project and which of its changes a copy was read at: (project serial, change count).
pub type At = (u64, u64);

/// A copy read for `key` at a project change ([`At`]).
#[derive(Debug)]
struct Cached<K, V> {
    key: K,
    at: Option<At>,
    value: V,
}

impl<K: PartialEq, V: Clone> Cached<K, V> {
    /// The copy for `key` at `at`, read again only when the key or the project differs, or
    /// `unchanged(count the copy was read at)` is false. Returns the value and whether it was read.
    fn get(slot: &mut Option<Self>, key: K, at: Option<At>, unchanged: impl Fn(u64) -> bool, read: impl FnOnce() -> V) -> (V, bool) {
        if let Some(c) = slot.as_mut()
            && c.key == key
        {
            let keep = match (c.at, at) {
                (None, None) => true,
                (Some((sp, sn)), Some((p, n))) => sp == p && (sn == n || unchanged(sn)),
                _ => false,
            };
            if keep {
                c.at = at;
                return (c.value.clone(), false);
            }
        }
        let value = read();
        *slot = Some(Cached { key, at, value: value.clone() });
        (value, true)
    }
}

/// The search window's state.
#[derive(Debug, Default)]
pub struct SymbolSearch {
    /// Brief 038: the Library workspace's main view it fills, or `None` to float.
    pub host: Option<egui::Rect>,
    pub open: bool,
    pub query: String,
    /// The highlighted row (Up/Down, Enter).
    pub selected: usize,
    /// Give the field the keyboard on the next frame.
    pub focus: bool,
    /// The rows, for (query, project) as of a change count ([`Cached`]).
    cache: Option<Cached<String, (Vec<Row>, usize, Option<String>)>>,
    /// The second step's choices, for (pick, project) as of a change count.
    choice_cache: Option<Cached<Pick, Vec<Choice>>>,
    /// How many times the rows and the choices were read from the project (tests).
    pub rebuilds: u64,
    /// The row picked: the window then shows its second step ([`choices`]).
    pub picked: Option<Pick>,
    /// Brief 041: a page or action row picked, for the shell to act on ([`Pick::Page`],
    /// [`Pick::Action`]).
    pub ran: Option<Pick>,
}

impl SymbolSearch {
    /// Open (focusing the field) or close.
    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.focus = self.open;
    }

    /// The rows for the current query (update map, owner 2026-11-08): read again only when the
    /// query or the project differs, or when a change since the copy touched a device, a page or
    /// the settings (`unchanged(since)` says none did). Drawing edits, wires and view switches
    /// keep the copy.
    pub fn rows_for(
        &mut self, at: Option<At>, unchanged: impl Fn(u64) -> bool, read: impl FnOnce() -> (Vec<Row>, usize, Option<String>),
    ) -> (Vec<Row>, usize, Option<String>) {
        let key = self.query.clone();
        let (v, built) = Cached::get(&mut self.cache, key, at, unchanged, read);
        self.rebuilds += u64::from(built);
        v
    }

    /// The second step's choices for the pick shown, under the same update map as
    /// [`SymbolSearch::rows_for`]; empty when nothing is picked.
    pub fn choices_for(&mut self, at: Option<At>, unchanged: impl Fn(u64) -> bool, read: impl FnOnce(&Pick) -> Vec<Choice>) -> Vec<Choice> {
        let Some(pick) = self.picked.clone() else {
            self.choice_cache = None;
            return Vec::new();
        };
        let (v, built) = Cached::get(&mut self.choice_cache, pick.clone(), at, unchanged, || read(&pick));
        self.rebuilds += u64::from(built);
        v
    }

    /// Draw the window: the search and its rows, or, once a row is picked, its second step
    /// ([`choices`]). Returns the choice clicked.
    pub fn show(&mut self, ctx: &egui::Context, rows: &(Vec<Row>, usize, Option<String>), choices: &[Choice]) -> Option<Choice> {
        if !self.open {
            return None;
        }
        let mut chosen = None;
        let mut picked = None;
        let mut back = false;
        let mut open = self.open;
        let window = egui::Window::new(TITLE)
            .id(egui::Id::unique("ecad.symbolsearch.window"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_pos(egui::pos2(260.0, 120.0));
        crate::shell::hosted(window, self.host).show(ctx, |ui| {
            if let Some(p) = &self.picked {
                let what = match p {
                    Pick::Symbol(k) => format!("Place {} as:", symbols::by_key(k).map_or(*k, |s| s.name)),
                    Pick::Device(t) => format!("Add to {t}:"),
                    Pick::Page(_) | Pick::Action(_) => String::new(),
                };
                ui.label(what);
                ui.push_id(CHOICES_ID, |ui| {
                    for c in choices {
                        if ui.button(egui::RichText::new(&c.text).monospace()).clicked() {
                            chosen = Some(c.clone());
                        }
                    }
                    if choices.is_empty() {
                        ui.weak("Nothing can be added to that device.");
                    }
                });
                if ui.button("Back").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    back = true;
                }
                return;
            }
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
            let keys = ui.input(|i| (i.key_pressed(egui::Key::ArrowDown), i.key_pressed(egui::Key::ArrowUp), i.key_pressed(egui::Key::Escape)));
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
                    picked = list.get(self.selected).map(|row| row.pick.clone());
                }
            }
            ui.push_id(RESULTS_ID, |ui| {
                for (i, row) in list.iter().enumerate() {
                    let text = egui::RichText::new(&row.text).monospace();
                    if ui.selectable_label(i == self.selected, text).clicked() {
                        picked = Some(row.pick.clone());
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
        // Brief 041: a page or an action runs at once (no second step).
        if let Some(p @ (Pick::Page(_) | Pick::Action(_))) = picked {
            self.ran = Some(p);
            self.open = false;
            self.query.clear();
            return None;
        }
        if picked.is_some() {
            self.picked = picked;
        }
        if back || chosen.is_some() {
            self.picked = None;
        }
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
        assert_eq!(
            r.iter().map(|x| x.pick.clone()).collect::<Vec<_>>(),
            [Pick::Symbol("mcb1"), Pick::Symbol("mcb2"), Pick::Symbol("rcbo2"), Pick::Symbol("rcd2"), Pick::Symbol("rcd4")]
        );
        let (r, _, nothing) = rows(None, "xyz");
        assert!(r.is_empty());
        assert_eq!(nothing.as_deref(), Some("Nothing matches \"xyz\"."));
    }
}
