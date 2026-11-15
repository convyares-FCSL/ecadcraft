//! Reports, Print Book and Release (briefs 024 and 027).

use crate::*;

impl Trial {
    /// Open the Reports window (design 024 §6.2).
    /// Open Reports and show it: in Lists, or beside the page when Draw's companion is open.
    pub fn reports_view(&mut self) {
        self.open_reports();
        self.shell.lists = shell::ListsView::Reports;
        if !(self.shell.state.workspace == shell::Workspace::Draw && self.shell.state.companion.is_some()) {
            self.set_workspace(shell::Workspace::Lists);
        }
    }

    /// Show Publish on `tab`.
    pub fn publish_view(&mut self, tab: shell::PublishTab) {
        self.shell.publish = tab;
        self.set_workspace(shell::Workspace::Publish);
    }

    pub fn open_reports(&mut self) {
        if self.reports.is_none() {
            let stem = self.output_stem();
            let mut r = outputs_ui::Reports::default();
            r.stem = stem;
            self.reports = Some(r);
        }
    }

    /// Open the Print Book dialog (design 024 §4.1), scoped to `folder` when given.
    pub fn open_print_book(&mut self, folder: Option<ecad_model::FolderId>) {
        let Some(p) = self.project.clone() else { return };
        let folders = p
            .read(&mut self.cad.session, |p, _| {
                let doc = p.ledger.document();
                doc.folders.keys().map(|f| (f.hex(), ecad_cadcraft::sheet_cmds::folder_path(doc, Some(*f)))).collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let chosen = folder.and_then(|f| folders.iter().position(|(h, _)| *h == f.hex()));
        let path = format!("{}.pdf", self.output_stem());
        self.print_book = Some(outputs_ui::PrintBook {
            scope: if chosen.is_some() { outputs_ui::ScopeKind::Folder } else { outputs_ui::ScopeKind::All },
            folders,
            folder: chosen.unwrap_or(0),
            path,
            mono: true,
            lineweights: true,
            ..outputs_ui::PrintBook::default()
        });
    }

    /// The Reports window and the Print Book dialog.
    pub(crate) fn output_windows(&mut self, ctx: &egui::Context) {
        let serial = self.project_serial;
        if let Some(p) = self.project.clone()
            && p.is_attached(&self.cad.session)
            && p.configure(&mut self.cad.session, |p| std::mem::take(&mut p.open_reports)).unwrap_or(false)
        {
            self.reports_view();
        }
        let writable = !self.header().read_only;
        if let (Some(r), Some(p)) = (&mut self.reports, self.project.clone()) {
            r.writable = writable;
            let _ = p.configure(&mut self.cad.session, |p| r.sync(serial, p));
        }
        let out = self.reports.as_mut().filter(|r| !r.hidden).and_then(|r| r.show(ctx));
        match out {
            Some(outputs_ui::Out::Close) => self.reports = None,
            Some(outputs_ui::Out::Export(path)) => self.export_report(&path),
            Some(outputs_ui::Out::Run(line)) => {
                let refusal = self.run_line(&line);
                if let Some(r) = self.reports.as_mut() {
                    r.ran(refusal);
                }
            }
            Some(outputs_ui::Out::GotoAt(page, at)) => self.goto_place(page, at),
            // Brief 028: double-clicking a cable shows its cores.
            Some(outputs_ui::Out::Select(outputs_ui::Tab::Cables, i)) if self.reports.as_ref().is_some_and(|r| r.cores_of.is_none()) => {
                self.show_cores_of_row(i)
            }
            Some(outputs_ui::Out::Goto(tab, i) | outputs_ui::Out::Select(tab, i)) => self.goto_report_row(tab, i),
            None => {}
        }
        self.print_window(ctx);
        self.release_window(ctx);
    }

    /// Write the Reports tab as shown to `path` as CSV (design 024 §6.3).
    pub fn export_report(&mut self, path: &str) {
        let path = self.output_path(path);
        let Some(r) = &mut self.reports else { return };
        let t = r.shown_table();
        let flagged = t.flagged();
        let text = ecad_cadcraft::outputs::csv(&t, true);
        match path.and_then(|path| ecad_cadcraft::printbook::write_file(&path, text.as_bytes()).map(|()| path)) {
            Ok(path) => {
                let marked = if flagged == 1 { "1 cell marked ?".to_string() } else { format!("{flagged} cells marked ?") };
                let m = format!("Wrote {} rows to {path} ({marked}).", t.rows.len());
                r.note = Some(m.clone());
                r.export = None;
                self.cad.session.echo(m);
            }
            Err(e) => r.note = Some(e),
        }
    }

    /// Brief 028: show the core list of the cable on shown row `i` of the Cables tab.
    fn show_cores_of_row(&mut self, i: usize) {
        let Some(k) = self.reports.as_ref().and_then(|r| r.row_index(i)) else { return };
        let Some(p) = self.project.clone() else { return };
        let cable = p.configure(&mut self.cad.session, |p| p.outputs().cables().get(k).map(|c| c.cable)).ok().flatten();
        if let (Some(r), Some(c)) = (&mut self.reports, cable) {
            r.cores_of = Some(c);
        }
    }

    /// Brief 028: Reports on the Terminals tab for `strip` (Properties' **Show plan**).
    pub fn show_plan(&mut self, strip: Option<ecad_model::ComponentId>) {
        self.reports_view();
        if let Some(r) = &mut self.reports {
            r.tab = outputs_ui::Tab::Terminals;
            r.strip = strip;
            r.cores_of = None;
            r.invalidate();
        }
    }

    /// Brief 028: Reports on the Cables tab with `cable`'s cores (Properties' **Show cores**).
    pub fn show_cores(&mut self, cable: ecad_model::CableId) {
        self.reports_view();
        if let Some(r) = &mut self.reports {
            r.tab = outputs_ui::Tab::Cables;
            r.cores_of = Some(cable);
            r.invalidate();
        }
    }

    /// Go to a Reports row: a BOM row's first device's main symbol, a wire's first route, a
    /// cable's label box (design 024 §6.2), a terminal (brief 028). Nothing is recorded.
    pub fn goto_report_row(&mut self, tab: outputs_ui::Tab, i: usize) {
        enum To {
            Symbol(ecad_model::RepresentationId),
            Place(PageId, ecad_model::Pt),
        }
        let Some(k) = self.reports.as_ref().and_then(|r| r.row_index(i)) else { return };
        let Some(p) = self.project.clone() else { return };
        let group = self.reports.as_ref().map(|r| r.group).unwrap_or_default();
        let terminal = self.reports.as_ref().and_then(|r| r.gos.get(k).copied().flatten());
        let cores = self.reports.as_ref().is_some_and(|r| r.cores_of.is_some());
        let to = p
            .configure(&mut self.cad.session, |p| {
                let o = p.outputs();
                match tab {
                    outputs_ui::Tab::Bom => {
                        let row = o.bom(group).into_iter().nth(k)?;
                        let c = *row.devices.first()?;
                        o.device(c).and_then(|d| d.goto).map(|(rep, _, _)| To::Symbol(rep))
                    }
                    outputs_ui::Tab::Wires => o.wires().get(k).and_then(|w| w.go).map(|(pg, at)| To::Place(pg, at)),
                    outputs_ui::Tab::Cables if cores => None,
                    outputs_ui::Tab::Cables => o.cables().get(k).and_then(|c| c.go).map(|(pg, at)| To::Place(pg, at)),
                    outputs_ui::Tab::Terminals => terminal.map(|(pg, at)| To::Place(pg, at)),
                }
            })
            .ok()
            .flatten();
        match to {
            Some(To::Symbol(rep)) => {
                let _ = self.goto_symbol(rep);
            }
            Some(To::Place(page, at)) => self.goto_place(page, at),
            None => {}
        }
    }

    /// The Release badge: counted again only when the kept Checks or report models change.
    pub(crate) fn release_badge(&mut self) -> Option<(String, Option<(usize, usize)>)> {
        let p = self.project.clone()?;
        p.read(&mut self.cad.session, |p, _| (ecad_cadcraft::revisions::pending_rev(p.ledger.document()), release_ui::badge(p))).ok()
    }

    /// Open the Release dialog (design 027 §5.1): the pending revision, or `rev` written again.
    pub fn open_release(&mut self, rev: Option<String>) {
        let read_only = self.header().read_only;
        self.release = Some(release_ui::ReleaseDialog::new(rev, read_only));
        self.refresh_release();
    }

    /// Take the dialog's view again when the project, its kept Checks or the tick changed.
    pub fn refresh_release(&mut self) {
        let Some(p) = self.project.clone() else { return };
        let Some(d) = &self.release else { return };
        let (rev, accept, key) = (d.rev.clone(), d.accept, d.key);
        let r = p.configure(&mut self.cad.session, |p| {
            // An issued revision's view is built once (its state never changes); the pending one
            // again only when what it shows can differ (`release::dialog_key`).
            if key.is_some() && (rev.is_some() || key == ecad_cadcraft::release::dialog_key(p, accept)) {
                return Ok(None);
            }
            let v = ecad_cadcraft::release::view(p, rev.as_deref(), accept, ecad_cadcraft::release::MAX_ROWS)?;
            // The kept lists exist now: the key is read after the view built them.
            let k = if rev.is_some() { 0 } else { ecad_cadcraft::release::dialog_key(p, accept).unwrap_or(0) };
            Ok::<_, String>(Some((v, k)))
        });
        match r {
            Ok(Ok(Some((v, k)))) => {
                if let Some(d) = &mut self.release {
                    d.set_view(v, k);
                }
            }
            Ok(Ok(None)) => {}
            Ok(Err(e)) | Err(e) => {
                if let Some(d) = &mut self.release {
                    d.error = Some(e);
                }
            }
        }
    }

    /// Release with the dialog's choices: `release` through the command path. A folder that
    /// exists asks §9's question; anything else refused stays in the dialog with its way out.
    pub fn release_from_dialog(&mut self, params: serde_json::Value) -> Result<serde_json::Value, String> {
        let r = self.run_value("release", params);
        match &r {
            Ok(_) => {
                // The command echoed what it wrote (design 027 §5.1).
                self.release = None;
                if self.revisions.is_some() {
                    self.open_revisions();
                }
            }
            Err(e) => {
                if let Some(d) = &mut self.release {
                    if e.contains(" exists (") && e.contains("Give existing:") {
                        d.exists = Some(e.clone());
                        d.error = None;
                    } else {
                        d.exists = None;
                        d.error = Some(e.clone());
                    }
                }
            }
        }
        r
    }

    /// The Release dialog, live while open.
    fn release_window(&mut self, ctx: &egui::Context) {
        if self.release.as_ref().is_none_or(|d| d.hidden) {
            return;
        }
        self.refresh_release();
        let answer = self.release.as_mut().and_then(|d| d.show(ctx));
        match answer {
            Some(release_ui::Out::Run(line)) => {
                let e = self.run_line(&line);
                if let Some(d) = &mut self.release {
                    d.error = e;
                }
            }
            Some(release_ui::Out::Release(params)) => {
                let _ = self.release_from_dialog(params);
            }
            Some(release_ui::Out::GoTo(c)) => {
                let _ = self.goto_check(&c);
            }
            Some(release_ui::Out::ShowReports) => {
                self.reports_view();
                if let Some(r) = &mut self.reports {
                    r.tab = outputs_ui::Tab::Bom;
                    r.only_flagged = true;
                    r.invalidate();
                }
            }
            Some(release_ui::Out::Finalise) => self.open_renumber(true, false, None),
            Some(release_ui::Out::ShowChecks) => self.panel.tab = project_panel::Tab::Checks,
            Some(release_ui::Out::SaveAs) => self.request_save_as(),
            Some(release_ui::Out::WriteIssued(rev)) => {
                self.open_release(Some(rev));
                self.publish_view(shell::PublishTab::Release);
            }
            Some(release_ui::Out::Close) => self.release = None,
            None => {}
        }
    }

    /// Print with the dialog's choices (Print): `printbook` through the command path.
    pub fn print_from_dialog(&mut self) -> Result<serde_json::Value, String> {
        let Some(d) = self.print_book.clone() else { return Err("Print Book is not open".into()) };
        let mut params = d.params();
        params["path"] = json!(self.output_path(&d.path)?);
        let r = self.run_value("printbook", params);
        match &r {
            Ok(_) => self.print_book = None,
            Err(e) => {
                if let Some(d) = &mut self.print_book {
                    d.error = Some(e.clone());
                }
            }
        }
        r
    }

    /// The Print Book dialog: live counts, then `printbook` on Print.
    fn print_window(&mut self, ctx: &egui::Context) {
        let Some(d) = self.print_book.as_ref().filter(|d| !d.hidden) else { return };
        let scope = match d.scope {
            outputs_ui::ScopeKind::All => ecad_cadcraft::printbook::Scope::All,
            outputs_ui::ScopeKind::Folder => match d.folders.get(d.folder).and_then(|f| ecad_model::FolderId::parse(&f.0)) {
                Some(f) => ecad_cadcraft::printbook::Scope::Folder(f),
                None => ecad_cadcraft::printbook::Scope::All,
            },
            outputs_ui::ScopeKind::Pages => ecad_cadcraft::printbook::Scope::Pages(d.pages.clone()),
        };
        if let Some(p) = self.project.clone()
            && let Ok((sheets, unknown, flagged)) = p.configure(&mut self.cad.session, |p| ecad_cadcraft::printbook::count(p, &scope))
            && let Some(d) = &mut self.print_book
        {
            d.sheets = sheets;
            d.unknown = unknown;
            d.flagged = flagged;
        }
        if let Some(p) = self.project.clone()
            && let Ok(notes) = p.configure(&mut self.cad.session, |p| ecad_cadcraft::printbook::notes(p, &scope))
            && let Some(d) = &mut self.print_book
        {
            d.notes = notes;
        }
        let answer = self.print_book.as_mut().and_then(|d| d.show(ctx));
        match answer {
            Some(true) => {
                let _ = self.print_from_dialog();
            }
            Some(false) => self.print_book = None,
            None => {}
        }
    }
}
