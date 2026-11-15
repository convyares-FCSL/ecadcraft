//! The page name box (New Page and Rename Page).

use crate::*;

impl Trial {
    pub fn open_new_page_box(&mut self) {
        self.new_page_folder = None;
        let n = self.project_page_count() + 1;
        self.name_box = Some(NameBox { kind: NameBoxKind::New, text: n.to_string(), error: None, focused: false });
    }

    /// Open the page name box to rename `page` (the active page when `None`), prefilled with its
    /// title (Tool Sets' Rename Page). Nothing opens when there is no such page.
    pub fn open_rename_box(&mut self, page: Option<PageId>) {
        let Some(p) = self.project.clone() else { return };
        let found = p.read(&mut self.cad.session, |p, _| {
            let page = page.or_else(|| p.active_page())?;
            Some((page, p.ledger.document().pages.get(&page)?.title.clone()))
        });
        match found {
            Ok(Some((page, title))) => self.name_box = Some(NameBox { kind: NameBoxKind::Rename(page), text: title, error: None, focused: false }),
            _ => self.cad.session.echo("Rename Page: show the page to rename first."),
        }
    }

    fn project_page_count(&mut self) -> usize {
        let Some(p) = self.project.clone() else { return 0 };
        p.read(&mut self.cad.session, |p, _| p.ledger.document().page_order.len()).unwrap_or(0)
    }

    /// OK in the page name box: one `newpage` (the new page is shown at once) or `renamepage`. A
    /// refusal keeps the box open with the message.
    pub fn confirm_name_box(&mut self) {
        let Some(b) = self.name_box.clone() else { return };
        if self.cad.session.running.is_some() {
            self.cad.session.cancel();
        }
        let line = match b.kind {
            NameBoxKind::New => match self.new_page_folder {
                Some(f) => format!("newpage {}", json!({ "title": b.text, "folder": f.hex() })),
                None => format!("newpage {}", json!({ "title": b.text })),
            },
            NameBoxKind::Rename(page) => format!("renamepage {}", json!({"page": page.hex(), "title": b.text})),
        };
        match self.run_line(&line) {
            None => {
                self.name_box = None;
                self.refresh_panel();
                if let Some(page) = self.panel.active {
                    self.panel.reveal(page);
                }
            }
            Some(e) => {
                if let Some(nb) = &mut self.name_box {
                    nb.error = Some(e);
                    nb.focused = false;
                }
            }
        }
    }

    /// Cancel in the page name box: nothing is written.
    pub fn cancel_name_box(&mut self) {
        self.name_box = None;
    }

    pub(crate) fn name_box_dialog(&mut self, ctx: &egui::Context) {
        let Some(b) = &mut self.name_box else { return };
        let heading = match b.kind {
            NameBoxKind::New => "New page",
            NameBoxKind::Rename(_) => "Rename page",
        };
        let (mut ok, mut cancel) = (false, false);
        egui::Modal::new(egui::Id::unique("ecad.page_name_box")).show(ctx, |ui| {
            ui.set_width(320.0);
            ui.label(RichText::new(heading).strong());
            ui.add_space(4.0);
            let id = egui::Id::unique("ecad.page_name_box.field");
            let field = ui.add(egui::TextEdit::singleline(&mut b.text).id(id).desired_width(f32::INFINITY)).accessible_name(NAME_BOX_FIELD);
            if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                ok = true;
            } else if !b.focused {
                field.request_focus();
                if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id) {
                    let all = egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(b.text.chars().count()));
                    state.cursor.set_char_range(Some(all));
                    state.store(ui.ctx(), id);
                    b.focused = true;
                }
            }
            if let Some(e) = &b.error {
                ui.label(RichText::new(e.as_str()).color(ui.visuals().warn_fg_color));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ok |= ui.button("OK").clicked();
                cancel = ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape));
            });
        });
        // The key that closed the box is the box's: CADCraft's command line must not take it too.
        if ok || cancel {
            ctx.input_mut(|i| {
                i.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
                i.consume_key(egui::Modifiers::NONE, egui::Key::Escape);
            });
        }
        if cancel {
            self.cancel_name_box();
        } else if ok {
            self.confirm_name_box();
        }
    }
}
