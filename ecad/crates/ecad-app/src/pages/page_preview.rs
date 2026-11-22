//! The read-only preview of another page (brief 016) and of a revision's page.

use crate::*;

impl App {
    /// Preview `page` of the revision opened read-only (design 019 §7.3).
    pub fn preview_revision_page(&mut self, page: PageId) {
        let Some(p) = self.project.clone() else { return };
        let rv = p.read(&mut self.cad.session, |p, _| p.revision_view.clone()).ok().flatten();
        let Some(rv) = rv else { return };
        let mut v = Preview::new(page, None, None);
        v.revision = Some((rv.doc.clone(), format!("Rev {}", rv.rev)));
        if let Some(r) = self.cad.canvas.rect {
            v.place(r);
        }
        self.preview = Some(v);
    }

    /// Preview `page` at its extents (the panel's Preview, Space or context menu). Asked again for
    /// the page previewed now, the preview closes.
    pub fn preview_page(&mut self, page: PageId) {
        if self.preview.as_ref().is_some_and(|v| v.page == page) {
            self.preview = None;
            return;
        }
        if self.panel.active == Some(page) {
            self.panel.note = Some(crate::pages::project_panel::ALREADY_SHOWN.into());
            self.cad.session.echo(crate::pages::project_panel::ALREADY_SHOWN);
            return;
        }
        let mut v = Preview::new(page, None, None);
        if let Some(r) = self.cad.canvas.rect {
            v.place(r);
        }
        self.preview = Some(v);
    }

    /// **Preview Reference** (design 016 §4.4): the selected cable reference's other end, in the
    /// preview. Starts no command and records nothing.
    ///
    /// # Errors
    /// No reference selected, or an orphan (also echoed).
    pub fn preview_reference(&mut self) -> Result<(), String> {
        let r = (|| {
            let st = self.cad.session.state().map_err(|_| ecad_engine::pages::preview::NOT_A_REFERENCE.to_string())?;
            let (uid, handles) = (st.uid, st.selection.clone());
            let p = self.project.clone().ok_or(ecad_engine::pages::preview::NOT_A_REFERENCE)?;
            p.read(&mut self.cad.session, |p, _| {
                let r = ecad_engine::pages::preview::selected_reference(p, uid, &handles).ok_or(ecad_engine::pages::preview::NOT_A_REFERENCE)?;
                ecad_engine::pages::preview::other_end(p, r)
            })?
        })();
        match r {
            Ok((page, at)) => {
                let mut v = Preview::at_reference(page, at);
                if let Some(r) = self.cad.canvas.rect {
                    v.place(r);
                }
                self.preview = Some(v);
                Ok(())
            }
            Err(e) => {
                self.cad.session.echo(e.clone());
                Err(e)
            }
        }
    }

    /// The preview window: brought up to date (only when a change touched its page), shown, and
    /// closed when its page is gone or became active.
    pub(crate) fn preview_window(&mut self, ctx: &egui::Context) {
        let Some(project) = self.project.clone() else {
            self.preview = None;
            return;
        };
        let Some(v) = &mut self.preview else { return };
        if !v.update(&project, &mut self.cad.session) {
            self.preview = None;
            self.cad.session.echo("The previewed page no longer exists.");
            return;
        }
        if self.panel.active == Some(v.page) && v.revision.is_none() {
            self.preview = None;
            return;
        }
        match v.show(ctx) {
            Some(crate::pages::preview::Action::Close) => self.preview = None,
            Some(crate::pages::preview::Action::Open(page, view)) => {
                self.preview = None;
                if self.show_page(page).is_ok()
                    && let Ok(st) = self.cad.session.state_mut()
                {
                    let c = ecad_engine::cad::Vec2::new(view.center.x, view.center.y);
                    let _ = st.set_view(ecad_engine::cad::View { center: c, height: view.height, twist: 0.0 });
                }
            }
            None => {}
        }
    }
}
