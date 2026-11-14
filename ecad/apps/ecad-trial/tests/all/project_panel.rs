//! Brief 016 acceptance in the trial (design 016 §10, checks A–J), headless with egui_kittest in a
//! 1280×800 window: the Project panel by its labels, the one page window, undo bringing its page
//! to the front, the read-only preview, and the reference screenshots of design §10, written to
//! `$ECAD_TESTER_SHOTS` when it is set and a wgpu adapter exists (`016-panel.png`,
//! `016-rename.png`, `016-drag.png`, `016-collapsed.png`, `016-preview-ref.png`, `016-large.png`).
//!
//! Fixture P (design §10): 010's fixture R (pages `Supply` and `Field`, cable C1 with linked
//! references `2/F1` and `1/F2`) plus a third page `Spare`.

use std::sync::{Arc, Mutex};

use ecad_model::{Op, PageId};
use ecad_trial::project_panel::{self, ROW_H};
use ecad_trial::shell::LeftTab;
use ecad_trial::{Trial, tools};
use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

pub(crate) type Deltas = Arc<Mutex<Vec<(egui::TextureId, Option<egui::epaint::ImageDelta>)>>>;

pub(crate) struct Collect(pub(crate) Deltas);

impl egui_kittest::TestRenderer for Collect {
    fn handle_delta(&mut self, delta: &mut egui::TexturesDelta) {
        let mut out = self.0.lock().unwrap();
        for (id, imgs) in delta.set.drain() {
            out.extend(imgs.into_iter().map(|i| (id, Some(i))));
        }
        out.extend(delta.free.drain().map(|id| (id, None)));
    }
}

fn block_on<F: std::future::Future>(f: F) -> F::Output {
    use std::task::{Context, Poll, Wake, Waker};
    struct T(std::thread::Thread);
    impl Wake for T {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(T(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    let mut f = std::pin::pin!(f);
    loop {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
        std::thread::park_timeout(std::time::Duration::from_millis(5));
    }
}

/// Render the harness's last frame with egui-wgpu and save it as PNG; `None` without an adapter.
pub(crate) fn shot(h: &Harness<'static, Trial>, deltas: &Deltas, path: &std::path::Path) -> Option<egui::ColorImage> {
    use eframe::egui_wgpu::{self, wgpu};
    let setup = egui_wgpu::WgpuSetup::without_display_handle();
    let instance = block_on(setup.new_instance());
    let config = egui_wgpu::WgpuConfiguration { wgpu_setup: setup, ..Default::default() };
    let rs = block_on(egui_wgpu::RenderState::create(&config, &instance, None, egui_wgpu::RendererOptions::PREDICTABLE)).ok()?;
    let ctx = &h.ctx;
    let ppp = ctx.pixels_per_point();
    {
        let mut r = rs.renderer.write();
        for (id, img) in deltas.lock().unwrap().iter() {
            match img {
                Some(img) => r.update_texture(&rs.device, &rs.queue, *id, img),
                None => r.free_texture(id),
            }
        }
    }
    let size = ctx.content_rect().size() * ppp;
    let screen = egui_wgpu::ScreenDescriptor { pixels_per_point: ppp, size_in_pixels: [size.x.round() as u32, size.y.round() as u32] };
    let tess = ctx.tessellate(h.output().shapes.clone(), ppp);
    let mut encoder = rs.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    let texture = rs.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: screen.size_in_pixels[0], height: screen.size_in_pixels[1], depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: rs.target_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    {
        let mut r = rs.renderer.write();
        let user = r.update_buffers(&rs.device, &rs.queue, &mut encoder, &tess, &screen);
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                        depth_slice: None,
                    })],
                    ..Default::default()
                })
                .forget_lifetime();
            r.render(&mut pass, &tess, &screen);
        }
        rs.queue.submit(user.into_iter().chain(std::iter::once(encoder.finish())));
    }
    let (w, hgt) = (texture.width() as usize, texture.height() as usize);
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize;
    let padded = (w * 4).div_ceil(align) * align;
    let buffer = rs.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (padded * hgt) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut enc = rs.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    enc.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded as u32), rows_per_image: None },
        },
        wgpu::Extent3d { width: w as u32, height: hgt as u32, depth_or_array_layers: 1 },
    );
    let idx = rs.queue.submit(std::iter::once(enc.finish()));
    let (tx, rx) = std::sync::mpsc::channel();
    buffer.slice(..).map_async(wgpu::MapMode::Read, move |v| drop(tx.send(v)));
    rs.device.poll(wgpu::PollType::Wait { submission_index: Some(idx), timeout: Some(std::time::Duration::from_secs(20)) }).ok()?;
    rx.recv().ok()?.ok()?;
    let data = buffer.slice(..).get_mapped_range().ok()?.to_vec();
    let bgra = matches!(rs.target_format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
    let mut pixels = Vec::with_capacity(w * hgt);
    for row in data.chunks_exact(padded) {
        for &[c0, c1, c2, _] in row[..w * 4].as_chunks::<4>().0 {
            let (r, g, b) = if bgra { (c2, c1, c0) } else { (c0, c1, c2) };
            pixels.push(egui::Color32::from_rgb(r, g, b));
        }
    }
    let image = egui::ColorImage { size: [w, hgt], pixels, source_size: egui::vec2(w as f32, hgt as f32) };
    let r = cadcraft_ui_egui::control::save_screenshot(&image, Some(&path.display().to_string()), 0);
    assert_eq!(r["ok"], true, "{r}");
    Some(image)
}

pub(crate) fn build(t: Trial, deltas: Option<Deltas>) -> Harness<'static, Trial> {
    // 60 frames a second, so two clicks a few frames apart are a double-click.
    let mut b = Harness::builder().with_size(egui::vec2(1280.0, 800.0)).with_step_dt(1.0 / 60.0);
    if let Some(d) = deltas {
        b = b.renderer(Collect(d));
    }
    let mut h = b.build_ui_state(
        |ui, t: &mut Trial| {
            t.logic(&ui.ctx().clone());
            t.ui(ui);
        },
        t,
    );
    h.run_steps(5);
    h
}

pub(crate) fn trial() -> Trial {
    let mut t = rev3_trial();
    open_panels(&mut t);
    t
}

/// The trial as it starts (UI rev 3, board 1.01: both sides closed).
pub(crate) fn rev3_trial() -> Trial {
    ecad_cadcraft::install();
    ecad_cadcraft::views::isolate_for_tests();
    let mut t = Trial::new(None).unwrap();
    t.hover_delay = 0.0;
    t
}

/// Open the Book panel and the inspector, as a person does from the strip and the rail: the
/// tests of the panels' contents start with them shown (UI rev 3 starts with both closed).
pub(crate) fn open_panels(t: &mut Trial) {
    t.shell.state.left.collapsed = false;
    t.shell.state.right_collapsed = false;
}

pub(crate) fn ok(h: &mut Harness<'static, Trial>, line: &str) {
    let e = h.state_mut().submit_command(line);
    assert!(e.is_none(), "{line}: {e:?}");
    h.run_steps(2);
}

pub(crate) fn doc(h: &mut Harness<'static, Trial>) -> ecad_model::Document {
    let t = h.state_mut();
    let p = t.project().unwrap().clone();
    p.read(&mut t.cad.session, |p, _| p.ledger.document().clone()).unwrap()
}

pub(crate) fn head(h: &mut Harness<'static, Trial>) -> u64 {
    let t = h.state_mut();
    let p = t.project().unwrap().clone();
    p.read(&mut t.cad.session, |p, _| p.ledger.head()).unwrap()
}

pub(crate) fn last_ops(h: &mut Harness<'static, Trial>) -> (String, Vec<Op>) {
    let t = h.state_mut();
    let p = t.project().unwrap().clone();
    p.read(&mut t.cad.session, |p, _| p.ledger.log().last().map(|x| (x.label.clone(), x.ops.clone())).unwrap()).unwrap()
}

pub(crate) fn active(h: &mut Harness<'static, Trial>) -> Option<PageId> {
    let t = h.state_mut();
    let p = t.project().unwrap().clone();
    p.read(&mut t.cad.session, |p, _| p.active_page()).unwrap()
}

fn titles(h: &mut Harness<'static, Trial>) -> Vec<String> {
    let d = doc(h);
    d.page_order.iter().map(|p| d.pages[p].title.clone()).collect()
}

/// The one page window's title.
fn tab_title(h: &Harness<'static, Trial>) -> String {
    let t = h.state();
    let owned: Vec<_> = t.cad.session.docs.iter().filter(|d| t.cad.session.ext.owner_of(d.uid).is_some()).collect();
    assert!(owned.len() <= 1, "one page window at most: {}", owned.len());
    owned.first().map(|d| d.title.clone()).unwrap_or_default()
}

pub(crate) fn click(h: &mut Harness<'static, Trial>, label: &str) {
    // UI rev 3: the panels' tabs are the strip's panel buttons and the inspector's Checks tab.
    let tab = match label {
        project_panel::PAGES_TAB => Some(LeftTab::Pages),
        project_panel::DEVICES_TAB => Some(LeftTab::Devices),
        ecad_trial::history_panel::HISTORY_TAB => Some(LeftTab::History),
        _ => None,
    };
    if let Some(tab) = tab {
        let s = h.state().shell_state();
        if s.workspace != ecad_trial::shell::Workspace::Draw {
            // Only Draw has the strip; elsewhere the tab is kept for when Draw is shown again.
            h.state_mut().set_left_tab(tab);
        } else if s.left.collapsed || s.left.tab != tab {
            h.get_by_label(&ecad_trial::shell::tool_strip::panel_label(tab)).click();
        }
    } else if label == ecad_trial::checks_panel::CHECKS_TAB {
        // The rail's Checks button opens it (pressed while open on Checks it would close).
        let s = h.state().shell_state();
        if s.right_collapsed || s.right_tab != ecad_trial::shell::RightTab::Checks {
            h.get_by_label(&ecad_trial::shell::right::rail_label(ecad_trial::shell::RightTab::Checks)).click();
        }
    } else {
        h.get_by_label(label).click();
    }
    h.run_steps(3);
}

/// Brief 038: an entry of the top bar's project menu (the old panel header's buttons).
pub(crate) fn project_menu(h: &mut Harness<'static, Trial>, label: &str) {
    click(h, ecad_trial::shell::top_bar::PROJECT_MENU);
    click(h, label);
}

pub(crate) fn key(h: &mut Harness<'static, Trial>, key: Key, modifiers: Modifiers) {
    h.event(Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers });
    h.event(Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers });
    h.run_steps(2);
}

pub(crate) fn pointer(h: &mut Harness<'static, Trial>, at: Pos2, pressed: Option<bool>) {
    h.event(Event::PointerMoved(at));
    if let Some(pressed) = pressed {
        h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    }
    h.run_steps(1);
}

/// The screen rectangle of the panel row labelled `label`.
pub(crate) fn row_rect(h: &Harness<'static, Trial>, label: &str) -> egui::Rect {
    h.get_by_label(label).rect()
}

/// Fixture P: Supply (C1 leaving at (65,5), `2/F1`), Field (C1 arriving at (20,5), `1/F2`), Spare.
pub(crate) fn fixture_p(h: &mut Harness<'static, Trial>) -> (PageId, PageId, PageId) {
    h.state_mut().cad.session.viewport_px = (1280.0, 800.0);
    let supply = doc(h).page_order[0];
    ok(h, &format!(r#"renamepage {{"page":"{}","title":"Supply"}}"#, supply.hex()));
    for y in [0, 5, 10] {
        ok(h, &format!(r#"wire {{"points":[[0,{y}],[60,{y}]]}}"#));
    }
    ok(h, r#"cableslash {"from":[28,-3],"to":[32,18],"tag":"C1"}"#);
    let cable = doc(h).cables.keys().next().copied().unwrap();
    ok(h, &format!(r#"cableref {{"cable":"{}","at":[65,5]}}"#, cable.hex()));
    ok(h, r#"newpage {"title":"Field"}"#);
    ok(h, &format!(r#"cablecontinue {{"cable":"{}","at":[20,5],"toward":[40,5]}}"#, cable.hex()));
    ok(h, r#"newpage {"title":"Spare"}"#);
    let d = doc(h);
    (d.page_order[0], d.page_order[1], d.page_order[2])
}

pub(crate) fn shots() -> Option<std::path::PathBuf> {
    std::env::var_os("ECAD_TESTER_SHOTS").map(std::path::PathBuf::from)
}

pub(crate) fn take(h: &Harness<'static, Trial>, deltas: &Deltas, name: &str) -> Option<egui::ColorImage> {
    shots().and_then(|dir| shot(h, deltas, &dir.join(name)))
}

/// Checks A, B, C, E, G, J and the screenshots `016-panel`, `016-rename`, `016-drag`,
/// `016-collapsed`.
#[test]
fn the_project_panel_lists_switches_edits_and_follows_the_project() {
    let deltas: Deltas = Arc::default();
    let mut h = build(trial(), Some(deltas.clone()));
    let (supply, field, spare) = fixture_p(&mut h);
    h.run_steps(3);

    // A: every page listed in order; a click makes it active in the one page window.
    for label in ["Page 1: Supply", "Page 2: Field", "Page 3: Spare"] {
        assert!(h.query_by_label(label).is_some(), "{label} is listed");
    }
    assert_eq!(active(&mut h), Some(spare), "newpage made Spare active");
    for (label, page, title) in [
        ("Page 1: Supply", supply, "Untitled - page 1: Supply"),
        ("Page 2: Field", field, "Untitled - page 2: Field"),
        ("Page 3: Spare", spare, "Untitled - page 3: Spare"),
    ] {
        click(&mut h, label);
        assert_eq!(active(&mut h), Some(page), "{label}");
        assert_eq!(tab_title(&h), title);
    }
    assert_eq!(h.state().cad.session.docs.len(), 1, "exactly one ECAD file tab after clicking every row");
    click(&mut h, "Page 1: Supply");
    let shown = h.state().cad.session.doc().unwrap().model.iter().filter(|e| e.common.layer != ecad_cadcraft::frame::LAYER).count();
    assert!(shown > 3, "the page window shows Supply's items ({shown})");

    // J: after a row click, Down, Down, Enter makes row 3 active; typing then reaches the
    // command line.
    key(&mut h, Key::ArrowDown, Modifiers::NONE);
    key(&mut h, Key::ArrowDown, Modifiers::NONE);
    assert_eq!(active(&mut h), Some(supply), "the arrows move the cursor only");
    key(&mut h, Key::Enter, Modifiers::NONE);
    assert_eq!(active(&mut h), Some(spare), "Enter makes the cursor row active");
    h.event(Event::Text("l".into()));
    h.run_steps(2);
    assert!(!h.state().panel.armed, "another key disarms the panel");
    // Brief 038: typing while no command runs is the shell's (US1 `idle_typing` off), so the
    // letter starts nothing at CADCraft's hidden command line.
    assert_eq!(h.state().cad.cmd.buffer, "", "and does not reach the command line");
    h.state_mut().cad.cmd.buffer.clear();

    // Screenshot 016-panel: Field active, cursor on Spare.
    click(&mut h, "Page 2: Field");
    key(&mut h, Key::ArrowDown, Modifiers::NONE);
    let panel = take(&h, &deltas, "016-panel.png");

    // B: New page (the panel's +, as Tool Sets' New Page) opens the name box prefilled with the
    // next number; Cancel writes nothing; Enter records exactly `newpage`'s transaction and shows
    // the page at once.
    let before = head(&mut h);
    click(&mut h, project_panel::NEW_PAGE);
    assert_eq!(h.state().name_box.as_ref().map(|b| b.text.as_str()), Some("4"), "prefilled");
    click(&mut h, "Cancel");
    assert!(h.state().name_box.is_none() && head(&mut h) == before, "Cancel does nothing");
    h.get_by_label("Page 1: Supply").click_secondary();
    h.run_steps(3);
    {
        // Brief 041: the slot inside the wheel (the Pages tab has a New page button too).
        let c = h.state().shell.tools.wheel.as_ref().map(|w| w.widget.center).expect("the row's wheel");
        h.get_all_by_label("New page").find(|n| (n.rect().center() - c).length() <= ecad_ui::tokens::wheel::OUTER_RADIUS).expect("its slot").click();
    }
    h.run_steps(3);
    assert_eq!(h.state().name_box.as_ref().map(|b| b.text.as_str()), Some("4"), "the row wheel's New page too");
    key(&mut h, Key::Enter, Modifiers::NONE);
    assert!(h.state().name_box.is_none());
    assert_eq!(head(&mut h), before + 1, "one transaction");
    let (label, ops) = last_ops(&mut h);
    assert_eq!(label, "New page");
    assert!(matches!(ops.as_slice(), [Op::AddPage { page, index: 3, .. }] if page.title == "4"), "{ops:?}");
    let new = active(&mut h).unwrap();
    assert_eq!(new, doc(&mut h).page_order[3], "the new page is shown at once");
    assert!(h.state().cad.cmd.buffer.is_empty() && h.state().cad.session.running.is_none(), "Enter stayed in the box");
    // `u` removes it (the panel follows), redo brings it back.
    ok(&mut h, "u");
    h.run_steps(2);
    assert!(h.query_by_label("Page 4: 4").is_none(), "the panel follows undo");
    ok(&mut h, "redo");
    h.run_steps(2);
    assert!(h.query_by_label("Page 4: 4").is_some(), "and redo");
    assert_eq!(active(&mut h), Some(new));

    // B: rename by double-click, type, Enter: exactly `renamepage`'s ops.
    let before = head(&mut h);
    // A double-click.
    let at = row_rect(&h, "Page 3: Spare").center();
    for _ in 0..2 {
        pointer(&mut h, at, Some(true));
        pointer(&mut h, at, Some(false));
    }
    h.run_steps(3);
    assert!(h.state().panel.rename.is_some(), "double-click opens rename");
    let rename = take(&h, &deltas, "016-rename.png");
    h.event(Event::Key { key: Key::A, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND });
    h.event(Event::Text("Spare 2".into()));
    h.run_steps(1);
    key(&mut h, Key::Enter, Modifiers::NONE);
    assert_eq!(head(&mut h), before + 1, "one transaction");
    let (label, ops) = last_ops(&mut h);
    assert_eq!(label, "Rename page");
    assert!(
        matches!(ops.as_slice(), [Op::RemovePage { id: a, .. }, Op::AddPage { id: b, page, index: 2 }] if a == b && *a == spare && page.title == "Spare 2"),
        "{ops:?}"
    );
    assert!(h.query_by_label("Page 3: Spare 2").is_some());
    ok(&mut h, "u");
    h.run_steps(2);
    assert!(h.query_by_label("Page 3: Spare").is_some(), "u: back to Spare");
    // Refusals: empty and 201 characters keep the field open with the message; nothing recorded.
    for bad in ["   ".to_string(), "x".repeat(201)] {
        let before = head(&mut h);
        h.state_mut().panel.start_rename(spare);
        h.run_steps(2);
        h.state_mut().panel.rename.as_mut().unwrap().text = bad.clone();
        key(&mut h, Key::Enter, Modifiers::NONE);
        assert_eq!(head(&mut h), before, "{} characters: nothing recorded", bad.len());
        let r = h.state().panel.rename.clone();
        assert!(r.as_ref().is_some_and(|r| r.error.as_deref().is_some_and(|e| e.contains("title"))), "{r:?}");
        key(&mut h, Key::Escape, Modifiers::NONE);
        h.state_mut().panel.rename = None;
        h.run_steps(2);
    }

    // B: drag Spare above Supply: exactly `movepage {page, index: 1}`.
    let before = head(&mut h);
    let from = row_rect(&h, "Page 3: Spare").center();
    let to = row_rect(&h, "Page 1: Supply").center() - egui::vec2(0.0, ROW_H * 0.4);
    pointer(&mut h, from, Some(true));
    for k in 1..=6 {
        pointer(&mut h, from + (to - from) * (k as f32 / 6.0), None);
    }
    h.run_steps(2);
    assert_eq!(h.state().panel.dragging(), Some((spare, Some(0))), "dragging Spare, landing before Supply");
    // The screenshot with the insertion line between Supply and Field (clear of the list's edge).
    let between = row_rect(&h, "Page 1: Supply").center() + egui::vec2(0.0, ROW_H * 0.45);
    pointer(&mut h, between, None);
    h.run_steps(2);
    assert_eq!(h.state().panel.dragging(), Some((spare, Some(1))));
    let drag = take(&h, &deltas, "016-drag.png");
    pointer(&mut h, to, None);
    h.run_steps(2);
    pointer(&mut h, to, Some(false));
    h.run_steps(3);
    assert_eq!(head(&mut h), before + 1, "one transaction");
    let (label, ops) = last_ops(&mut h);
    assert_eq!(label, "Move page");
    assert!(matches!(ops.as_slice(), [Op::RemovePage { id, index: 2, .. }, Op::AddPage { index: 0, .. }] if *id == spare), "{ops:?}");
    assert_eq!(titles(&mut h), ["Spare", "Supply", "Field", "4"]);
    // Page numbers are stored (brief 019): the move changes no reference text.
    let texts: Vec<String> = ecad_cadcraft::references::markers_on_page(&doc(&mut h), supply).into_iter().map(|m| m.text).collect();
    assert_eq!(texts, ["2/F1"]);
    // Alt+Down on the cursor row (Spare, row 1): `movepage {index: 2}`.
    click(&mut h, "Page 3: Spare");
    let before = head(&mut h);
    key(&mut h, Key::ArrowDown, Modifiers::ALT);
    assert_eq!(head(&mut h), before + 1);
    assert_eq!(titles(&mut h), ["Supply", "Spare", "Field", "4"]);
    click(&mut h, project_panel::MOVE_DOWN);
    assert_eq!(titles(&mut h), ["Supply", "Field", "Spare", "4"], "the header's Move down");

    // C: a page change typed in the command window shows in the panel the next frame.
    ok(&mut h, &format!(r#"renamepage {{"page":"{}","title":"Field (typed)"}}"#, field.hex()));
    h.run_steps(1);
    assert!(h.query_by_label("Page 2: Field (typed)").is_some(), "the panel follows a typed command");
    let rebuilds = h.state().panel.rebuilds;
    h.run_steps(10);
    assert_eq!(h.state().panel.rebuilds, rebuilds, "no rebuild while nothing changes");

    // G (AC8): on Field draw a wire, make Spare active, `u`: Field comes back without the wire.
    click(&mut h, "Page 2: Field (typed)");
    let items = doc(&mut h).items.len();
    ok(&mut h, r#"wire {"points":[[0,-30],[40,-30]]}"#);
    click(&mut h, "Page 3: Spare");
    ok(&mut h, "u");
    assert_eq!(active(&mut h), Some(field), "Field is brought to the front");
    assert_eq!(doc(&mut h).items.len(), items, "the wire is gone");
    assert!(h.state().cad.session.log.iter().any(|l| l == "Page 2 \"Field (typed)\" brought to the front."));
    ok(&mut h, "redo");
    assert_eq!(active(&mut h), Some(field), "redo: Field stays");
    // Undo of `newpage` while its page is active: the page before it becomes active.
    click(&mut h, "Page 4: 4");
    let four = active(&mut h);
    ok(&mut h, r#"newpage {"title":"5"}"#);
    ok(&mut h, "u");
    assert_eq!(doc(&mut h).page_order.len(), 4);
    assert_eq!(active(&mut h), four, "the page before the removed page");

    // E: close the drawing tab: every page still listed, none active; a click shows a page.
    h.state_mut().cad.request_close(None);
    h.run_steps(3);
    assert!(h.state().project_open() && h.state().cad.session.docs.is_empty());
    assert!(h.query_by_label("Page 1: Supply").is_some() && h.query_by_label("Page 3: Spare").is_some());
    assert_eq!(active(&mut h), None);
    click(&mut h, "Page 2: Field (typed)");
    assert_eq!(active(&mut h), Some(field));
    assert_eq!(h.state().cad.session.docs.len(), 1);

    // Close the panel (UI rev 3: the strip's Book opens it again).
    click(&mut h, ecad_trial::shell::left::CLOSE);
    assert!(h.state().shell_state().left.collapsed && h.query_by_label("Page 1: Supply").is_none());
    let collapsed = take(&h, &deltas, "016-collapsed.png");
    click(&mut h, &ecad_trial::shell::tool_strip::panel_label(LeftTab::Pages));
    assert!(!h.state().shell_state().left.collapsed);

    if shots().is_some() {
        for (name, img) in [("panel", &panel), ("rename", &rename), ("drag", &drag), ("collapsed", &collapsed)] {
            assert!(img.is_some(), "016-{name}.png needs a wgpu adapter");
        }
    }
}

/// Check H (AC9) and the screenshot `016-preview-ref.png`: Preview Reference on Supply's
/// reference shows Field around the other end, follows moves, renames, edits, undo and redo of
/// its page only, records nothing and leaves the active page's selection and history alone.
#[test]
fn preview_reference_shows_the_other_end_and_records_nothing() {
    let deltas: Deltas = Arc::default();
    let mut h = build(trial(), Some(deltas.clone()));
    let (supply, field, spare) = fixture_p(&mut h);
    click(&mut h, "Page 1: Supply");
    // Select Supply's reference point, then Preview Reference (ECAD tab, Wiring).
    let point = {
        let t = h.state_mut();
        let p = t.project().unwrap().clone();
        p.read(&mut t.cad.session, |p, _| {
            let r = ecad_cadcraft::references::references_on_page(p.ledger.document(), supply).into_iter().next()?;
            p.pages.get(&supply)?.map.item(r.item?)
        })
        .unwrap()
        .expect("Supply's reference point")
    };
    // Brief 038: the ECAD Tool Sets tab is behind the developer switch, in the main view.
    h.state_mut().set_cad_chrome(true);
    h.set_size(egui::vec2(1920.0, 1200.0));
    h.run_steps(3);
    h.state_mut().cad.session.set_selection(vec![point]);
    click(&mut h, tools::TAB);
    let before = head(&mut h);
    click(&mut h, "Preview Reference");
    h.run_steps(3);
    let (page, ring, title, projections) = {
        let v = h.state().preview.as_ref().expect("the preview is open");
        (v.page, v.ring, v.title.clone(), v.projections)
    };
    assert_eq!(page, field);
    assert_eq!(ring.map(|p| (p.x, p.y)), Some((20.0, 5.0)), "centred on the other end");
    assert_eq!(title, "page 2: Field");
    assert_eq!(active(&mut h), Some(supply), "the active page stays");
    assert_eq!(h.state().cad.session.selection(), vec![point], "the selection stays");
    assert_eq!(head(&mut h), before, "nothing recorded");
    assert!(h.query_by_label(ecad_trial::preview::OPEN_PAGE).is_some());
    h.run_steps(20); // the window's fade-in
    let shot_img = take(&h, &deltas, "016-preview-ref.png");
    if let (Some(img), Some(rect)) = (&shot_img, h.state().preview.as_ref().and_then(|v| v.image_rect)) {
        let v = h.state().preview.as_ref().unwrap();
        let at = v.to_screen(rect, ecad_model::Pt::new(20.0, 5.0));
        assert!(magenta_near(img, at + egui::vec2(10.0, 0.0), 3), "the ring around the other end");
    }

    // Edits elsewhere do not redraw it; a change on its page or of its texts does.
    let n = h.state().preview.as_ref().unwrap().projections;
    assert_eq!(n, projections);
    ok(&mut h, r#"line {"points":[[0,-40],[30,-40]]}"#);
    h.run_steps(2);
    assert_eq!(h.state().preview.as_ref().unwrap().projections, n, "an edit on Supply does not touch Field");
    ok(&mut h, &format!(r#"movepage {{"page":"{}","index":1}}"#, field.hex()));
    h.run_steps(2);
    assert_eq!(h.state().preview.as_ref().unwrap().title, "page 2: Field", "the number stays with the page (brief 019)");
    ok(&mut h, &format!(r#"renamepage {{"page":"{}","title":"Field X"}}"#, field.hex()));
    h.run_steps(2);
    assert_eq!(h.state().preview.as_ref().unwrap().title, "page 2: Field X");
    ok(&mut h, "u");
    ok(&mut h, "u");
    h.run_steps(2);
    assert_eq!(h.state().preview.as_ref().unwrap().title, "page 2: Field", "undo");
    ok(&mut h, "redo");
    h.run_steps(2);
    assert_eq!(h.state().preview.as_ref().unwrap().title, "page 2: Field", "redo");
    assert!(h.state().preview.as_ref().unwrap().projections > n, "page changes redraw it");

    // Clicks, drags, wheel and keys inside it record nothing and keep the selection.
    let before = head(&mut h);
    let selection = h.state().cad.session.selection();
    let rect = h.state().preview.as_ref().unwrap().image_rect.unwrap();
    pointer(&mut h, rect.center(), Some(true));
    pointer(&mut h, rect.center() + egui::vec2(30.0, 10.0), None);
    pointer(&mut h, rect.center() + egui::vec2(30.0, 10.0), Some(false));
    h.run_steps(3);
    assert_eq!(head(&mut h), before, "nothing recorded");
    assert_eq!(h.state().cad.session.selection(), selection, "the active page's selection is unchanged");
    assert_eq!(active(&mut h), Some(supply));
    let draws = h.state().preview.as_ref().unwrap().draws;
    h.run_steps(10);
    assert_eq!(h.state().preview.as_ref().unwrap().draws, draws, "never redrawn per frame");

    // Open page makes Field active with the preview's view and closes it.
    click(&mut h, ecad_trial::preview::OPEN_PAGE);
    assert_eq!(active(&mut h), Some(field));
    assert!(h.state().preview.is_none());
    // The panel's Preview on a row, and Space on it again closes; the active page says so.
    click(&mut h, "Page 3: Spare");
    click(&mut h, "Page 1: Supply");
    key(&mut h, Key::Space, Modifiers::NONE);
    assert!(h.state().preview.is_none(), "the cursor row is active: no preview");
    click(&mut h, "Page 3: Spare");
    key(&mut h, Key::ArrowUp, Modifiers::NONE);
    key(&mut h, Key::Space, Modifiers::NONE);
    assert_eq!(h.state().preview.as_ref().map(|v| v.page), Some(supply), "Space previews the cursor row");
    key(&mut h, Key::Space, Modifiers::NONE);
    assert!(h.state().preview.is_none(), "Space again closes it");
    let _ = spare;
    if shots().is_some() {
        assert!(shot_img.is_some(), "016-preview-ref.png needs a wgpu adapter");
    }
}

/// Check D (AC4) and `016-large.png`: 500 pages. The list draws only its visible rows, never
/// rebuilds while nothing changes, rebuilds once after a rename, and scrolls to page 250.
/// The 500 pages are made here (empty): the large bench project opens far too slowly in test
/// builds, which check every model invariant on each replayed transaction. `ecad-bench`'s `panel ms`
/// column times the rebuild on the large project itself.
#[test]
fn five_hundred_pages_cost_only_the_visible_rows() {
    let deltas: Deltas = Arc::default();
    let t = trial();
    let mut h = build(t, Some(deltas.clone()));
    {
        let t = h.state_mut();
        let p = t.project().unwrap().clone();
        let have = p.read(&mut t.cad.session, |p, _| p.ledger.document().page_order.len()).unwrap();
        for i in have..500 {
            assert!(t.submit_command(&format!(r#"newpage {{"title":"P{}"}}"#, i + 1)).is_none());
        }
    }
    h.run_steps(3);
    let rows = h.state().panel.rows().len();
    assert!(rows >= 500, "{rows}");
    assert!(h.state().panel.rows_drawn <= 40, "virtualised: {} rows laid out", h.state().panel.rows_drawn);
    let rebuilds = h.state().panel.rebuilds;
    // Frame cost: panel shown against collapsed (reported; debug builds are noisy).
    let time = |h: &mut Harness<'static, Trial>| {
        let t = std::time::Instant::now();
        h.run_steps(20);
        t.elapsed().as_secs_f64() * 1000.0 / 20.0
    };
    let shown = time(&mut h);
    h.state_mut().shell.state.left.collapsed = true;
    let hidden = time(&mut h);
    h.state_mut().shell.state.left.collapsed = false;
    h.run_steps(2);
    assert_eq!(h.state().panel.rebuilds, rebuilds, "no rebuild per frame");
    // One rename: one rebuild, timed.
    let first = doc(&mut h).page_order[0];
    ok(&mut h, &format!(r#"renamepage {{"page":"{}","title":"Renamed"}}"#, first.hex()));
    let t0 = std::time::Instant::now();
    h.run_steps(1);
    let rename_frame = t0.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(h.state().panel.rebuilds, rebuilds + 1, "one rebuild after one rename");
    eprintln!(
        "016 AC4: {rows} pages; frame {shown:.2} ms with the panel, {hidden:.2} ms collapsed; frame with the rebuild after one renamepage {rename_frame:.2} ms; {} rows laid out",
        h.state().panel.rows_drawn
    );
    // Scroll to page 250 (cursor there, as End/PgDn would) and show it.
    let p250 = doc(&mut h).page_order[249];
    let label250 = project_panel::row_label("250", &doc(&mut h).pages[&p250].title);
    h.state_mut().show_page(p250).unwrap();
    h.run_steps(4);
    assert!(h.query_by_label(&label250).is_some(), "page 250's row is scrolled into view");
    assert_eq!(active(&mut h), Some(p250));
    let large = take(&h, &deltas, "016-large.png");
    if shots().is_some() {
        assert!(large.is_some(), "016-large.png needs a wgpu adapter");
    }
}

fn pixel(img: &egui::ColorImage, at: Pos2) -> egui::Color32 {
    let (x, y) = (at.x.round() as usize, at.y.round() as usize);
    img.pixels.get(y * img.size[0] + x).copied().unwrap_or_default()
}

fn magenta_near(img: &egui::ColorImage, at: Pos2, r: i32) -> bool {
    (-r..=r).any(|dx| {
        (-r..=r).any(|dy| {
            let c = pixel(img, at + egui::vec2(dx as f32, dy as f32));
            c.r() > 180 && c.b() > 180 && c.g() < 90
        })
    })
}

/// Check I: a read-only project (another session holds the file) lists and switches pages, but
/// New page, the move buttons and rename write nothing.
#[test]
fn a_read_only_project_lists_and_switches_but_writes_nothing() {
    let dir = std::env::temp_dir().join(format!("ecad-016-ro-{}", ecad_store::disk::unique()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("ro.ecad").display().to_string();
    let mut first = trial();
    assert!(first.submit_command(r#"newpage {"title":"Field"}"#).is_none());
    first.save_as(&path).unwrap();
    ecad_cadcraft::views::isolate_for_tests();
    let mut second = Trial::with_file(None, &path).unwrap();
    second.dismiss_notice();
    open_panels(&mut second);
    assert!(second.status().unwrap().file.read_only.is_some());
    let mut h = build(second, None);
    let before = head(&mut h);
    click(&mut h, "Page 2: Field");
    let field = doc(&mut h).page_order[1];
    assert_eq!(active(&mut h), Some(field), "switching works");
    // New page, New folder, Move up and Move down are disabled and named by why.
    let disabled = h.get_all_by_label(project_panel::READ_ONLY).count();
    assert_eq!(disabled, 4, "New page, New folder, Move up and Move down");
    h.get_all_by_label(project_panel::READ_ONLY).for_each(|n| n.click());
    h.run_steps(3);
    key(&mut h, Key::ArrowUp, Modifiers::ALT);
    key(&mut h, Key::F2, Modifiers::NONE);
    assert!(h.state().panel.rename.is_none(), "no rename");
    assert_eq!(head(&mut h), before, "nothing recorded");
    assert_eq!(doc(&mut h).page_order.len(), 2);
    // Brief 019, check J: every Properties field and dialog field read-only, Issue hidden; a
    // revision can still be reopened (the command reads only).
    assert!(!h.state().props.lock().unwrap().writable, "Properties fields are read-only");
    h.state_mut().open_project_popout(ecad_trial::sheets_ui::Popout::Info);
    assert!(h.state().project_props.as_ref().is_some_and(|d| d.read_only), "Project Properties read-only");
    h.state_mut().project_props = None;
    h.state_mut().open_revisions();
    assert!(h.state().revisions.as_ref().is_some_and(|d| d.read_only), "no pending row, no Issue");
    h.state_mut().revisions = None;
    assert_eq!(head(&mut h), before, "nothing recorded");
    drop(h);
    drop(first);
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------- Devices tab (after brief 018)

/// A coil `-K01` on page 1 and its NO contact on page 2 (column A), page 1 shown, Devices tab open.
pub(crate) fn devices_fixture(h: &mut Harness<'static, Trial>) -> (ecad_model::ComponentId, PageId, PageId) {
    h.state_mut().cad.session.viewport_px = (1280.0, 800.0);
    let t = h.state_mut();
    let page1 = {
        let p = t.project().unwrap().clone();
        p.read(&mut t.cad.session, |p, _| p.ledger.document().page_order[0]).unwrap()
    };
    let coil = t.cad.run("place", serde_json::json!({"symbol": "coil", "at": [0, 0]})).unwrap();
    let coil = coil["component"].as_str().unwrap().to_string();
    t.cad.run("newpage", serde_json::json!({"title": "Field"})).unwrap();
    t.cad.run("place", serde_json::json!({"symbol": "no", "at": [45, 0], "device": coil})).unwrap();
    let page2 = {
        let p = t.project().unwrap().clone();
        p.read(&mut t.cad.session, |p, _| p.ledger.document().page_order[1]).unwrap()
    };
    t.show_page(page1).unwrap();
    h.run_steps(2);
    // Brief 038 (fix round): the editable Devices table is Lists ▸ Device list (the left panel's
    // tab is navigation, at 264 px).
    h.state_mut().set_workspace(ecad_trial::shell::Workspace::Lists);
    h.state_mut().shell.lists = ecad_trial::shell::ListsView::Devices;
    h.run_steps(3);
    let c = ecad_model::ComponentId(u128::from_str_radix(&coil, 16).unwrap());
    (c, page1, page2)
}

/// The Devices tab: grouped by prefix, a click selects and unfolds a device, a click on a symbol
/// shows its page and selects it (the one page window), and the filter narrows the list.
#[test]
fn devices_tab_goes_to_a_symbol_and_filters() {
    let deltas: Deltas = Arc::default();
    let mut h = build(trial(), Some(deltas.clone()));
    let (coil, page1, page2) = devices_fixture(&mut h);
    assert!(h.query_by_label("Device -K01: Contactor / relay coil").is_some(), "the device is listed");
    click(&mut h, "Device -K01: Contactor / relay coil");
    assert_eq!(h.state().panel.devices.selected, Some(coil));
    // Brief 039: the row is a table row; its Pages cell lists both symbols and goes to the first.
    let pages = {
        use ecad_ui::table::TableSource;
        let d = &h.state().panel.devices;
        d.source.cell(coil, ecad_trial::lists::devices::PAGES).text().into_owned()
    };
    assert_eq!(pages, "1/F1, 2/F1");
    let before = head(&mut h);
    h.state_mut().show_page(page2).unwrap();
    h.run_steps(2);
    click(&mut h, "Device -K01: Contactor / relay coil");
    assert_eq!(active(&mut h), Some(page1), "the first symbol's page is shown");
    let selected = h.state().cad.session.selection();
    assert!(!selected.is_empty(), "the symbol is selected");
    let t = h.state_mut();
    let p = t.project().unwrap().clone();
    let mapped = p.read(&mut t.cad.session, |p, _| {
        let doc = p.ledger.document();
        let rep = doc.representations_of(ecad_model::Subject::Component(coil)).find(|(_, r)| r.page == page1).unwrap().0;
        ecad_cadcraft::device_list::symbol_target(p, rep).unwrap().2
    });
    assert_eq!(selected, mapped.unwrap(), "exactly the coil's drawing is selected");
    assert_eq!(head(&mut h), before, "a go to records nothing");
    assert_eq!(h.state().cad.session.docs.len(), 1, "one page window");
    let shot = take(&h, &deltas, "016-devices.png");
    // Filter: by tag and by text.
    ok(&mut h, r#"place {"symbol":"mcb1","at":[100,0]}"#);
    h.run_steps(2);
    assert_eq!(h.state().panel.devices.entries().len(), 2);
    h.state_mut().panel.devices.filter = "q".into();
    h.run_steps(2);
    let tags: Vec<_> = h.state().panel.devices.entries().iter().map(|e| e.tag.clone().unwrap_or_default()).collect();
    assert_eq!(tags, ["-Q01"]);
    h.state_mut().panel.devices.filter = "relay".into();
    h.run_steps(2);
    let tags: Vec<_> = h.state().panel.devices.entries().iter().map(|e| e.tag.clone().unwrap_or_default()).collect();
    assert_eq!(tags, ["-K01"]);
    let rebuilds = h.state().panel.devices.list.rebuilds;
    h.run_steps(10);
    assert_eq!(h.state().panel.devices.list.rebuilds, rebuilds, "no rebuild while nothing changes");
    if shots().is_some() {
        assert!(shot.is_some(), "016-devices.png needs a wgpu adapter");
    }
}

/// Editing a device property in the Devices tab is exactly `setprop`'s transaction and one `u`;
/// a tag another device has is refused with the merge offer, which merges in one step.
#[test]
fn devices_tab_edits_a_property_in_one_transaction() {
    let mut h = build(trial(), None);
    let (coil, ..) = devices_fixture(&mut h);
    click(&mut h, "Device -K01: Contactor / relay coil");
    let before = head(&mut h);
    h.get_by_label("Description field").click();
    h.run_steps(2);
    h.event(Event::Key { key: Key::A, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND });
    h.event(Event::Text("Main contactor".into()));
    h.run_steps(1);
    key(&mut h, Key::Enter, Modifiers::NONE);
    h.run_steps(2);
    assert_eq!(head(&mut h), before + 1, "one transaction");
    let (label, ops) = last_ops(&mut h);
    assert_eq!(label, "Set property");
    assert!(matches!(ops.as_slice(), [Op::SetProperty { key, after: Some(v), .. }] if key == "description" && v == "Main contactor"), "{ops:?}");
    assert!(h.query_by_label("Device -K01: Main contactor").is_some(), "the list follows");
    assert!(h.state().cad.cmd.buffer.is_empty() && h.state().cad.session.running.is_none(), "Enter stayed in the field");
    ok(&mut h, "u");
    h.run_steps(2);
    assert_eq!(head(&mut h), before + 2, "one undo");
    assert!(h.query_by_label("Device -K01: Contactor / relay coil").is_some(), "u puts it back");

    // A second coil given -K01's tag: refused with the merge offer; nothing written until it is
    // taken; the merge joins the contact to -K01 in one step.
    ok(&mut h, r#"place {"symbol":"no","at":[0,40]}"#);
    h.run_steps(2);
    let other = doc(&mut h).components.keys().copied().find(|c| *c != coil).unwrap();
    let before = head(&mut h);
    h.state_mut().panel_action(project_panel::Action::SetDeviceProperty { component: other, key: "tag", value: "K01".into(), conflict: None });
    h.run_steps(2);
    assert_eq!(head(&mut h), before, "refused");
    let err = h.state().panel.devices.error.clone().expect("the refusal is shown");
    assert!(err.0.contains("is used by -K01") && err.1.is_some(), "{err:?}");
    h.state_mut().panel.devices.selected = Some(other);
    h.run_steps(2);
    // Brief 026: the clash box names its buttons ecad.tagclash.*; Make it the same device is the second.
    click(&mut h, "ecad.tagclash.merge");
    assert_eq!(head(&mut h), before + 1, "the merge is one transaction: {:?} {:?}", h.state().panel.devices.error, h.state().cad.session.log.last());
    assert_eq!(last_ops(&mut h).0, "Merge into -K01");
    ok(&mut h, "u");
    assert_eq!(head(&mut h), before + 2, "and one undo");
}

/// Owner, 2026-11-08: the Devices list acts only on the ops that matter. With the tab open, a wire
/// (and its undo) leaves the list untouched; a tag rename refreshes only that device, regrouped;
/// moving a page renumbers the `/page.column` texts; nothing is ever rebuilt whole after the tab
/// is shown.
#[test]
fn devices_tab_refreshes_only_the_devices_a_change_touches() {
    let mut h = build(trial(), None);
    let (coil, page1, _page2) = devices_fixture(&mut h);
    ok(&mut h, r#"place {"symbol":"mcb1","at":[100,0]}"#);
    h.run_steps(2);
    let count = |h: &Harness<'static, Trial>| {
        let d = &h.state().panel.devices;
        (d.list.rebuilds, d.list.updates, d.list.devices_refreshed)
    };
    let (rebuilds, updates, refreshed) = count(&h);
    assert_eq!(h.state().panel.devices.entries().len(), 2);

    // A wire, its undo and redo: nothing in the list can differ, so nothing is done.
    ok(&mut h, r#"wire {"points":[[0,-40],[60,-40]]}"#);
    ok(&mut h, "u");
    ok(&mut h, "redo");
    ok(&mut h, r#"line {"points":[[0,-60],[60,-60]]}"#);
    h.run_steps(3);
    assert_eq!(count(&h), (rebuilds, updates, refreshed), "wires and lines leave the list untouched");

    // A tag rename: one update, one device refreshed, regrouped under its new prefix.
    let before = h.state().panel.devices.entries().to_vec();
    ok(&mut h, &format!(r#"setprop {{"component":"{}","key":"tag","value":"S7"}}"#, coil.hex()));
    h.run_steps(2);
    assert_eq!(count(&h), (rebuilds, updates + 1, refreshed + 1), "only that device");
    let after = h.state().panel.devices.entries().to_vec();
    let tags: Vec<_> = after.iter().map(|e| e.tag.clone().unwrap_or_default()).collect();
    assert_eq!(tags, ["-Q01", "-S07"], "re-sorted: -Q before -S");
    let other_before = before.iter().find(|e| e.id != coil).unwrap();
    let other_after = after.iter().find(|e| e.id != coil).unwrap();
    assert_eq!(other_before, other_after, "the other device is untouched");
    assert!(h.query_by_label("Device -S07: Contactor / relay coil").is_some(), "the row shows it");

    // A page move: the symbols are re-sorted in book order, without a rebuild; their page numbers
    // are stored (brief 019), so they stay.
    ok(&mut h, &format!(r#"movepage {{"page":"{}","index":2}}"#, page1.hex()));
    h.run_steps(2);
    let e = h.state().panel.devices.entries().iter().find(|e| e.id == coil).unwrap().clone();
    let numbers: Vec<_> = e.symbols.iter().map(|s| s.page_number.clone()).collect();
    assert_eq!(numbers, ["2", "1"], "the contact (page 2, now first in book order) first, the coil after");
    assert_eq!(e.symbols[1].name, "Contactor / relay coil");
    assert_eq!(h.state().panel.devices.list.rebuilds, rebuilds, "never rebuilt whole while shown");

    // Hidden, then shown again: rebuilt once.
    click(&mut h, project_panel::PAGES_TAB);
    click(&mut h, project_panel::DEVICES_TAB);
    assert_eq!(h.state().panel.devices.list.rebuilds, rebuilds + 1);
}

/// Review of PR #41 (S1, S3, W1): the page rows and the Find Symbol window follow their update
/// maps. Drawing edits (a wire, its undo and redo, a line) re-read neither; a device change re-reads
/// the search but not the page rows; a page rename re-reads the rows once; idle frames read
/// nothing.
#[test]
fn page_rows_and_find_symbol_are_read_only_when_their_inputs_change() {
    let mut h = build(trial(), None);
    let (coil, page1, _page2) = devices_fixture(&mut h);
    {
        let s = &mut h.state_mut().search;
        s.open = true;
        s.query = "k".into();
        s.picked = Some(ecad_trial::search::Pick::Symbol("no"));
    }
    h.run_steps(3);
    let count = |h: &Harness<'static, Trial>| (h.state().panel.rebuilds, h.state().search.rebuilds);
    let (rows, search) = count(&h);
    h.run_steps(5);
    assert_eq!(count(&h), (rows, search), "idle frames read nothing");

    ok(&mut h, r#"wire {"points":[[0,-40],[60,-40]]}"#);
    ok(&mut h, "u");
    ok(&mut h, "redo");
    ok(&mut h, r#"line {"points":[[0,-60],[60,-60]]}"#);
    h.run_steps(3);
    assert_eq!(count(&h), (rows, search), "wires and lines leave the page rows and the search alone");

    ok(&mut h, &format!(r#"setprop {{"component":"{}","key":"description","value":"Pump"}}"#, coil.hex()));
    h.run_steps(3);
    assert_eq!(h.state().panel.rebuilds, rows, "a device change leaves the page rows");
    assert_eq!(h.state().search.rebuilds, search + 2, "the rows and the second step, once each");

    ok(&mut h, &format!(r#"renamepage {{"page":"{}","title":"Renamed"}}"#, page1.hex()));
    h.run_steps(3);
    assert_eq!(h.state().panel.rebuilds, rows + 1, "a rename re-reads the rows once");
}

/// The developer tools live in the Project panel's Developer section, folded by default, and the
/// section hosts them itself (owner, 2026-11-08): the command field with its reply, the debugger
/// inline, and Inspect Project as readable text. No ECAD Dev tab, no right-hand debugger panel, no
/// floating command window.
#[test]
fn the_developer_section_hosts_the_command_the_debugger_and_inspect() {
    let deltas: Deltas = Arc::default();
    let mut h = build(trial(), Some(deltas.clone()));
    assert!(h.query_by_label("ECAD Dev").is_none(), "no ECAD Dev tab");
    assert!(h.query_by_label(ecad_trial::developer::FIELD).is_none(), "folded by default");
    // Brief 038: the Developer section is the Text panel's Commands tab (Ctrl+/).
    h.state_mut().toggle_text_panel();
    h.run_steps(3);
    assert!(h.state().developer.open);
    assert!(h.query_by_label("Export Inspector Snapshot").is_none(), "only with --snapshots");
    // The command field: Run shows the reply under it; one transaction.
    let before = head(&mut h);
    h.state_mut().command = r#"wire {"points":[[0,0],[40,0]]}"#.into();
    h.run_steps(1);
    click(&mut h, "Run");
    assert_eq!(head(&mut h), before + 1, "the line ran");
    let reply = h.state().developer.reply.join(
        "
",
    );
    assert!(reply.contains("conductor"), "the reply is shown in the section: {reply}");
    // The debugger, inline.
    click(&mut h, ecad_trial::developer::DEBUGGER);
    assert!(h.state().debugger.open, "{:?} {:?}", h.state().developer.body, h.state().developer.open);
    assert!(h.query_by_label("ECAD debugger (read-only)").is_some(), "drawn in the section");
    h.run_steps(3);
    let shot = take(&h, &deltas, "016-developer.png");
    if shots().is_some() {
        assert!(shot.is_some(), "016-developer.png needs a wgpu adapter");
    }
    // Inspect Project: readable, in the section.
    click(&mut h, ecad_trial::developer::INSPECT);
    let text = h.state().developer.inspect.clone().unwrap_or_default();
    assert!(text.contains("Pages (1):") && text.contains("Counts:") && text.contains("1 conductors"), "{text}");
    assert_eq!(head(&mut h), before + 1, "reading records nothing");
}
