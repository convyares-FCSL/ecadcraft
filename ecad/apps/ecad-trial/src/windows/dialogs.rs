//! The trial's modal questions: the close question, the file dialog, the issue question, Renumber,
//! merge, and the symbol search window.

use crate::actions::with_param;
use crate::file_ops::in_default_folder;
use crate::*;

impl Trial {
    /// The symbol search window (brief 018): its rows and second step are read from the project
    /// only when the query, the pick or a device, page or setting changed (search.rs's update
    /// map); a chosen row starts Place Symbol with it.
    pub(crate) fn symbol_search(&mut self, ctx: &egui::Context) {
        if !self.search.open {
            return;
        }
        let (project, session, finder, serial) = (&self.project, &mut self.cad.session, &mut self.search, self.project_serial);
        let q = finder.query.clone();
        // Both are copies kept under the search's update map (search.rs): a frame reads nothing
        // from the project unless a change touched a device, a page or the settings.
        let read = project.as_ref().and_then(|p| {
            p.read(session, |p, _| {
                let at = Some((serial, p.change_count()));
                let unchanged = |since: u64| p.device_changes_since(since).is_some_and(|t| t.leaves_search());
                let rows = finder.rows_for(at, unchanged, || search::rows(Some(p.ledger.document()), &q));
                let choices = finder.choices_for(at, unchanged, |pick| search::choices(p, pick));
                (rows, choices)
            })
            .ok()
        });
        let (rows, choices) = match read {
            Some(v) => v,
            None => {
                let rows = finder.rows_for(None, |_| true, || search::rows(None, &q));
                (rows, Vec::new())
            }
        };
        if let Some(choice) = finder.show(ctx, &rows, &choices) {
            // Placing happens on the page: from Library, back to Draw (brief 038).
            self.set_workspace(shell::Workspace::Draw);
            let _ = self.start_choice(&choice);
        }
    }

    /// The provisional question's window: Finalise numbering now, Issue anyway or Cancel.
    pub(crate) fn issue_prompt_window(&mut self, ctx: &egui::Context) {
        let Some((line, text)) = self.issue_prompt.clone() else { return };
        let rev = self.revisions.as_ref().and_then(|d| d.pending.first().cloned()).unwrap_or_default();
        let mut chosen = None;
        egui::Window::new(format!("Issue Rev {rev}"))
            .id(egui::Id::unique("ecad.issue"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(egui::RichText::new(&text).small().monospace());
                ui.horizontal(|ui| {
                    if ui.button("Finalise numbering now").accessible_name("ecad.issue.finalise").clicked() {
                        chosen = Some(1);
                    }
                    if ui.button("Issue anyway").accessible_name("ecad.issue.anyway").clicked() {
                        chosen = Some(2);
                    }
                    if ui.button("Cancel").accessible_name("ecad.issue.cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        chosen = Some(0);
                    }
                });
            });
        match chosen {
            Some(1) => {
                self.issue_prompt = None;
                self.issue_after_finalise = Some(line);
                self.open_renumber(true, false, None);
            }
            Some(2) => {
                self.issue_prompt = None;
                let line = with_param(&line, "provisional", json!("issue"));
                let r = self.run_line(&line);
                if let Some(d) = &mut self.revisions {
                    d.error = r.clone();
                }
                if r.is_none() {
                    self.open_revisions();
                }
            }
            Some(_) => self.issue_prompt = None,
            None => {}
        }
    }

    /// The Renumber dialog per frame: previews on option changes, Apply, go to a row (brief 026).
    pub(crate) fn renumber_window(&mut self, ctx: &egui::Context) {
        let Some(d) = &mut self.renumber else { return };
        match d.show(ctx) {
            Some(renumber_dialog::Out::Preview(c, v)) => {
                let r = self.run_click(c, v);
                if let Some(d) = &mut self.renumber {
                    match r {
                        Ok(v) => {
                            d.preview = v;
                            d.error = None;
                        }
                        Err(e) => d.error = Some(e),
                    }
                }
            }
            Some(renumber_dialog::Out::Apply(c, v)) => match self.run_click(c, v) {
                Ok(_) => {
                    self.renumber = None;
                    // Finalise numbering now (design §5.5): the issue follows, its own step.
                    if let Some(line) = self.issue_after_finalise.take() {
                        let _ = self.run_line(&line);
                        self.open_revisions();
                    }
                }
                Err(e) => {
                    // A stale preview is computed again (design §3).
                    let again = self.renumber.as_ref().and_then(|d| d.params(true).ok());
                    let fresh = again.map(|(c, v)| self.run_click(c, v));
                    if let Some(d) = &mut self.renumber {
                        d.error = Some(e);
                        if let Some(Ok(v)) = fresh {
                            d.preview = v;
                        }
                    }
                }
            },
            Some(renumber_dialog::Out::Goto(page, at)) => {
                if let Some(page) = u128::from_str_radix(&page, 16).ok().map(PageId)
                    && self.show_page(page).is_ok()
                    && let Ok(st) = self.cad.session.state_mut()
                {
                    let height = st.view().height.clamp(GOTO_MIN_HEIGHT, GOTO_MAX_HEIGHT);
                    let _ = st.set_view(cadcraft_engine::View { center: cadcraft_engine::geom::Vec2::new(at[0], at[1]), height, twist: 0.0 });
                }
            }
            Some(renumber_dialog::Out::Cancel) => {
                self.renumber = None;
                self.issue_after_finalise = None;
            }
            None => {}
        }
    }

    /// The merge dialog and the question an undo of a merge stops at (brief 020b).
    pub(crate) fn merge_windows(&mut self, ctx: &egui::Context) {
        if let Some(d) = &mut self.merge {
            match d.show(ctx) {
                Some(merge_dialog::Out::Preview(params)) => match self.run_click("merge", params) {
                    Ok(v) => {
                        if let Some(d) = &mut self.merge {
                            d.preview = v;
                            d.error = None;
                        }
                    }
                    Err(e) => {
                        if let Some(d) = &mut self.merge {
                            d.error = Some(e);
                        }
                    }
                },
                Some(merge_dialog::Out::Merge(params)) => match self.run_click("merge", params) {
                    Ok(_) => {
                        self.merge = None;
                        self.panel.history.done();
                        self.sync_branch();
                    }
                    Err(e) => {
                        if let Some(d) = &mut self.merge {
                            d.error = Some(e);
                        }
                        self.sync_branch();
                    }
                },
                Some(merge_dialog::Out::Cancel) => {
                    // The prepared states are dropped with the dialog.
                    self.merge = None;
                    if let Some(p) = self.project.clone() {
                        let _ = p.configure(&mut self.cad.session, |p| p.merge_cache = None);
                    }
                }
                None => {}
            }
        }
        let question = self.project.clone().and_then(|p| p.read(&mut self.cad.session, |p, _| p.merge_undo_question.clone()).ok().flatten());
        if let Some(q) = question {
            match merge_dialog::undo_merge(ctx, &q) {
                Some(true) => {
                    let _ = self.run_line("undomerge {}");
                    self.panel.history.invalidate();
                }
                Some(false) => {
                    if let Some(p) = self.project.clone() {
                        let _ = p.configure(&mut self.cad.session, |p| p.merge_undo_question = None);
                    }
                }
                None => {}
            }
        }
    }

    pub(crate) fn dialogs(&mut self, ctx: &egui::Context) {
        if let Some(h) = &self.held {
            self.close_dialog(ctx, h.title.clone(), h.changes);
        } else if let Some((source, output)) = self.convert_offer.clone() {
            let mut answer = None;
            egui::Modal::new(egui::Id::unique("ecad_trial_convert")).show(ctx, |ui| {
                ui.set_width(520.0);
                ui.label(RichText::new("Convert this project to the new file format?").strong());
                ui.add_space(4.0);
                ui.label(format!("{source} is a version 1 project file. This version saves projects in version 2."));
                ui.label(format!(
                    "Converting writes a new file, {output}, checks it by replaying both, and opens it. \
                     The original file is left untouched."
                ));
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("Convert and open").clicked() {
                        answer = Some(true);
                    }
                    if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        answer = Some(false);
                    }
                });
            });
            match answer {
                Some(true) => {
                    if let Err(e) = self.accept_conversion() {
                        self.notice = Some(format!("The project was not converted.\n\n{e}"));
                    }
                }
                Some(false) => self.decline_conversion(),
                None => {}
            }
        } else if self.dialog.is_some() {
            self.file_dialog(ctx);
        } else if self.name_box.is_some() {
            self.name_box_dialog(ctx);
        } else if let Some(msg) = self.notice.clone() {
            let mut ok = false;
            egui::Modal::new(egui::Id::unique("ecad_trial_notice")).show(ctx, |ui| {
                ui.set_width(460.0);
                ui.label(RichText::new("ECAD trial").strong());
                ui.add_space(4.0);
                ui.label(msg);
                ui.add_space(10.0);
                ok = ui.button("OK").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape));
            });
            if ok {
                self.notice = None;
            }
        }
    }

    fn close_dialog(&mut self, ctx: &egui::Context, title: String, changes: u64) {
        let file = self.status().map(|s| s.file).unwrap_or_default();
        let name = self.status().map_or_else(|| PROJECT_NAME.to_string(), |s| s.name);
        let mut answer = None;
        egui::Modal::new(egui::Id::unique("ecad_trial_close")).show(ctx, |ui| {
            ui.set_width(460.0);
            ui.label(RichText::new(format!("Save changes to project \"{name}\"?")).strong());
            ui.add_space(4.0);
            let _ = title;
            ui.label(format!("Closing the project. It has {changes} unsaved change{}.", if changes == 1 { "" } else { "s" }));
            ui.label(match (&file.path, &file.read_only) {
                (Some(_), Some(r)) => format!("The project is read-only ({r}); Save asks for another file name."),
                (Some(p), None) => format!("Save writes them to {p}."),
                (None, _) => "The project has no file yet; Save asks for a file name.".to_string(),
            });
            ui.label(RichText::new("Discard closes it and loses the unsaved changes.").color(ui.visuals().warn_fg_color));
            if self.snapshots.is_some() {
                ui.label(format!("Discard exports a snapshot first. {SNAPSHOT_NOTE}"));
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    answer = Some(Answer::Save);
                }
                if ui.button("Discard").clicked() {
                    answer = Some(Answer::Discard);
                }
                if ui.button("Cancel").clicked() {
                    answer = Some(Answer::Cancel);
                }
            });
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                answer = Some(Answer::Cancel);
            }
        });
        if let Some(a) = answer {
            self.answer(a);
            ctx.request_repaint();
        }
    }

    fn file_dialog(&mut self, ctx: &egui::Context) {
        let Some(d) = &mut self.dialog else { return };
        let (heading, action) = match d.kind {
            DialogKind::Open => ("Open project", "Open"),
            DialogKind::SaveAs => ("Save project as", "Save"),
        };
        let mut confirm = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::unique("ecad_trial_file")).show(ctx, |ui| {
            ui.set_width(520.0);
            ui.label(RichText::new(heading).strong());
            ui.add_space(4.0);
            ui.label(format!(
                "A .ecad project file (the extension is added if missing). Keep projects in a local folder, for \
example {}; other locations are unverified.",
                in_default_folder("demo.ecad")
            ));
            let field = ui.add(egui::TextEdit::singleline(&mut d.path).desired_width(f32::INFINITY));
            // Enter ends the text field's input (it gives up focus): checked before focus is
            // requested, which would otherwise cancel that.
            if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                confirm = true;
            } else if !d.focused {
                field.request_focus();
                d.focused = true;
            }
            if let Some(e) = &d.error {
                ui.label(RichText::new(e.as_str()).color(ui.visuals().warn_fg_color));
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                confirm |= ui.button(action).clicked();
                cancel = ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape));
            });
        });
        if cancel {
            self.cancel_dialog();
        } else if confirm {
            self.confirm_dialog();
        }
        if cancel || confirm {
            ctx.request_repaint();
        }
    }
}
