//! Brief 059 (design 059 §5.6): while Cable slash runs, the right panel offers the **Cable** to
//! slash into: `New cable` (the default, Q7), then the cables not drawn (those whose planned end
//! is this page's location first, marked `matches`), then drawn cables not slashed on this page
//! (032). Picking one types the same `Cable` keyword the command line takes, so the prompt machine
//! does the rest; Esc writes nothing.
//!
//! **Update map.** The list is read once when the tool starts (or the page or project changes),
//! never per frame: O(cables).

use ecad_cadcraft::cable_cmds::PickItem;
use ecad_ui::control::{self, Select};
use eframe::egui;

use crate::Trial;

/// The picker's accessible name (design 059 §5.7).
pub const NAME: &str = "ecad.draw.slash.cable";
pub const NEW_CABLE: &str = "New cable";

/// The picker's state: the list read when it opened, and the item picked.
#[derive(Debug, Clone, Default)]
pub struct Picker {
    /// (project change count, page) the list was read at.
    pub seen: Option<(u64, ecad_model::PageId)>,
    pub items: Vec<PickItem>,
    /// 0: New cable; `i + 1`: `items[i]`.
    pub picked: usize,
    /// The labels shown, with their section words.
    pub labels: Vec<String>,
    /// A cable picked before the list was read (Assign in the drawing).
    pub want: Option<ecad_model::CableId>,
}

impl Picker {
    fn labels(items: &[PickItem]) -> Vec<String> {
        let mut v = vec![NEW_CABLE.to_string()];
        for i in items {
            let section = if i.drawn { "Drawn" } else { "Not drawn" };
            let mark = if i.matches { "  \u{b7} matches" } else { "" };
            v.push(format!("{section}: {}{mark}", i.line));
        }
        v
    }
}

impl Trial {
    /// Whether Cable slash runs.
    pub fn slash_running(&self) -> bool {
        self.cad.session.running.as_ref().is_some_and(|r| r.id.eq_ignore_ascii_case("cableslash"))
    }

    /// Read the picker's list when the tool has just started, or the page or project changed.
    pub fn sync_slash_picker(&mut self) {
        if !self.slash_running() {
            self.shell.slash_picker = Picker::default();
            return;
        }
        let Some(p) = self.project.clone() else { return };
        let Some(page) = self.panel.active else { return };
        let read = p.read(&mut self.cad.session, |p, _| {
            let doc = p.ledger.document();
            (p.ledger.head(), ecad_cadcraft::cable_cmds::picker(doc, page))
        });
        let Ok((head, items)) = read else { return };
        let key = Some((head, page));
        if self.shell.slash_picker.seen == key {
            return;
        }
        let old = &self.shell.slash_picker;
        let picked = old.want.or_else(|| old.items.get(old.picked.wrapping_sub(1)).map(|i| i.cable));
        let labels = Picker::labels(&items);
        let at = picked.and_then(|k| items.iter().position(|i| i.cable == k)).map_or(0, |i| i + 1);
        self.shell.slash_picker = Picker { seen: key, items, picked: at, labels, want: None };
    }

    /// Pick item `i` (0: New cable): the `Cable` keyword, then the tag or `New`.
    pub fn slash_pick(&mut self, i: usize) {
        use cadcraft_engine::Input;
        let tag = i.checked_sub(1).and_then(|j| self.shell.slash_picker.items.get(j)).map(|x| x.tag.clone());
        let _ = self.cad.session.input(Input::Keyword("Cable".into()));
        let r = match tag {
            Some(t) => self.cad.session.input(Input::Text(t)),
            None => self.cad.session.input(Input::Keyword("New".into())),
        };
        match r {
            Ok(_) => self.shell.slash_picker.picked = i,
            Err(e) => self.cad.session.echo(e.to_string()),
        }
    }
}

/// The picker's body in the right panel. Returns the item picked this frame.
pub fn show(ui: &mut egui::Ui, p: &Picker) -> Option<usize> {
    let opts: Vec<&str> = p.labels.iter().map(String::as_str).collect();
    let mut at = p.picked;
    ecad_ui::panel::section_header(ui, true, "Cable slash", "one transaction");
    control::row(ui, "Cable", |ui| Select::new(&mut at, &opts, NAME).show(ui));
    let words = match at {
        0 => "A new cable takes the next free tag.".to_string(),
        i => p.items.get(i - 1).map_or_else(String::new, |x| {
            if x.drawn { format!("Continues {} on this page.", x.tag) } else { format!("The wires crossed become {}'s cores, in order.", x.tag) }
        }),
    };
    control::row(ui, "", |ui| control::read_text(ui, &words, control::Decor::default(), "ecad.draw.slash.cable.words"));
    (at != p.picked).then_some(at)
}
