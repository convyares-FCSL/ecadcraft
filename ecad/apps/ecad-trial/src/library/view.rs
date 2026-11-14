//! The Library workspace in the shell's slots: the main view (browser or builder), the right
//! panel (the selected symbol's details, or the builder's Details) and the drawer's review.

use ecad_cadcraft::defaults::symbols::{Field, project_value};
use ecad_cadcraft::symbols;
use ecad_model::DefinitionId;
use eframe::egui::{self, Rect};

use super::browser::{self, Listing, Tile};
use super::builder::{self, Act as BuilderAct};
use super::details::{self, Info};
use super::draft::Source;
use super::review;
use crate::Trial;
use crate::shell::Workspace;

impl Trial {
    /// The Library's main view this frame.
    pub(crate) fn library_view(&mut self, ui: &mut egui::Ui, main: Rect) {
        let sheet = self.shell.state.sheet.tokens();
        let writable = self.project_open() && self.writable();
        if let Some(b) = self.library.builder.as_mut() {
            let acts = crate::shell::in_rect(ui, main, builder::BUILDER, |ui| builder::show(ui, main, b, sheet, writable));
            for a in acts {
                match a {
                    BuilderAct::Back => self.set_workspace(Workspace::Draw),
                    BuilderAct::EditVersion(id) => {
                        let rep = match self.library.builder.as_ref().map(|b| &b.draft.source) {
                            Some(Source::Edit { rep: Some(r), .. }) => u128::from_str_radix(r, 16).ok().map(ecad_model::RepresentationId::from),
                            _ => None,
                        };
                        let back = self.library.back_to_page;
                        self.close_builder(true);
                        self.library.back_to_page = back;
                        if let Err(e) = self.edit_symbol(id, rep) {
                            self.cad.session.echo(e);
                        }
                    }
                }
            }
            self.keep_draft();
            return;
        }
        let key = self.drafts_key();
        let drafts = key.as_deref().map(|k| self.library.drafts.of(k).to_vec()).unwrap_or_default();
        let listing = match self.project.clone().filter(|p| p.is_attached(&self.cad.session)) {
            Some(p) => p.read(&mut self.cad.session, |p, _| Listing::read(Some(p.ledger.document()), &drafts)).unwrap_or_default(),
            None => Listing::read(None, &drafts),
        };
        let acts = crate::shell::in_rect(ui, main, browser::BROWSER, |ui| browser::show(ui, main, &mut self.library.browser, &listing, sheet));
        self.library.listing = listing;
        self.browser_acts(acts);
    }

    fn browser_acts(&mut self, acts: Vec<browser::Act>) {
        for a in acts {
            let r = match a {
                browser::Act::New => {
                    self.new_symbol();
                    Ok(())
                }
                browser::Act::Place(Tile::Project(l)) => {
                    match self.library.listing.get(Tile::Project(l)).map(|e| e.key.clone()).filter(|k| !k.is_empty()) {
                        Some(k) => self.place_project_symbol(&k),
                        None => self.cad.session.echo("That symbol has no key to place it by.".to_string()),
                    }
                    Ok(())
                }
                browser::Act::Place(Tile::Builtin(k)) => {
                    self.set_workspace(Workspace::Draw);
                    self.start_pick(crate::search::Pick::Symbol(k))
                }
                browser::Act::Edit(Tile::Project(l)) => {
                    let newest = self
                        .project
                        .clone()
                        .and_then(|p| p.read(&mut self.cad.session, |p, _| symbols::newest(p.ledger.document(), l).map(|v| v.id)).ok().flatten());
                    self.library.back_to_page = false;
                    newest.ok_or_else(|| "No such symbol.".to_string()).and_then(|id: DefinitionId| self.edit_symbol(id, None))
                }
                browser::Act::Edit(Tile::Builtin(k)) => self.edit_starter(k),
                browser::Act::Resume(n) | browser::Act::Place(Tile::Draft(n)) | browser::Act::Edit(Tile::Draft(n)) => {
                    self.library.back_to_page = false;
                    self.resume_draft(n);
                    Ok(())
                }
                browser::Act::Forget(n) => {
                    if let Some(k) = self.drafts_key() {
                        self.library.drafts.forget(&k, n);
                    }
                    self.library.browser.selected = None;
                    Ok(())
                }
            };
            if let Err(e) = r {
                self.cad.session.echo(e);
            }
        }
    }

    /// The Library's right panel in `rect` (UI rev 3 boards 3.01–3.04: no inspector rail; the
    /// selected symbol, or the builder's Details).
    pub(crate) fn library_panel(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let s = ecad_ui::SHELL;
        ui.painter().rect_filled(rect, 0.0, ecad_ui::color(s.panel));
        ui.painter().vline(rect.left() + 0.5, rect.y_range(), egui::Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, ecad_ui::color(s.line)));
        let body = rect.shrink2(egui::vec2(ecad_ui::tokens::space::L, ecad_ui::tokens::space::L));
        crate::shell::in_rect(ui, body, details::DETAILS, |ui| {
            egui::ScrollArea::vertical().id_salt("library.details.scroll").auto_shrink([false, false]).show(ui, |ui| self.library_right(ui));
        });
    }

    /// The right panel in the Library: the builder's Details, or the selected symbol's.
    pub(crate) fn library_right(&mut self, ui: &mut egui::Ui) {
        let writable = self.project_open() && self.writable();
        if self.library.builder.is_none() {
            let page = self.panel.active.and_then(|a| self.panel.rows().iter().find(|r| r.id == a)).map(|r| r.number.clone());
            let acts = browser::details(ui, &self.library.browser, &self.library.listing, writable, page.as_deref());
            self.browser_acts(acts);
            return;
        }
        let info = self.details_info(writable);
        let Some(b) = self.library.builder.as_mut() else { return };
        let acts = details::show(ui, b, &info);
        for a in acts {
            match a {
                details::Act::Save => self.builder_save(false),
                details::Act::SaveAsNew => self.builder_save(true),
                details::Act::Close(forget) => {
                    self.library.back_to_page = false;
                    self.close_builder(forget);
                }
                details::Act::Standards(key) => {
                    let k = self.library.builder.as_ref().map(|b| b.draft.key.trim().to_string()).unwrap_or_default();
                    self.open_standards(Some(&format!("symbol.{k}.{key}")));
                }
            }
        }
    }

    /// What Details reads from the project.
    fn details_info(&mut self, writable: bool) -> Info {
        let Some(b) = self.library.builder.as_ref() else { return Info::default() };
        let draft = b.draft.clone();
        let edited: Option<DefinitionId> = match &draft.source {
            Source::Edit { definition, .. } => u128::from_str_radix(definition, 16).ok().map(DefinitionId::from),
            _ => None,
        };
        let Some(p) = self.project.clone().filter(|p| p.is_attached(&self.cad.session)) else {
            return Info { source: "No project is open".into(), writable: false, ..Info::default() };
        };
        p.read(&mut self.cad.session, |p, _| {
            let doc = p.ledger.document();
            let version = edited.and_then(|id| doc.definitions.get(&id).map(|d| (id, d)));
            let lineage = version.and_then(|(id, d)| symbols::Version::of(id, d)).map(|v| v.lineage);
            let missing = draft.missing(doc, lineage);
            let mut accepted = draft.clone();
            let missing_after_offers = match accepted.accept_all() {
                Ok(()) => accepted.missing(doc, lineage),
                Err(e) => vec![super::draft::Missing { item: "other", text: e }],
            };
            let key = draft.key.trim().to_string();
            let mut overrides = Vec::new();
            if !key.is_empty() {
                for (name, f) in [("prefix", Field::Prefix), ("description", Field::Description), ("rating", Field::Rating)] {
                    if let Some(v) = project_value(doc, &key, f) {
                        overrides.push((name, v));
                    }
                }
            }
            let versions = lineage.map(|l| {
                symbols::versions_of(doc, l)
                    .iter()
                    .rev()
                    .map(|v| format!("v{} ({} placed)", v.seq, doc.items_using(v.id).len()))
                    .collect::<Vec<_>>()
                    .join(" · ")
            });
            let source = match (&draft.source, version) {
                (Source::Edit { builtin: true, .. }, Some((_, d))) => format!("Project · from {} (built-in v1) · new copy", d.name),
                (Source::Edit { version, .. }, Some((_, d))) => match d.derived_from.as_ref().and_then(symbols::starter_of_origin) {
                    Some(s) => format!("Project · v{version} · from {} (built-in v1)", s.name),
                    None => format!("Project · v{version}"),
                },
                (Source::Make { page_number, .. }, _) => format!("Project · new, from the drawing on page {page_number}"),
                (Source::Promote { tag, .. }, _) => format!("Project · new, from the drawing of {tag} (its IDs and wires are kept)"),
                _ => "Project · new".to_string(),
            };
            Info { missing, missing_after_offers, overrides, versions, source, writable }
        })
        .unwrap_or_default()
    }

    /// The drawer's body in the Library builder (the review) or for Compare: whether it showed.
    pub(crate) fn library_drawer(&mut self, ui: &mut egui::Ui) -> bool {
        let writable = self.project_open() && self.writable();
        let in_builder = self.shell.state.workspace == Workspace::Library;
        let review = if in_builder { self.library.builder.as_mut().and_then(|b| b.review.as_mut()) } else { None };
        let (acts, compare) = match review {
            Some(r) => (review::show(ui, r, writable), false),
            None => match self.library.compare.as_mut() {
                Some(r) => (review::show(ui, r, writable), true),
                None => return false,
            },
        };
        for a in acts {
            match a {
                review::Act::Save => self.review_save(),
                review::Act::Back => {
                    if let Some(b) = self.library.builder.as_mut() {
                        b.review = None;
                    }
                }
                review::Act::Close => self.library.compare = None,
                review::Act::GoTo(rep) => {
                    if let Some(r) = u128::from_str_radix(&rep, 16).ok().map(ecad_model::RepresentationId::from) {
                        self.set_workspace(Workspace::Draw);
                        if let Err(e) = self.goto_symbols(&[r]) {
                            self.cad.session.echo(e);
                        }
                    }
                }
                review::Act::Update(rep) => {
                    if let Some(r) = u128::from_str_radix(&rep, 16).ok().map(ecad_model::RepresentationId::from) {
                        match self.update_symbol(r) {
                            Ok(()) => self.library.compare = None,
                            Err(e) => self.cad.session.echo(e),
                        }
                    }
                }
            }
        }
        let _ = compare;
        true
    }

    /// The drawer's tab title while the review or Compare shows.
    pub fn drawer_review_tab(&self) -> Option<&'static str> {
        let in_builder = self.shell.state.workspace == Workspace::Library;
        if in_builder && self.library.builder.as_ref().is_some_and(|b| b.review.is_some()) {
            Some(review::TAB)
        } else if self.library.compare.is_some() {
            Some(review::COMPARE_TAB)
        } else {
            None
        }
    }
}
