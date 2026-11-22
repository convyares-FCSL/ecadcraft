//! Project Properties, Revisions and the ECAD section of the Properties palette (brief 019).

use crate::*;

impl App {
    /// Open the Project info or Project settings popout: since brief 042, Settings ▸ Project at
    /// that tab, filled from `projectprops {}` (reads only).
    pub fn open_project_popout(&mut self, popout: sheets_ui::Popout) {
        self.fill_project_props(popout);
        if self.project_props.is_some() {
            // UI rev 3: Project info, or Pages and sheets for the project's settings.
            let section = match popout {
                sheets_ui::Popout::Info => settings::Section::Project,
                sheets_ui::Popout::Settings => settings::Section::Pages,
            };
            // Brief 054: Project settings opens Pages and sheets at the project's own rows.
            if popout == sheets_ui::Popout::Settings {
                self.settings.pages.project_open = true;
            }
            self.open_settings(section);
        }
    }

    /// Open the Revisions dialog, filled from `projectprops {}` (reads only).
    pub fn open_revisions(&mut self) {
        let read_only = self.header().read_only;
        let off_main = self.branch_info().filter(|b| !b.release_line).map(|b| b.name);
        match self.run_value("projectprops", json!({})) {
            Ok(v) => {
                let mut r = sheets_ui::Revisions::from_json(&v, read_only || off_main.is_some());
                r.off_main = off_main;
                self.fill_folders(&mut r);
                self.revisions = Some(r);
                self.revisions_seen = self.change_count();
            }
            Err(e) => self.cad.session.echo(e),
        }
    }

    /// The project's change count (0 without a project).
    pub(crate) fn change_count(&mut self) -> u64 {
        let Some(p) = self.project.clone() else { return 0 };
        p.read(&mut self.cad.session, |p, _| p.change_count()).unwrap_or(0)
    }

    /// Bring the open Revisions dialog up to date after a change of its update map (the project's
    /// properties, issuing, undo and redo of them), keeping unsent typing. One integer compared
    /// per frame otherwise.
    fn refresh_revisions(&mut self) {
        if self.revisions.is_none() {
            return;
        }
        let Some(p) = self.project.clone() else { return };
        let seen = self.revisions_seen;
        let Ok((count, touched)) = p.read(&mut self.cad.session, |p, _| {
            let count = p.change_count();
            (count, if count == seen { Some(Default::default()) } else { p.props_changes_since(seen) })
        }) else {
            return;
        };
        self.revisions_seen = count;
        let hit = touched.is_none_or(|t: std::collections::BTreeSet<ecad_model::ObjectRef>| t.contains(&ecad_model::ObjectRef::Project));
        if !hit {
            return;
        }
        let read_only = self.header().read_only;
        if let Ok(v) = self.run_value("projectprops", json!({})) {
            let mut fresh = sheets_ui::Revisions::from_json(&v, read_only);
            self.fill_folders(&mut fresh);
            if let Some(d) = &mut self.revisions {
                d.refresh(fresh);
            }
        }
    }

    /// Bring the Properties section's copy up to date (design 019 §9): rebuilt on a new selection
    /// or page, or after a change that touched what it shows; otherwise kept.
    pub fn sync_props(&mut self) {
        let Some(p) = self.project.clone().filter(|p| p.is_attached(&self.cad.session)) else {
            if let Ok(mut sh) = self.props.lock() {
                sh.view = ecad_electrical::props::PropsView::default();
            }
            self.props_seen = None;
            return;
        };
        let owned = self.cad.session.state().is_ok_and(|st| self.cad.session.ext.owner_of(st.uid).is_some());
        let selection = if owned { self.cad.session.selection() } else { Vec::new() };
        let serial = self.project_serial;
        let seen = &self.props_seen;
        let read = p.read(&mut self.cad.session, |p, _| {
            let page = p.active_page();
            let same = seen.as_ref().is_some_and(|(s, _, pg, sel)| *s == serial && *pg == page && *sel == selection);
            // Nothing changed since the copy was built: nothing to look at (the usual frame).
            let since = seen.as_ref().map(|(_, n, _, _)| *n);
            let touched = match since {
                Some(n) if n == p.change_count() => Some(Default::default()),
                Some(n) => p.props_changes_since(n),
                None => None,
            };
            (page, p.change_count(), same, touched)
        });
        let Ok((page, count, same, touched)) = read else { return };
        let stale = !same
            || touched.is_none_or(|t: std::collections::BTreeSet<ecad_model::ObjectRef>| {
                self.props.lock().map_or(true, |sh| t.iter().any(|o| sh.view.deps.contains(o)))
            });
        if !stale {
            if let Some(seen) = &mut self.props_seen {
                seen.1 = count;
            }
            return;
        }
        let view = p.read(&mut self.cad.session, |p, _| ecad_engine::props::view(p, page, &selection));
        if let Ok(view) = view {
            let writable = !self.header().read_only;
            if let Ok(mut sh) = self.props.lock() {
                // A refusal stays under its field until what the section shows changes.
                if sh.view != view {
                    sh.errors.clear();
                }
                sh.view = view;
                sh.writable = writable;
            }
            self.props_rebuilds += 1;
            self.props_seen = Some((serial, count, page, selection));
        }
    }

    /// Run what the Properties section asked for: each edit through its command's JSON form, a
    /// refusal kept under its field.
    pub(crate) fn take_props(&mut self) {
        let (edits, buttons) = match self.props.lock() {
            Ok(mut sh) => (std::mem::take(&mut sh.edits), std::mem::take(&mut sh.buttons)),
            Err(_) => return,
        };
        // A command waiting at a prompt is cancelled first (nothing written), as a page click does.
        if (!edits.is_empty() || !buttons.is_empty()) && self.cad.session.running.is_some() {
            self.cad.session.cancel();
        }
        for (target, key, value) in edits {
            let line = ecad_electrical::props::edit_line(&target, &key, value.as_deref());
            let r = self.run_line(&line);
            // A clash box's choice (`tag#swap`, brief 026) answers the Tag field's refusal.
            let key = key.split('#').next().unwrap_or_default().to_string();
            if let Ok(mut sh) = self.props.lock() {
                match r {
                    Some(e) => {
                        sh.errors.insert(key, e);
                    }
                    None => {
                        sh.errors.remove(&key);
                    }
                }
            }
        }
        for b in buttons {
            match b.as_str() {
                "revisions" => self.open_revisions(),
                "more" => self.open_project_popout(sheets_ui::Popout::Info),
                g => {
                    if let Some(r) = g.strip_prefix("goto:") {
                        let _ = self.run_line(&format!("cablegoto {}", json!({ "reference": r })));
                    } else if let Some(c) = g.strip_prefix("showplan:") {
                        // Brief 028: Reports ▸ Terminals on this strip.
                        self.show_plan(ecad_model::DeviceId::parse(c));
                    } else if let Some(k) = g.strip_prefix("showcores:").and_then(ecad_model::CableId::parse) {
                        self.show_cores(k);
                    } else if let Some(rest) = g.strip_prefix("refgoto:") {
                        // Brief 025: a device reference's Go to.
                        let mut it = rest.splitn(3, ':');
                        let (source, target) = (it.next().unwrap_or_default(), it.next().unwrap_or_default());
                        let mut params = json!({ "source": source });
                        if !target.is_empty() {
                            params["target"] = json!(target);
                        }
                        let _ = self.run_line(&format!("refgoto {params}"));
                    }
                }
            }
        }
    }

    /// Brief 027: the issued rows' release folders, read from disk now.
    fn fill_folders(&mut self, r: &mut sheets_ui::Revisions) {
        if let Some(p) = self.project.clone() {
            let _ = p.read(&mut self.cad.session, |p, _| release_ui::fill_folders(&mut r.issued, p));
        }
    }

    /// The Revisions window and the question a plain undo across an issue
    /// stops at.
    pub(crate) fn sheet_windows(&mut self, ctx: &egui::Context) {
        // Brief 042: the project popouts are Settings sections (`settings::project`).
        self.refresh_revisions();
        if let Some(d) = &mut self.revisions {
            match d.show(ctx) {
                Some(sheets_ui::Out::Run(line)) => {
                    let issuing = line.starts_with("issuerevision");
                    let r = self.run_line(&line);
                    // Brief 026 (design §5.5): provisional items ask first.
                    let provisional = issuing && r.as_deref().is_some_and(|e| e.contains("provisional: \"finalise\""));
                    if provisional {
                        self.ask_issue_provisional(line);
                    } else {
                        if let Some(d) = &mut self.revisions {
                            d.error = r.clone();
                        }
                        if r.is_none() && issuing {
                            self.open_revisions();
                        }
                    }
                }
                Some(sheets_ui::Out::Open(rev)) => {
                    if let Some(e) = self.run_line(&format!("openrevision {}", json!({ "rev": rev })))
                        && let Some(d) = &mut self.revisions
                    {
                        d.error = Some(e);
                    }
                    self.refresh_panel();
                }
                Some(sheets_ui::Out::Close) => self.revisions = None,
                Some(sheets_ui::Out::SwitchToMain) => self.ask_switch("main", "main"),
                Some(sheets_ui::Out::MergeIntoMain) => {
                    self.revisions = None;
                    let shown = self.branch_info().map(|b| b.name).unwrap_or_default();
                    self.ask_merge(json!({"from": shown, "into": "main"}));
                }
                Some(sheets_ui::Out::Release(rev)) => {
                    self.open_release(rev);
                    self.publish_view(shell::PublishTab::Release);
                }
                Some(sheets_ui::Out::OpenFolder(f)) => {
                    if let Err(e) = release_ui::open_folder(&f)
                        && let Some(d) = &mut self.revisions
                    {
                        d.error = Some(e);
                    }
                }
                Some(sheets_ui::Out::OpenRevisions) | None => {}
            }
        }
        let question = self.project.clone().and_then(|p| p.read(&mut self.cad.session, |p, _| p.issue_undo_question.clone()).ok().flatten());
        if let Some(q) = question {
            match sheets_ui::issue_undo(ctx, &q) {
                Some(true) => {
                    let _ = self.run_line("undoissue {}");
                }
                Some(false) => {
                    if let Some(p) = self.project.clone() {
                        let _ = p.configure(&mut self.cad.session, |p| p.issue_undo_question = None);
                    }
                }
                None => {}
            }
        }
    }
}
