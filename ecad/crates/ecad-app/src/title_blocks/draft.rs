//! The title block editor's draft (design 055 §4.2, 048's pattern): nothing is written while
//! editing; the draft has its own undo; drafts are kept in eframe storage under [`DRAFTS_KEY`]
//! (per project, at most [`MAX_DRAFTS`]) so a crash loses nothing. Save writes one transaction.

use std::collections::{BTreeMap, BTreeSet};

use ecad_model::title_block::{BlockItem, Rect, TitleBlockLayout};
use serde::{Deserialize, Serialize};

/// The eframe storage key of the editor's drafts.
pub const DRAFTS_KEY: &str = "title_block_drafts";
/// Most drafts kept per project.
pub const MAX_DRAFTS: usize = 8;
/// Most undo steps kept.
pub const MAX_UNDO: usize = 200;
/// The snap step, mm.
pub const SNAP_MM: f64 = 1.0;
/// How near a point snaps to an item edge or a line end, mm.
pub const SNAP_NEAR_MM: f64 = 1.5;

/// What Save writes: the frame's name, fields and block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub name: String,
    /// The frame's fields (`sheet`, `margins`, zones…), as `titleblock` returns them.
    pub fields: BTreeMap<String, String>,
    pub layout: TitleBlockLayout,
}

/// A draft of one frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Draft {
    /// The frame's ID (`sheet` or `#…`).
    pub id: String,
    pub builtin: bool,
    pub now: State,
    /// As last read from the project (Save and Discard compare with it).
    pub base: State,
    #[serde(skip)]
    pub undo: Vec<State>,
    #[serde(skip)]
    pub redo: Vec<State>,
    #[serde(skip)]
    pub selected: BTreeSet<usize>,
}

impl Draft {
    pub fn new(id: &str, builtin: bool, state: State) -> Draft {
        Draft { id: id.to_string(), builtin, now: state.clone(), base: state, undo: Vec::new(), redo: Vec::new(), selected: BTreeSet::new() }
    }

    /// Whether the draft differs from the project's frame.
    pub fn changed(&self) -> bool {
        self.now != self.base
    }

    /// Record the state before an edit (one undo step).
    pub fn step(&mut self) {
        self.undo.push(self.now.clone());
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn undo(&mut self) -> bool {
        match self.undo.pop() {
            Some(s) => {
                self.redo.push(std::mem::replace(&mut self.now, s));
                self.selected.retain(|i| *i < self.now.layout.items.len());
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self) -> bool {
        match self.redo.pop() {
            Some(s) => {
                self.undo.push(std::mem::replace(&mut self.now, s));
                self.selected.retain(|i| *i < self.now.layout.items.len());
                true
            }
            None => false,
        }
    }

    /// Add `item` and select it alone; returns its index.
    pub fn add(&mut self, item: BlockItem) -> usize {
        self.step();
        self.now.layout.items.push(item);
        let i = self.now.layout.items.len() - 1;
        self.selected = BTreeSet::from([i]);
        i
    }

    /// Remove the selected items.
    pub fn delete_selected(&mut self) {
        if self.selected.is_empty() {
            return;
        }
        self.step();
        let sel = std::mem::take(&mut self.selected);
        let mut i = 0;
        self.now.layout.items.retain(|_| {
            let keep = !sel.contains(&i);
            i += 1;
            keep
        });
    }

    /// Move the selected items by (dx, dy) mm (no undo step: the caller made one).
    pub fn nudge(&mut self, dx: f64, dy: f64) {
        for i in self.selected.clone() {
            if let Some(item) = self.now.layout.items.get_mut(i) {
                move_item(item, dx, dy);
            }
        }
    }

    /// Duplicate the selected items 5 mm down-right and select the copies.
    pub fn duplicate_selected(&mut self) {
        if self.selected.is_empty() {
            return;
        }
        self.step();
        let copies: Vec<BlockItem> = self.selected.iter().filter_map(|i| self.now.layout.items.get(*i).cloned()).collect();
        let start = self.now.layout.items.len();
        for mut c in copies {
            move_item(&mut c, 5.0, -5.0);
            self.now.layout.items.push(c);
        }
        self.selected = (start..self.now.layout.items.len()).collect();
    }

    /// The items outside the block (drawn with a warning outline), by index.
    pub fn outside(&self) -> Vec<usize> {
        let (w, h) = self.now.layout.size;
        self.now
            .layout
            .items
            .iter()
            .enumerate()
            .filter(|(_, i)| i.bounds().is_some_and(|b| b.x < -1e-6 || b.y < -1e-6 || b.right() > w + 1e-6 || b.top() > h + 1e-6))
            .map(|(n, _)| n)
            .collect()
    }
}

/// Move `item` by (dx, dy) mm.
pub fn move_item(item: &mut BlockItem, dx: f64, dy: f64) {
    let r = |at: &mut Rect| {
        at.x += dx;
        at.y += dy;
    };
    match item {
        BlockItem::Line { a, b, .. } => {
            *a = (a.0 + dx, a.1 + dy);
            *b = (b.0 + dx, b.1 + dy);
        }
        BlockItem::Text { at, .. } | BlockItem::Field { at, .. } | BlockItem::RevTable { at, .. } | BlockItem::Logo { at, .. } => r(at),
    }
}

/// `v` on the snap grid.
pub fn snap(v: f64, on: bool) -> f64 {
    if on { (v / SNAP_MM).round() * SNAP_MM } else { (v * 10.0).round() / 10.0 }
}

/// The drafts of every project, by project key.
#[derive(Debug, Default)]
pub struct Drafts {
    by_project: BTreeMap<String, Vec<Draft>>,
}

impl Drafts {
    /// Read the stored drafts; anything unreadable is dropped.
    pub fn from_stored(text: Option<&str>) -> Drafts {
        let by_project: BTreeMap<String, Vec<Draft>> = text.and_then(|t| serde_json::from_str(t).ok()).unwrap_or_default();
        Drafts { by_project }
    }

    pub fn to_stored(&self) -> String {
        serde_json::to_string(&self.by_project).unwrap_or_default()
    }

    /// Keep `d` for project `key` (only changed drafts are kept; the oldest go past the bound).
    pub fn keep(&mut self, key: &str, d: &Draft) {
        let v = self.by_project.entry(key.to_string()).or_default();
        v.retain(|x| x.id != d.id);
        if d.changed() {
            v.push(d.clone());
        }
        while v.len() > MAX_DRAFTS {
            v.remove(0);
        }
    }

    /// The kept draft of frame `id` in project `key`.
    pub fn get(&self, key: &str, id: &str) -> Option<&Draft> {
        self.by_project.get(key)?.iter().find(|d| d.id == id)
    }

    /// Whether project `key` keeps a changed draft of frame `id` (the list's dot).
    pub fn has(&self, key: &str, id: &str) -> bool {
        self.get(key, id).is_some()
    }

    pub fn forget(&mut self, key: &str, id: &str) {
        if let Some(v) = self.by_project.get_mut(key) {
            v.retain(|x| x.id != id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecad_model::title_block::LineWeight;

    fn state() -> State {
        State { name: "A3".into(), fields: BTreeMap::new(), layout: TitleBlockLayout::blank(180.0, 40.0) }
    }

    #[test]
    fn undo_redo_and_changed() {
        let mut d = Draft::new("#1", false, state());
        assert!(!d.changed());
        d.add(BlockItem::Line { a: (0.0, 0.0), b: (10.0, 0.0), weight: LineWeight::Thin });
        assert!(d.changed());
        d.step();
        d.nudge(1.0, 0.0);
        assert!(d.undo());
        assert!(d.undo());
        assert!(!d.changed());
        assert!(d.redo());
        assert_eq!(d.now.layout.items.len(), 1);
    }

    #[test]
    fn drafts_store_and_bound() {
        let mut all = Drafts::default();
        for i in 0..12 {
            let mut d = Draft::new(&format!("#{i}"), false, state());
            d.now.name = "changed".into();
            all.keep("p", &d);
        }
        let back = Drafts::from_stored(Some(&all.to_stored()));
        assert!(back.has("p", "#11") && !back.has("p", "#0"));
        assert_eq!(Drafts::from_stored(Some("{not json")).to_stored(), "{}");
    }
}
