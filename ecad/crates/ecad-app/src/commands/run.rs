//! What the app runs: command lines, Tool Sets buttons, the Project panel's, history's and
//! debugger's actions, branch and merge questions, Renumber, and going to places in the drawing.

use crate::shell::app::*;
use crate::{developer, settings, shell};
use ecad_model::PageId;
use eframe::egui::{self};
use serde_json::json;

impl App {
    /// Run one whole command line, as typed in the banner's field: `name {json}` runs an ECAD or
    /// CADCraft command with parameters; anything else is read like a script line (spaces separate
    /// inputs). Returns the error, which is also echoed at the command line.
    ///
    /// Spaces are inputs: a trailing space is an Enter (`line 0,0 10,0 ` ends LINE), and while a
    /// command is running an empty line is an Enter too. Only leading spaces before a new command
    /// are dropped.
    ///
    /// A line still holding a `<placeholder>` of a template put there by an ECAD Tool Sets button
    /// is refused, so a template is never run with its placeholders.
    pub fn submit_command(&mut self, text: &str) -> Option<String> {
        let running = self.cad.session.running.is_some();
        let text = if running { text } else { text.trim_start() };
        if text.is_empty() && !running {
            return None;
        }
        if self.command_hint.is_some() {
            let left = crate::commands::tools::placeholders(text);
            if !left.is_empty() {
                let e = format!("Replace the placeholders first: {}", left.join(", "));
                self.cad.session.echo(e.clone());
                return Some(e);
            }
        }
        let r = self.run_line(text);
        if r.is_none() {
            self.command_hint = None;
        }
        r
    }

    /// Run one command line through the command path, as the command field does.
    pub(crate) fn run_line(&mut self, text: &str) -> Option<String> {
        self.cad.session.echo(format!("Command: {text}"));
        let r = self.cad.cmdline(text);
        self.after_logic();
        r
    }

    /// What to fill in for the template in the command field, if one was put there.
    pub fn command_hint(&self) -> Option<&str> {
        self.command_hint
    }

    /// Act on a click of an ECAD Tool Sets button (see [`tools`]): start its command's interactive
    /// form (as typing its bare name does; the Command field is cleared and the drawing gets the
    /// keyboard, so the next key answers the prompt), run its command through the command line
    /// (the same path as typing it), or put its template in the command field. The selection is
    /// left as it is. A command waiting at a prompt is cancelled first, as CADCraft's own Tool Sets
    /// buttons do, so the button's line is never taken as prompt input.
    ///
    /// # Errors
    /// An unknown tool, or the command's own refusal (also echoed at the command line).
    pub fn tool(&mut self, id: &str) -> Result<(), String> {
        let Some(tool) = crate::commands::tools::find(id) else {
            let e = format!("unknown ECAD tool {id}");
            self.cad.session.echo(e.clone());
            return Err(e);
        };
        if self.cad.session.running.is_some() {
            self.cad.session.cancel();
        }
        match tool.kind {
            crate::commands::tools::Kind::Start(command) => {
                self.command.clear();
                self.command_hint = None;
                self.focus_drawing = true;
                let r = self.cad.session.start(command).map_err(|e| e.to_string());
                if let Err(e) = &r {
                    self.cad.session.echo(e.clone());
                }
                self.after_logic();
                r
            }
            // A template waiting in the command field keeps its note and its placeholder check.
            crate::commands::tools::Kind::Run(line) => match self.run_line(line) {
                None => Ok(()),
                Some(e) => Err(e),
            },
            crate::commands::tools::Kind::Search => {
                self.search.toggle();
                Ok(())
            }
            crate::commands::tools::Kind::Template => {
                let picked = self.picked();
                let (line, hint) = crate::commands::tools::template(id, &picked).ok_or_else(|| format!("no template for {id}"))?;
                self.command = line;
                self.command_hint = Some(hint);
                self.focus_command = true;
                self.developer.open = true;
                Ok(())
            }
            crate::commands::tools::Kind::App => self.app_tool(id),
        }
    }

    /// The tools the app handles itself: Preview Reference and the ECAD Dev tab.
    fn app_tool(&mut self, id: &str) -> Result<(), String> {
        match id {
            // The Renumber dialog (Renumber Wires opens it with Wires chosen).
            "ecad.renumber" => {
                self.open_renumber(false, false, None);
                Ok(())
            }
            "ecad.renumberwires" => {
                self.open_renumber(false, true, None);
                Ok(())
            }
            "ecad.finalise" => {
                self.open_renumber(true, false, None);
                Ok(())
            }
            "ecad.cablepreview" => self.preview_reference(),
            "ecad.dev.debugger" => {
                self.toggle_debugger();
                Ok(())
            }
            "ecad.dev.snapshot" => {
                let r = self.export_snapshot("manual");
                self.notice = Some(match &r {
                    Ok(p) => format!("Snapshot written to {}.\n\n{SNAPSHOT_NOTE}", p.display()),
                    Err(e) => format!("Snapshot not written: {e}"),
                });
                r.map(|_| ())
            }
            "ecad.dev.command" => {
                self.developer.open = true;
                self.focus_command = true;
                Ok(())
            }
            _ => Err(format!("unknown ECAD tool {id}")),
        }
    }

    /// The current selection and the ECAD objects it names (empty off an ECAD page).
    pub(crate) fn picked(&mut self) -> crate::commands::tools::Picked {
        let Ok(st) = self.cad.session.state() else { return crate::commands::tools::Picked::default() };
        let (uid, handles) = (st.uid, st.selection.clone());
        let Some(p) = self.project.as_ref() else { return crate::commands::tools::Picked { handles, ..Default::default() } };
        let fallback = crate::commands::tools::Picked { handles: handles.clone(), ..Default::default() };
        p.read(&mut self.cad.session, |p, _| crate::commands::tools::Picked::from_selection(p, uid, handles)).unwrap_or(fallback)
    }

    /// Act on every ECAD Tool Sets click waiting since the last frame, each once.
    pub(crate) fn handle_tools(&mut self) {
        for id in self.cad.take_tool_requests() {
            // Refusals are echoed at the command line by the command path itself.
            let _ = self.tool(&id);
        }
    }

    /// Start Place Symbol for a choice of the search window's second step: the symbol, and the
    /// device it joins (as `D` and its tag would), so the next click on the page places it.
    ///
    /// # Errors
    /// The command's refusal.
    pub fn start_choice(&mut self, choice: &crate::library::search::Choice) -> Result<(), String> {
        self.start_pick(crate::library::search::Pick::Symbol(choice.symbol))?;
        let r = match &choice.device {
            Some(tag) => self
                .cad
                .session
                .input(ecad_engine::cad::Input::Keyword("Device".into()))
                .and_then(|()| self.cad.session.input(ecad_engine::cad::Input::Text(tag.clone())))
                .map_err(|e| e.to_string()),
            None => Ok(()),
        };
        if let Err(e) = &r {
            self.cad.session.echo(e.clone());
        }
        self.after_logic();
        r
    }

    /// Start Place Symbol for a search row: a symbol goes to the insertion point, a device asks
    /// which symbol to add to it. Refusals are echoed at the command line.
    ///
    /// # Errors
    /// The command's refusal.
    pub fn start_pick(&mut self, pick: crate::library::search::Pick) -> Result<(), String> {
        if self.cad.session.running.is_some() {
            self.cad.session.cancel();
        }
        // The picker's chip names the symbol being placed.
        self.shell.tools.placing.symbol = match &pick {
            crate::library::search::Pick::Symbol(k) => Some(*k),
            _ => None,
        };
        let text = match pick {
            crate::library::search::Pick::Symbol(k) => k.to_string(),
            crate::library::search::Pick::Device(tag) => tag,
            crate::library::search::Pick::Project(k) => k,
            // Pages and actions are run by the shell, not placed.
            crate::library::search::Pick::Page(_) | crate::library::search::Pick::Action(_) => return Ok(()),
        };
        let r = self.cad.session.start("place").and_then(|()| self.cad.session.input(ecad_engine::cad::Input::Text(text))).map_err(|e| e.to_string());
        if let Err(e) = &r {
            self.cad.session.echo(e.clone());
        }
        self.focus_drawing = true;
        self.after_logic();
        r
    }

    /// Show or hide the debugger, inline in the Project panel's Developer section (which unfolds).
    pub fn toggle_debugger(&mut self) {
        self.debugger.open = !self.debugger.open;
        if self.debugger.open {
            self.developer.open = true;
            self.developer.body = developer::Body::Debugger;
            self.refresh_debugger(false);
        } else if self.developer.body == developer::Body::Debugger {
            self.developer.body = developer::Body::None;
        }
    }

    /// **Inspect Project** in the Developer section: a readable summary of the project (pages,
    /// devices, cables, counts, file and transaction), shown there. Read only.
    pub fn inspect_project(&mut self) {
        self.developer.open = true;
        self.developer.body = developer::Body::Inspect;
        let Some(p) = self.project.clone() else {
            self.developer.inspect = Some("No ECAD project is open.".into());
            return;
        };
        let file = self.status().map(|s| status_line(&s)).unwrap_or_default();
        let text = p.read(&mut self.cad.session, |p, _| {
            let doc = p.ledger.document();
            let mut out = vec![file, format!("Transaction {} (saved through {}).", p.ledger.head(), p.ledger.persisted())];
            out.push(String::new());
            out.push(format!("Pages ({}):", doc.page_order().len()));
            for (i, page) in doc.page_order().iter().enumerate() {
                let title = doc.pages().get(page).map(|x| x.title.as_str()).unwrap_or_default();
                let items = doc.items().values().filter(|it| it.page == *page).count();
                out.push(format!("  {:>3}  {title}  ({items} items)  {}", i + 1, page.hex()));
            }
            let devices = ecad_electrical::devices::json::devices(doc, None);
            out.push(String::new());
            out.push(format!("Devices ({}):", devices.len()));
            for c in &devices {
                out.push(format!(
                    "  {:<8} {}  ({})  {}",
                    ecad_electrical::devices::tags::tag_words(doc, *c),
                    ecad_electrical::devices::tags::description(doc, *c),
                    ecad_electrical::devices::tags::pages_words(doc, *c),
                    c.hex()
                ));
            }
            out.push(String::new());
            out.push(format!("Cables ({}):", doc.cables().len()));
            for (k, cable) in doc.cables() {
                out.push(format!("  {:<8} {} members  {}", ecad_electrical::cables::cable_title(doc, *k), cable.cores.len(), k.hex()));
            }
            out.push(String::new());
            out.push(format!(
                "Counts: {} components, {} connection points, {} conductors, {} items, {} representations, {} definitions.",
                doc.devices().len(),
                doc.connection_points().len(),
                doc.wires().len(),
                doc.items().len(),
                doc.representations().len(),
                doc.definitions().len()
            ));
            out.join("\n")
        });
        self.developer.inspect = Some(text.unwrap_or_else(|e| format!("The project could not be read: {e}")));
    }

    /// Act on what the Developer section asked for.
    pub(crate) fn developer_action(&mut self, a: developer::Action) {
        match a {
            developer::Action::Run(line) => {
                let n = self.cad.session.log.len();
                let failed = self.submit_command(&line);
                self.developer.reply = self.cad.session.log.iter().skip(n).filter(|l| !l.starts_with("Command: ")).take(40).cloned().collect();
                if failed.is_some() {
                    self.command = line; // kept for correction
                }
            }
            developer::Action::ToggleDebugger => self.toggle_debugger(),
            developer::Action::AgentMode(m) => self.agent_set_mode(m),
            developer::Action::AgentPort => self.agent_apply_port(),
            developer::Action::Banner(a) => self.agent_action(a),
            developer::Action::Inspect => self.inspect_project(),
            developer::Action::Snapshot => {
                let _ = self.app_tool("ecad.dev.snapshot");
            }
            developer::Action::Debugger(a) => {
                let close = a == crate::developer::debugger::Action::Close;
                self.debugger_action(a);
                if close {
                    self.developer.body = developer::Body::None;
                }
            }
        }
    }

    /// Bring the debugger's snapshot up to date (a new one only when something changed, or
    /// always with `force`). The project is locked only while it is copied.
    pub fn refresh_debugger(&mut self, force: bool) {
        if let Some(p) = &self.project {
            self.debugger.refresh(p, &mut self.cad.session, force);
        }
    }

    /// Apply a debugger action that needs the project (Refresh, a check).
    pub fn debugger_action(&mut self, a: crate::developer::debugger::Action) {
        if let Some(p) = &self.project {
            self.debugger.apply(a, p, &mut self.cad.session);
        }
    }

    /// Act on one thing the Project panel asked for. Writes go through the page commands' JSON
    /// forms on the command path, so each is exactly that command's transaction and one undo step.
    pub fn panel_action(&mut self, a: crate::pages::project_panel::Action) {
        use crate::pages::project_panel::Action as A;
        self.panel.note = None;
        match a {
            A::Open(page) => {
                // A plain page switch gets the page's stored view back.
                self.shell.restore_view = true;
                let _ = self.show_page(page);
            }
            A::New => self.open_new_page_box(),
            A::Rename(page, title) => {
                let line = format!("renamepage {}", json!({"page": page.hex(), "title": title}));
                match self.run_line(&line) {
                    None => self.panel.rename = None,
                    Some(e) => {
                        if let Some(r) = &mut self.panel.rename {
                            r.text = title;
                            r.error = Some(e);
                        }
                    }
                }
            }
            A::Move(page, index) => {
                let _ = self.run_line(&format!("movepage {}", json!({"page": page.hex(), "index": index})));
            }
            A::Preview(page) => self.preview_page(page),
            A::NewProject => {
                if let Err(e) = self.new_project() {
                    self.notice = Some(e);
                }
            }
            A::OpenProject => {
                if self.project_open() {
                    self.notice = Some("Close this project first (Close, in the Project panel).".into());
                } else {
                    self.request_open();
                }
            }
            A::Save => {
                let _ = self.save();
            }
            A::SaveAs => self.request_save_as(),
            A::CloseProject => self.close_project(),
            A::RenameInBox(page) => self.open_rename_box(Some(page)),
            A::GotoSymbols(reps) => {
                let _ = self.goto_symbols(&reps);
            }
            A::GotoCheck(check) => {
                let _ = self.goto_check(&check);
            }
            A::RenumberPage(page) => {
                if self.show_page(page).is_ok() {
                    self.panel.tab = crate::pages::project_panel::Tab::Pages;
                    self.panel.start_number_edit(page);
                }
            }
            A::MoveTable(check) => {
                if self.goto_check(&check).is_ok() {
                    if self.cad.session.running.is_some() {
                        self.cad.session.cancel();
                    }
                    if let Err(e) = self.cad.session.start("mirrormove") {
                        self.cad.session.echo(e.to_string());
                    }
                    self.after_logic();
                }
            }
            A::JoinWires(page, at) => {
                if self.show_page(page).is_ok() {
                    let _ = self.run_line(&format!("joinwires {}", json!({"at": [at.x, at.y]})));
                }
            }
            A::NumberThem => {
                let _ = self.run_line(&format!("renumberwires {}", json!({"scope": "unnumbered"})));
            }
            A::Finalise => self.open_renumber(true, false, None),
            A::Renumber => self.open_renumber(false, false, None),
            A::RenumberFolder(f) => self.open_renumber(false, false, Some(f)),
            A::UseContext(c, a) => {
                let code = self.project.clone().and_then(|p| {
                    p.read(&mut self.cad.session, |p, _| ecad_electrical::locations::shared_context(p.ledger.document(), c, a)).ok().flatten()
                });
                if let Some(code) = code {
                    let _ = self.run_line(&format!("setlocation {}", json!({"component": c.hex(), "aspect": a.word(), "value": code})));
                }
            }
            A::GoToDevice(c) => {
                let rep = self.project.clone().and_then(|p| {
                    p.read(&mut self.cad.session, |p, _| ecad_electrical::symbols::reps::goto(p.ledger.document(), c, None)).ok().flatten()
                });
                if let Some(rep) = rep {
                    let _ = self.goto_symbol(rep);
                }
            }
            A::ShowDesignations => self.open_settings(settings::Section::Project),
            A::ShowReports => {
                self.reports_view();
                if let Some(r) = &mut self.reports {
                    r.tab = crate::reports::panel::Tab::Bom;
                    r.only_flagged = true;
                    r.invalidate();
                }
            }
            // The terminal strip Checks fixes.
            A::OfferDrawn(c) => {
                let value = self.project.clone().and_then(|p| {
                    p.read(&mut self.cad.session, |p, _| {
                        // Every type at least its drawn positions.
                        ecad_electrical::terminal_strips::offer_drawn_value(p.ledger.document(), c)
                    })
                    .ok()
                });
                if let Some(v) = value {
                    let _ = self.run_line(&format!("setprop {}", json!({"component": c.hex(), "key": "terminal_blocks", "value": v})));
                }
            }
            A::RenumberStrip(c) => {
                let _ = self.run_line(&format!("renumber {}", json!({"kind": "terminal_blocks", "scope": "selection", "strips": [c.hex()]})));
            }
            A::SetCores(k) => self.focus_cable_cores(k),
            A::NextFreeCore(w) => {
                let label = self.project.clone().and_then(|p| {
                    p.read(&mut self.cad.session, |p, _| {
                        let doc = p.ledger.document();
                        doc.cables_of(w).first().and_then(|k| ecad_electrical::terminal_strips::cores::next_free_label(doc, *k))
                    })
                    .ok()
                    .flatten()
                });
                if let Some(l) = label {
                    let _ = self.run_line(&format!("cablecore {}", json!({"conductor": w.hex(), "core": l})));
                }
            }
            // The page row's wheel.
            A::PageWheel(page, at) => self.open_page_wheel(page, at),
            // The lone pins' mates, through Place ▸ Connector's prompts (connector,
            // halves, draw), as a person answers them.
            A::PlaceMates(c, side) => {
                let tag = self.project.clone().and_then(|p| {
                    p.read(&mut self.cad.session, |p, _| ecad_electrical::devices::tags::tag_of(p.ledger.document(), c).map(str::to_string))
                        .ok()
                        .flatten()
                });
                if let Some(tag) = tag {
                    let half = if side == ecad_electrical::symbols::Side::Socket { "Socket" } else { "Plug" };
                    for line in ["connector", tag.as_str(), half, "Pins"] {
                        if self.run_line(line).is_some() {
                            break;
                        }
                    }
                }
            }
            A::AccessoryFix(fix, i) => {
                let cmd: Option<String> = self.project.clone().and_then(|p| {
                    p.configure(&mut self.cad.session, |p| {
                        let o = p.reports();
                        ecad_electrical::reports::accessory_fix_commands(o, fix).into_iter().nth(i).map(|(_, c, v)| format!("{c} {v}"))
                    })
                    .ok()
                    .flatten()
                });
                if let Some(line) = cmd {
                    let _ = self.run_line(&line);
                }
            }
            A::OpenStandards(word) => self.open_standards_at(word),
            A::RatingFix(f) => self.rating_fix(f),
            A::CoreFix(f, k) => self.core_fix(f, k),
            A::CableFix(f, k) => self.cable_fix(f, k),
            A::EditTitleBlock(page, item) => self.open_title_blocks(Some((page, item))),
            A::ClearPageSheet(page) => {
                let _ = self.run_line(&format!("pageprops {}", serde_json::json!({"page": page, "sheet": null})));
            }
            A::ShowCables => {
                self.reports_view();
                if let Some(r) = &mut self.reports {
                    r.tab = crate::reports::panel::Tab::Cables;
                    r.cores_of = None;
                    r.only_flagged = true;
                    r.invalidate();
                }
            }
            A::Reports => self.reports_view(),
            A::ProjectDevices => {
                self.shell.lists = shell::ListsView::Devices;
                self.set_workspace(shell::Workspace::Lists);
            }
            A::PrintBook(folder) => {
                self.open_print_book(folder);
                self.publish_view(shell::PublishTab::PrintBook);
            }
            A::Release => {
                self.open_release(None);
                self.publish_view(shell::PublishTab::Release);
            }
            A::NewGenerated(kind) => {
                let mut v = json!({"generated": ecad_model::Word::word(kind)});
                if let Some(f) = self.panel.target_folder() {
                    v["folder"] = json!(f.hex());
                }
                let _ = self.run_line(&format!("newpage {v}"));
            }
            // New page ▸ Terminal strips.
            A::NewStripPage => {
                let mut v = json!({"generated": "terminal_blocks", "style": "graphic"});
                if let Some(f) = self.panel.target_folder() {
                    v["folder"] = json!(f.hex());
                }
                let _ = self.run_line(&format!("newpage {v}"));
            }
            A::MoveInto { page, folder, index } => {
                let mut v = json!({"page": page.hex(), "folder": folder.map(|f| f.hex())});
                if let Some(i) = index {
                    v["index"] = json!(i);
                }
                let _ = self.run_line(&format!("movepage {v}"));
            }
            A::MoveFolder { folder, parent } => {
                let line = format!("movefolder {}", json!({"folder": folder.hex(), "parent": parent.map(|f| f.hex())}));
                if let Some(e) = self.run_line(&line) {
                    self.panel.note = Some(e);
                }
            }
            A::NewFolder(parent) => {
                let r = self.run_value("newfolder", json!({"name": "New folder", "parent": parent.map(|f| f.hex())}));
                match r {
                    Ok(v) => {
                        self.refresh_panel();
                        if let Some(f) = ecad_engine::commands::reply::id::<ecad_model::FolderId>(&v, "folder") {
                            self.panel.folded.remove(&f);
                            self.panel.start_folder_rename(f);
                        }
                    }
                    Err(e) => self.panel.note = Some(e),
                }
            }
            A::RenameFolder(folder, name) => {
                let line = format!("renamefolder {}", json!({"folder": folder.hex(), "name": name}));
                match self.run_line(&line) {
                    None => self.panel.folder_rename = None,
                    Some(e) => {
                        if let Some(r) = &mut self.panel.folder_rename {
                            r.text = name;
                            r.error = Some(e);
                        }
                    }
                }
            }
            A::DeleteFolder(folder) => {
                let _ = self.run_line(&format!("deletefolder {}", json!({"folder": folder.hex()})));
            }
            // A group row's + opens the + Page picker, adding last in
            // that group.
            A::NewPageIn(Some(f)) => {
                let (at, above) = self.picker_anchor();
                self.open_new_page_picker(at, above, None, Some(f));
            }
            A::NewPageIn(None) => {
                self.open_new_page_box();
                self.new_page_folder = None;
            }
            A::SetFolderStart(folder, start) => {
                let line = format!("folderprops {}", json!({"folder": folder.hex(), "start": start}));
                match self.run_line(&line) {
                    None => self.panel.start_edit = None,
                    Some(e) => {
                        if let Some(r) = &mut self.panel.start_edit {
                            r.error = Some(e);
                        }
                    }
                }
            }
            A::RenumberPages(folder) => {
                let v = match folder {
                    Some(f) => json!({"folder": f.hex()}),
                    None => json!({}),
                };
                if let Some(e) = self.run_line(&format!("renumberpages {v}")) {
                    self.panel.note = Some(e);
                }
            }
            A::RunLine(line) => {
                if let Some(e) = self.run_line(&line) {
                    self.panel.note = Some(e);
                }
            }
            A::DeletePages(pages) => {
                let v = json!({"pages": pages.iter().map(|p| p.hex()).collect::<Vec<_>>()});
                if let Some(e) = self.run_line(&format!("deletepage {v}")) {
                    self.panel.note = Some(e);
                }
            }
            A::SetNumber(page, number) => {
                let line = format!("pageprops {}", json!({"page": page.hex(), "number": number}));
                match self.run_line(&line) {
                    None => self.panel.number_edit = None,
                    Some(e) => {
                        if let Some(r) = &mut self.panel.number_edit {
                            r.text = number;
                            r.error = Some(e);
                        }
                    }
                }
            }
            A::ProjectInfo => self.open_project_popout(crate::settings::sheets::Popout::Info),
            A::ProjectSettings => self.open_project_popout(crate::settings::sheets::Popout::Settings),
            A::Revisions => self.open_revisions(),
            A::BackToCurrent => {
                let _ = self.run_line("closerevision {}");
                self.preview = None;
            }
            A::RevisionPage(page) => self.preview_revision_page(page),
            A::History(d) => self.history_action(d),
            A::SetDeviceProperty { component, key, value, conflict } => {
                let value = json!(value);
                let line = format!("setprop {}", json!({"component": component.hex(), "key": key, "value": value, "conflict": conflict}));
                let value = value.as_str().unwrap_or_default().to_string();
                if let Some(e) = self.run_line(&line) {
                    self.panel.devices.refused(component, key, &value, e);
                } else {
                    self.panel.devices.error = None;
                }
            }
        }
        self.refresh_panel();
    }

    /// Show branch `branch` (named `name`) as the Branch list does: straight away with nothing
    /// unsaved, else through Save and switch (or Save As on a read-only file).
    pub fn ask_switch(&mut self, branch: &str, name: &str) {
        let (unsaved, read_only) = match self.status() {
            Some(s) => (s.unsaved, s.file.read_only.is_some()),
            None => return,
        };
        if unsaved == 0 {
            self.history_action(crate::history::panel::Do::Run("switchbranch", json!({"branch": branch})));
            return;
        }
        self.panel.tab = crate::pages::project_panel::Tab::History;
        self.panel.history.dialog = Some(if read_only {
            crate::history::panel::Dialog::SwitchReadOnly
        } else {
            crate::history::panel::Dialog::Switch { branch: branch.to_string(), name: name.to_string() }
        });
    }

    /// Act on what the History tab asked for: each write is its command's JSON form.
    pub fn history_action(&mut self, d: crate::history::panel::Do) {
        use crate::history::panel::Do;
        match d {
            Do::Agent(a) => self.agent_action(a),
            Do::Run(cmd, params) => {
                match self.run_click(cmd, params) {
                    Ok(_) => {
                        self.panel.history.done();
                        // The same refresh as for a typed command.
                        self.sync_branch();
                    }
                    Err(e) => self.panel.history.refused(e),
                }
            }
            Do::AskRestore(params) => {
                let mut ask = params.clone();
                ask["preview"] = json!(true);
                match self.run_click("restore", ask) {
                    Ok(v) => {
                        let question = v["question"].as_str().unwrap_or_default().to_string();
                        self.panel.history.dialog = Some(crate::history::panel::Dialog::Restore { params, question });
                    }
                    Err(e) => self.panel.history.refused(e),
                }
            }
            Do::SaveAs => self.request_save_as(),
            Do::ShowPoint(params) => match self.run_click("showpoint", params) {
                Ok(_) => self.preview = None,
                Err(e) => self.panel.history.refused(e),
            },
            Do::AskMerge(target) => self.ask_merge(target),
        }
    }

    /// Open the merge dialog for `target` (`{from, into?}`) with its preview:
    /// nothing is written until Merge; a merge into another branch than the one shown says it
    /// switches there first.
    pub fn ask_merge(&mut self, target: serde_json::Value) {
        let mut ask = target.clone();
        ask["preview"] = json!(true);
        match self.run_click("merge", ask) {
            Ok(v) => {
                let shown = self.branch_info().map(|b| b.name).unwrap_or_else(|| "main".into());
                let into = v["into"].as_str().unwrap_or("main").to_string();
                let note = (v["update"] != json!(true) && into != shown)
                    .then(|| format!("This switches to {into} (saving \"{shown}\" first) and merges into it."));
                self.merge = Some(crate::history::merge_dialog::MergeDialog::new(target, v, note));
            }
            Err(e) => self.panel.history.refused(e),
        }
    }

    /// Open the Renumber dialog, or Finalise numbering (`finalise`), with
    /// Wires chosen (`wires`) or a folder preset; its preview is computed now (reads only).
    pub fn open_renumber(&mut self, finalise: bool, wires: bool, folder: Option<ecad_model::FolderId>) {
        let Some(p) = self.project.clone() else { return };
        let read_only = self.header().read_only;
        let (uid, handles) = match self.cad.session.state() {
            Ok(st) => (Some(st.uid), st.selection.clone()),
            Err(_) => (None, Vec::new()),
        };
        let read = p.read(&mut self.cad.session, |p, _| {
            let doc = p.ledger.document();
            let objects =
                uid.and_then(|u| p.page_of(u)).map(|pg| ecad_engine::properties::props::selected_objects(p, pg, &handles)).unwrap_or_default();
            (
                ecad_electrical::numbering::commands::choices(doc),
                objects,
                ecad_electrical::devices::padding(doc),
                u8::try_from(doc.settings().wiring.digits()).unwrap_or(ecad_model::connectivity::wiring::MAX_DIGITS),
            )
        });
        let Ok((choices, objects, digits, wire_digits)) = read else { return };
        let mut d = crate::devices::renumber_dialog::RenumberDialog::new(&choices, objects, digits, read_only, finalise);
        if wires {
            d = d.for_wires(wire_digits);
        }
        if let Some(f) = folder {
            d = d.for_folder(f);
        }
        if let Ok((c, v)) = d.params(true) {
            match self.run_click(c, v) {
                Ok(v) => d.preview = ecad_electrical::numbering::commands::Preview::from_reply(&v).unwrap_or_default(),
                Err(e) => d.error = Some(e),
            }
        }
        self.renumber = Some(d);
    }

    /// The provisional question before an issue, for the refused `line`.
    pub(crate) fn ask_issue_provisional(&mut self, line: String) {
        let Some(p) = self.project.clone() else { return };
        let items = p.read(&mut self.cad.session, |p, _| ecad_electrical::numbering::commands::provisional_items(p.ledger.document()));
        if let Ok(items) = items {
            self.issue_prompt = Some((line, format!("{}:\n{}", items.words(), items.list_line())));
        }
    }

    /// Make `page` the active page, as a click on its row does: a command waiting at a prompt is
    /// cancelled first (nothing written), then the page is shown in the one page window. A
    /// preview of that page closes.
    ///
    /// # Errors
    /// No project, or the page cannot be shown (echoed at the command line).
    pub fn show_page(&mut self, page: PageId) -> Result<(), String> {
        let p = self.project.clone().ok_or("no ECAD project is open")?;
        if self.cad.session.running.is_some() {
            self.cad.session.cancel();
        }
        let r = p.open_page(&mut self.cad.session, page).map(|_| ());
        if let Err(e) = &r {
            self.cad.session.echo(e.clone());
        }
        self.panel.reveal(page);
        self.after_logic();
        r
    }

    /// The Devices tab's go to: `symbol`'s page becomes the active page (the path of `openpage` and
    /// Go to Reference), the view is centred on it at the current zoom and it is selected.
    ///
    /// # Errors
    /// No project, an unknown symbol, or a page that cannot be shown (echoed).
    pub fn goto_symbol(&mut self, symbol: ecad_model::RepresentationId) -> Result<(), String> {
        self.goto_symbols(&[symbol])
    }

    /// The Devices tab's go to for a device: the first of `symbols` is gone to as
    /// [`App::goto_symbol`] does, and every one of `symbols` on its page is selected (a device's
    /// symbols on other pages are not). A selection change only: nothing is recorded.
    ///
    /// # Errors
    /// No symbols, no project, an unknown first symbol, or a page that cannot be shown (echoed).
    pub fn goto_symbols(&mut self, symbols: &[ecad_model::RepresentationId]) -> Result<(), String> {
        let symbol = *symbols.first().ok_or("that device has no symbols")?;
        let p = self.project.clone().ok_or("no ECAD project is open")?;
        let page = p
            .read(&mut self.cad.session, |p, _| ecad_engine::reports::device_list::symbol_target(p, symbol).map(|t| t.0))?
            .ok_or("that symbol no longer exists")?;
        self.show_page(page)?;
        let (_, at, handles) = p
            .read(&mut self.cad.session, |p, _| {
                let (pg, at, mut handles) = ecad_engine::reports::device_list::symbol_target(p, symbol)?;
                for rep in symbols.iter().skip(1) {
                    if let Some((other, _, more)) = ecad_engine::reports::device_list::symbol_target(p, *rep)
                        && other == pg
                    {
                        handles.extend(more);
                    }
                }
                Some((pg, at, handles))
            })?
            .ok_or("that symbol no longer exists")?;
        if let (Some(at), Ok(st)) = (at, self.cad.session.state_mut()) {
            // The current zoom, as Go to Reference keeps it, within a readable range (a page shown
            // only while empty may have a tiny or a huge view).
            let height = st.view().height.clamp(GOTO_MIN_HEIGHT, GOTO_MAX_HEIGHT);
            let _ = st.set_view(ecad_engine::cad::View { center: ecad_engine::cad::Vec2::new(at.x, at.y), height, twist: 0.0 });
        }
        self.cad.session.set_selection(handles);
        Ok(())
    }

    /// The Checks tab's go to: the row's page becomes the active page, the view is
    /// centred on its point at the current zoom (kept between 60 and 600 mm high), its object is
    /// selected, and a ring in the row's severity colour marks the point until the next change, Esc
    /// or a click on empty space. Nothing is recorded.
    ///
    /// # Errors
    /// No project, a row with no page, or a page that cannot be shown (echoed).
    pub fn goto_check(&mut self, check: &ecad_electrical::checks::Check) -> Result<(), String> {
        let p = self.project.clone().ok_or("no ECAD project is open")?;
        let page = check.page.ok_or("that row is not on a page")?;
        self.show_page(page)?;
        let handles = p.configure(&mut self.cad.session, |p| {
            p.goto_ring = check.at.map(|at| (page, at, check.severity()));
            ecad_engine::checks::handles_of(p, page, check.object)
        })?;
        if let (Some(at), Ok(st)) = (check.at, self.cad.session.state_mut()) {
            let height = st.view().height.clamp(GOTO_MIN_HEIGHT, GOTO_MAX_HEIGHT);
            let _ = st.set_view(ecad_engine::cad::View { center: ecad_engine::cad::Vec2::new(at.x, at.y), height, twist: 0.0 });
        }
        self.cad.session.set_selection(handles);
        Ok(())
    }

    /// The go-to ring clears on Esc and on a click on empty space, i.e. a click on the drawing
    /// area with no object under the pointer; the next change or go-to clears
    /// it too. The project is touched only on such a frame.
    pub(crate) fn clear_goto_ring(&mut self, (esc, click): (bool, bool)) {
        if !(esc || click) {
            return;
        }
        if let Some(p) = self.project.clone()
            && p.is_attached(&self.cad.session)
        {
            let _ = p.configure(&mut self.cad.session, |p| p.goto_ring = None);
        }
    }

    /// Open the page name box for a new page, prefilled with its number (the panel's **+** and
    /// Tool Sets' New Page).
    /// Where the + Page picker opens when a panel + asked for it: at the pointer, worked out when
    /// it is first drawn.
    fn picker_anchor(&self) -> (Option<egui::Pos2>, bool) {
        (None, false)
    }

    /// Run a command's JSON form as the command path does, returning its result.
    /// A History or merge click's command: the command echoes its own
    /// line, so only a refusal is echoed here (`CadApp::run` would echo the reply's `message` a
    /// second time).
    pub(crate) fn run_click(&mut self, cmd: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
        let r = self.cad.session.execute(cmd, &params).map_err(|e| e.to_string());
        if let Err(e) = &r {
            self.cad.session.echo(e.clone());
        }
        self.after_logic();
        r
    }

    pub(crate) fn run_value(&mut self, cmd: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
        let r = self.cad.run(cmd, params).map_err(|e| e.to_string());
        self.after_logic();
        r
    }

    /// Checks' **Set cores…**: the cable's slash is shown and selected, so Properties
    /// shows its Cores field.
    pub fn focus_cable_cores(&mut self, cable: ecad_model::CableId) {
        let Some(p) = self.project.clone() else { return };
        let found = p
            .read(&mut self.cad.session, |p, _| {
                let doc = p.ledger.document();
                let (_, r) = doc.representations_of(ecad_model::Subject::Cable(cable)).find(|(_, r)| r.role == ecad_model::Role::Slash)?;
                let at = r.items.first().and_then(|i| doc.items().get(i)).and_then(|i| match &i.kind {
                    ecad_model::ItemKind::Primitive(ecad_model::Primitive::Line { a, b }) => {
                        Some(ecad_model::Pt::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0))
                    }
                    _ => None,
                });
                Some((r.page, r.items.clone(), at))
            })
            .ok()
            .flatten();
        let Some((page, items, at)) = found else {
            self.notice = Some("That cable has no slash drawn; set its cores in the Reports' Cables tab or with setprop.".into());
            return;
        };
        if self.show_page(page).is_err() {
            return;
        }
        let handles = p
            .read(&mut self.cad.session, |p, _| {
                p.pages.get(&page).map(|v| items.iter().filter_map(|i| v.map.item(*i)).collect::<Vec<_>>()).unwrap_or_default()
            })
            .unwrap_or_default();
        if let (Some(at), Ok(st)) = (at, self.cad.session.state_mut()) {
            let height = st.view().height.clamp(GOTO_MIN_HEIGHT, GOTO_MAX_HEIGHT);
            let _ = st.set_view(ecad_engine::cad::View { center: ecad_engine::cad::Vec2::new(at.x, at.y), height, twist: 0.0 });
        }
        self.cad.session.set_selection(handles);
    }

    /// Show `page`, centred on `at` at the current zoom (kept readable).
    pub fn goto_place(&mut self, page: PageId, at: ecad_model::Pt) {
        if self.show_page(page).is_ok()
            && let Ok(st) = self.cad.session.state_mut()
        {
            let height = st.view().height.clamp(GOTO_MIN_HEIGHT, GOTO_MAX_HEIGHT);
            let _ = st.set_view(ecad_engine::cad::View { center: ecad_engine::cad::Vec2::new(at.x, at.y), height, twist: 0.0 });
        }
    }
}

/// `line` (`name {json}`) with `key` set to `value` in its JSON (the issue's answer).
pub(crate) fn with_param(line: &str, key: &str, value: serde_json::Value) -> String {
    let (name, rest) = line.split_once(' ').unwrap_or((line, "{}"));
    let mut v: serde_json::Value = serde_json::from_str(rest).unwrap_or_else(|_| json!({}));
    if let Some(o) = v.as_object_mut() {
        o.insert(key.to_string(), value);
    }
    format!("{name} {v}")
}
