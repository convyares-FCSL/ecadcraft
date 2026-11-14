//! The Library workspace (brief 048, design 048 §3, §6, §8): the **browser** (project symbols and
//! the built-ins as tiles, with their details on the right) or the **builder** (a symbol drawn on
//! an empty grid, its Details and Missing list on the right, and the "Before saving" review in the
//! drawer). No new panel and no dialog: the main view, the right panel and the drawer change what
//! they hold.
//!
//! Save is the only write, one transaction through the JSON forms (`symbolnew`, `symboledit`,
//! `makesymbol`); the review reads `symbolimpact`. Programmatic calls never open the builder.

pub mod browser;
pub mod builder;
pub mod details;
pub mod draft;
pub mod review;
pub mod view;

use ecad_cadcraft::symbol_cmds::Content;
use ecad_cadcraft::symbols;
use ecad_model::{DefinitionId, Form, ItemKind, Subject};
use serde_json::{Value, json};

pub use builder::{Builder, Tool};
pub use draft::{Draft, Drafts, Source};
pub use review::Review;

use crate::Trial;

/// The Library workspace's state.
#[derive(Debug, Default)]
pub struct Library {
    /// The builder, while a symbol is drawn (else the browser shows).
    pub builder: Option<Builder>,
    pub browser: browser::Browser,
    /// The drafts of every project (eframe storage, design 048 §3.2).
    pub drafts: Drafts,
    /// The drafts as last stored, so `to_save` writes only when they changed.
    stored: String,
    /// Compare (design 048 §8.4): the drawer row of one placed copy, no save.
    pub compare: Option<Review>,
    /// The workspace to go back to when the builder closes (a Make or Edit from the page).
    pub back_to_page: bool,
    /// The browser's listing last frame (the right panel draws the selected tile's details).
    pub listing: browser::Listing,
}

impl Library {
    /// Take the stored drafts.
    pub fn load(&mut self, stored: Option<&str>) {
        self.drafts = Drafts::from_stored(stored);
        self.stored = self.drafts.to_stored();
    }

    /// The drafts to store when they changed since the last call.
    pub fn to_save(&mut self) -> Option<String> {
        let s = self.drafts.to_stored();
        (s != self.stored).then(|| {
            self.stored = s.clone();
            s
        })
    }
}

fn hex_of<T: From<u128>>(h: &str) -> Option<T> {
    u128::from_str_radix(h, 16).ok().map(T::from)
}

impl Trial {
    /// Whether the builder is open.
    pub fn builder_open(&self) -> bool {
        self.library.builder.is_some()
    }

    /// The open project's key for drafts, if a project is open.
    pub(crate) fn drafts_key(&mut self) -> Option<String> {
        let p = self.project.clone().filter(|p| p.is_attached(&self.cad.session))?;
        p.read(&mut self.cad.session, |p, _| p.id.hex()).ok()
    }

    /// Open the builder on `draft` in the Library workspace (the drawer closes while drawing).
    pub fn open_builder(&mut self, mut draft: Draft) {
        let key = self.drafts_key();
        if draft.id == 0 {
            draft.id = key.as_deref().map_or(1, |k| self.library.drafts.next_id(k));
        }
        if let Some(k) = &key {
            self.library.drafts.keep(k, &draft);
        }
        self.library.builder = Some(Builder::new(draft));
        self.library.compare = None;
        self.shell.state.builder = true;
        self.shell.state.drawer_open = false;
        self.shell.state.right_collapsed = false;
        self.set_workspace(crate::shell::Workspace::Library);
    }

    /// Library ▸ New symbol (design 048 §3.3).
    pub fn new_symbol(&mut self) {
        self.library.back_to_page = false;
        self.open_builder(Draft::new());
    }

    /// Resume a kept draft of this project.
    pub fn resume_draft(&mut self, id: u64) {
        let Some(k) = self.drafts_key() else { return };
        if let Some(d) = self.library.drafts.of(&k).iter().find(|d| d.id == id).cloned() {
            self.open_builder(d);
        }
    }

    /// Edit symbol on definition `def` (design 048 §6.1): the builder on that version; `rep` is
    /// the placed copy it was opened from. Editing an older version than the newest asks first.
    ///
    /// # Errors
    /// No project, or no such symbol.
    pub fn edit_symbol(&mut self, def: DefinitionId, rep: Option<ecad_model::RepresentationId>) -> Result<(), String> {
        let p = self.project.clone().ok_or("No project is open.")?;
        let found = p.read(&mut self.cad.session, |p, _| {
            let doc = p.ledger.document();
            let d = doc.definitions.get(&def)?;
            let newer =
                symbols::Version::of(def, d).and_then(|v| symbols::newest(doc, v.lineage).filter(|n| n.seq > v.seq).map(|n| (n.id, v.seq, n.seq)));
            Some((Content::of(d), d.origin.as_ref().map_or(0, |o| o.library_seq), symbols::is_project_symbol(d), newer))
        })?;
        let (content, seq, project, newer) = found.ok_or("No such symbol.")?;
        let source = Source::Edit { definition: def.hex(), version: seq, builtin: !project, rep: rep.map(|r| r.hex()) };
        let mut draft = Draft::of_content(&content, source);
        if !project {
            // A built-in edited: a project copy, which needs a key of its own (design 048 §6.3).
            draft.key.clear();
            draft.mark_start();
        }
        self.open_builder(draft);
        if let (Some(b), Some((newest, shown, n))) = (self.library.builder.as_mut(), newer) {
            b.ask_version = Some(builder::AskVersion { newest, shown, newest_seq: n });
        }
        Ok(())
    }

    /// Edit symbol on a built-in starter from the browser: its embedded definition when the
    /// project uses it, else a new symbol drawn from it (design 048 §8.3).
    pub fn edit_starter(&mut self, key: &str) -> Result<(), String> {
        let s = symbols::by_key(key).ok_or("No such symbol.")?;
        let p = self.project.clone().ok_or("No project is open.")?;
        let embedded = p.read(&mut self.cad.session, |p, _| symbols::embedded(p.ledger.document(), s))?;
        if let Some(id) = embedded {
            return self.edit_symbol(id, None);
        }
        let def = s.definition()?;
        let mut draft = Draft::of_content(&Content::of(def), Source::New);
        draft.key.clear();
        draft.name = s.name.to_string();
        draft.prefix = s.prefix().to_string();
        draft.from = Some(s.id.hex());
        draft.mark_start();
        self.library.back_to_page = false;
        self.open_builder(draft);
        Ok(())
    }

    /// Edit symbol on the wheel of a placed copy (design 048 §6.1).
    pub fn edit_placed(&mut self, rep: ecad_model::RepresentationId) -> Result<(), String> {
        let p = self.project.clone().ok_or("No project is open.")?;
        let def = p.read(&mut self.cad.session, |p, _| {
            let doc = p.ledger.document();
            let r = doc.representations.get(&rep)?;
            r.items.iter().find_map(|i| match doc.items.get(i).map(|x| &x.kind) {
                Some(ItemKind::Symbol(s)) => Some(s.definition),
                _ => None,
            })
        })?;
        let def = def.ok_or("That symbol has no definition to edit.")?;
        self.library.back_to_page = true;
        self.edit_symbol(def, Some(rep))
    }

    /// Make symbol (design 048 §4): from a drawn device (promotion), else from the selected plain
    /// geometry. Opens the builder; nothing is written until Save.
    ///
    /// # Errors
    /// Nothing to make a symbol of, or part of the selection has a meaning.
    pub fn make_symbol(&mut self, target: Option<&ecad_cadcraft::interact::kind::Target>) -> Result<(), String> {
        let p = self.project.clone().ok_or("No project is open.")?;
        if let Some(t) = target.filter(|t| t.drawn)
            && let Some(c) = t.component
        {
            let found = p.read(&mut self.cad.session, |p, _| {
                let doc = p.ledger.document();
                let (_, rep) = doc.representations_of(Subject::Component(c)).find(|(_, r)| r.form == Form::Drawn)?;
                let mut prims = Vec::new();
                for i in &rep.items {
                    if let Some(ItemKind::Primitive(pr)) = doc.items.get(i).map(|x| &x.kind) {
                        prims.push(pr.clone());
                    }
                }
                let o = ecad_cadcraft::symbol_cmds::grid_origin(&prims);
                let points: Vec<ecad_model::TerminalDef> = rep
                    .connection_points
                    .iter()
                    .enumerate()
                    .filter_map(|(n, (term, at))| {
                        let tm = doc.terminals.get(term)?;
                        Some(ecad_model::TerminalDef {
                            key: tm.key.clone().or_else(|| tm.label.clone()).unwrap_or_else(|| (n + 1).to_string()),
                            label: tm.label.clone(),
                            at: ecad_model::Pt::new(at.x - o.x, at.y - o.y),
                        })
                    })
                    .collect();
                let graphics = prims.iter().map(|g| ecad_cadcraft::symbol_cmds::shifted(g, o)).collect();
                let tag = ecad_cadcraft::devices::tag_words(doc, c);
                Some((graphics, points, tag, [o.x, o.y]))
            })?;
            let (graphics, points, tag, at) = found.ok_or("That device has no drawing to make a symbol of.")?;
            let content = Content { graphics, points, ..Content::default() };
            let mut draft = Draft::of_content(&content, Source::Promote { component: c.hex(), tag: tag.clone(), at });
            draft.shape.offers = draft::offers_for(&draft.shape.graphics).into_iter().filter(|o| matches!(o, draft::Offer::Slot { .. })).collect();
            draft.mark_start();
            self.library.back_to_page = true;
            self.open_builder(draft);
            return Ok(());
        }
        let handles = match target.filter(|t| !t.handles.is_empty()) {
            Some(t) if t.kind == ecad_cadcraft::actions::ObjectKind::Mixed => self.cad.session.selection(),
            Some(t) => t.handles.clone(),
            None => self.cad.session.selection(),
        };
        if handles.is_empty() {
            return Err("Select the lines, arcs and texts to make a symbol of.".into());
        }
        let page = self.panel.active.ok_or("No page is shown.")?;
        let found = p.read(&mut self.cad.session, |p, _| -> Result<(Vec<ecad_model::Primitive>, String), String> {
            let view = p.pages.get(&page).ok_or("The page is not shown.")?;
            let doc = p.ledger.document();
            let mut prims = Vec::new();
            for h in &handles {
                match view.map.get(*h) {
                    Some(ecad_cadcraft::project::Mapped::Item(i)) if doc.representation_of_item(i).is_none() => {
                        match doc.items.get(&i).map(|x| &x.kind) {
                            Some(ItemKind::Primitive(pr)) => prims.push(pr.clone()),
                            _ => return Err("Make symbol takes lines, arcs and text.".into()),
                        }
                    }
                    // The same words as `makesymbol`'s refusal, naming the object (design 048 §9.5).
                    Some(ecad_cadcraft::project::Mapped::Item(i)) => return Err(ecad_cadcraft::symbol_cmds::not_plain(doc, i)),
                    Some(_) => return Err("Make symbol takes lines, arcs and text; part of the selection already has a meaning.".into()),
                    None => return Err("The selection holds an object this page does not know.".into()),
                }
            }
            Ok((prims, ecad_cadcraft::references::page_number(doc, page)))
        })??;
        let (prims, number) = found;
        if prims.is_empty() {
            return Err("Select the lines, arcs and texts to make a symbol of.".into());
        }
        let draft = Draft::of_geometry(&prims, page.hex(), number, handles.iter().map(|h| h.0).collect());
        self.library.back_to_page = true;
        self.open_builder(draft);
        Ok(())
    }

    /// Close the builder (after Save, or Cancel / Discard), back to the browser, or to the page
    /// it was opened from.
    pub fn close_builder(&mut self, forget: bool) {
        if let Some(b) = self.library.builder.take()
            && forget
            && let Some(k) = self.drafts_key()
        {
            self.library.drafts.forget(&k, b.draft.id);
        }
        self.shell.state.builder = false;
        if std::mem::take(&mut self.library.back_to_page) {
            self.set_workspace(crate::shell::Workspace::Draw);
        }
    }

    /// Keep the builder's draft in the drafts (each frame it changed).
    pub(crate) fn keep_draft(&mut self) {
        let Some(d) = self.library.builder.as_ref().map(|b| b.draft.clone()) else { return };
        let Some(k) = self.drafts_key() else { return };
        if self.library.drafts.of(&k).contains(&d) {
            return;
        }
        self.library.drafts.keep(&k, &d);
    }

    /// The builder's Save (design 048 §3.3 step 7, §4.1 step 4, §6.1 step 3): a new symbol and a
    /// made one are written now; an edit opens the review in the drawer first. `as_new`: Save as
    /// new (an edit becomes version 1 of a new symbol).
    pub fn builder_save(&mut self, as_new: bool) {
        let Some(b) = self.library.builder.as_ref() else { return };
        let d = b.draft.clone();
        let mut params = d.params();
        let r = match &d.source {
            Source::New => {
                if let Some(f) = &d.from {
                    params["from"] = json!(f);
                }
                self.cad.run("symbolnew", params)
            }
            Source::Edit { definition, builtin, .. } => {
                params["definition"] = json!(definition);
                if as_new {
                    params["update"] = json!("new");
                    self.cad.run("symboledit", params)
                } else {
                    match self.cad.run("symbolimpact", params.clone()) {
                        Ok(v) => {
                            let mut review = Review::of(&v, params, *builtin, d.shown_name());
                            if let Some(p) = self.project.clone() {
                                let _ = p.read(&mut self.cad.session, |p, _| review.describe(p.ledger.document()));
                            }
                            if let Some(b) = self.library.builder.as_mut() {
                                b.review = Some(review);
                            }
                            self.shell.state.drawer_open = true;
                            return;
                        }
                        Err(e) => Err(e),
                    }
                }
            }
            Source::Make { handles, at, replace, .. } => {
                params["handles"] = json!(handles.iter().map(|h| format!("{h:x}")).collect::<Vec<_>>());
                params["at"] = json!(at);
                params["replace"] = json!(replace);
                self.cad.run("makesymbol", params)
            }
            Source::Promote { component, at, .. } => {
                params["component"] = json!(component);
                params.as_object_mut().map(|o| o.remove("graphics"));
                // Promotion reads slots as page points (its drawing stays where it is).
                if let Some(slots) = params.get_mut("slots").and_then(Value::as_array_mut) {
                    for s in slots {
                        if let (Some(x), Some(y)) = (s["at"][0].as_f64(), s["at"][1].as_f64()) {
                            s["at"] = json!([x + at[0], y + at[1]]);
                        }
                    }
                }
                self.cad.run("makesymbol", params)
            }
        };
        self.after_logic();
        self.saved(r);
    }

    /// After a write from the builder or the review: done (back to the browser with the new tile
    /// selected), or the refusal shown at Save.
    fn saved(&mut self, r: Result<Value, String>) {
        match r {
            Ok(v) => {
                let lineage = v.get("lineage").and_then(Value::as_str).and_then(hex_of::<DefinitionId>);
                let name = self.library.builder.as_ref().map(|b| b.draft.shown_name().to_string()).unwrap_or_default();
                self.close_builder(true);
                self.library.browser.selected = lineage.map(browser::Tile::Project);
                self.library.browser.placed_next = Some(name);
            }
            Err(e) => {
                if let Some(b) = self.library.builder.as_mut() {
                    b.refusal = Some(e.clone());
                    if let Some(r) = b.review.as_mut() {
                        r.refusal = Some(e.clone());
                    }
                }
                self.cad.session.echo(e);
            }
        }
    }

    /// The review's Save: the new version and the chosen updates, one transaction.
    pub fn review_save(&mut self) {
        let Some(r) = self.library.builder.as_ref().and_then(|b| b.review.clone()) else { return };
        let mut params = r.edit_params();
        // Save as a new symbol takes the Details as they are now (a new key, design 048 §6.3).
        if r.choice == review::Choice::New
            && let Some(b) = self.library.builder.as_ref()
        {
            let mut fresh = b.draft.params();
            fresh["definition"] = params["definition"].clone();
            fresh["update"] = json!("new");
            params = fresh;
        }
        let res = self.cad.run("symboledit", params);
        self.after_logic();
        self.saved(res);
    }

    /// Compare (design 048 §8.4): the review row of copy `rep` against its newest version, in
    /// the drawer, with no save.
    pub fn compare_symbol(&mut self, rep: ecad_model::RepresentationId) {
        let Some(p) = self.project.clone() else { return };
        let found = p.read(&mut self.cad.session, |p, _| {
            let doc = p.ledger.document();
            let (shown, _, _) = ecad_cadcraft::symbol_cmds::update_available(doc, rep)?;
            let lineage = symbols::Version::of(shown, doc.definitions.get(&shown)?)?.lineage;
            let newest = symbols::newest(doc, lineage)?;
            Some((shown, Content::of(newest.def), newest.def.name.clone()))
        });
        let Ok(Some((shown, content, name))) = found else {
            self.cad.session.echo("That copy shows the newest version.".to_string());
            return;
        };
        let mut params = Draft::of_content(&content, Source::New).params();
        params["definition"] = json!(shown.hex());
        match self.cad.run("symbolimpact", params.clone()) {
            Ok(v) => {
                let mut review = Review::of(&v, params, false, &name);
                review.only(rep);
                if let Some(p) = self.project.clone() {
                    let _ = p.read(&mut self.cad.session, |p, _| review.describe(p.ledger.document()));
                }
                review.compare = true;
                self.library.compare = Some(review);
                self.shell.state.drawer_open = true;
            }
            Err(e) => self.cad.session.echo(e),
        }
    }

    /// Update symbol on copy `rep` (design 048 §8.4): `symbolupdate`; when it would clash, the
    /// Compare row says why in the drawer.
    pub fn update_symbol(&mut self, rep: ecad_model::RepresentationId) -> Result<(), String> {
        let r = self.cad.run("symbolupdate", json!({"copies": [rep.hex()]}));
        self.after_logic();
        match r {
            Ok(_) => Ok(()),
            Err(e) => {
                self.compare_symbol(rep);
                if let Some(c) = self.library.compare.as_mut() {
                    c.refusal = Some(e.clone());
                }
                Err(e)
            }
        }
    }

    /// Place a project symbol (design 048 §8.2): Draw, and Place Symbol with its key typed.
    pub fn place_project_symbol(&mut self, key: &str) {
        self.set_workspace(crate::shell::Workspace::Draw);
        if self.cad.session.running.is_some() {
            self.cad.session.cancel();
        }
        self.focus_drawing = true;
        if let Err(e) = self.cad.session.start("place") {
            self.cad.session.echo(e.to_string());
            return;
        }
        if let Err(e) = self.cad.session.input(cadcraft_engine::Input::Text(key.to_string())) {
            self.cad.session.echo(e.to_string());
        }
    }
}
