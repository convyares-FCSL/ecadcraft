//! Brief 016 acceptance in the trial (design 016 §10, checks A–J), headless with egui_kittest in a
//! 1280×800 window: the Project panel by its labels, the one page window, undo bringing its page
//! to the front, the read-only preview, and the reference screenshots of design §10, written to
//! `$ECAD_TESTER_SHOTS` when it is set and a wgpu adapter exists (`016-panel.png`,
//! `016-rename.png`, `016-drag.png`, `016-collapsed.png`, `016-preview-ref.png`, `016-large.png`).
//!
//! Fixture P (design §10): 010's fixture R (pages `Supply` and `Field`, cable C1 with linked
//! references `/2.1` and `/1.2`) plus a third page `Spare`.

use std::sync::{Arc, Mutex};

use ecad_model::{Op, PageId};
use ecad_trial::project_panel::{self, ROW_H};
use ecad_trial::{Trial, tools};
use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

type Deltas = Arc<Mutex<Vec<(egui::TextureId, Option<egui::epaint::ImageDelta>)>>>;

struct Collect(Deltas);

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
fn shot(h: &Harness<'static, Trial>, deltas: &Deltas, path: &std::path::Path) -> Option<egui::ColorImage> {
    use eframe::egui_wgpu::{self, wgpu};
    let setup = egui_wgpu::WgpuSetup::without_display_handle();
    let instance = block_on(setup.new_instance());
    let config = egui_wgpu::WgpuConfiguration { wgpu_setup: setup, ..Default::default() };
    let rs =
        block_on(egui_wgpu::RenderState::create(&config, &instance, None, egui_wgpu::RendererOptions::PREDICTABLE))
            .ok()?;
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
    let screen = egui_wgpu::ScreenDescriptor {
        pixels_per_point: ppp,
        size_in_pixels: [size.x.round() as u32, size.y.round() as u32],
    };
    let tess = ctx.tessellate(h.output().shapes.clone(), ppp);
    let mut encoder = rs.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    let texture = rs.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: screen.size_in_pixels[0],
            height: screen.size_in_pixels[1],
            depth_or_array_layers: 1,
        },
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
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
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
    rs.device
        .poll(wgpu::PollType::Wait { submission_index: Some(idx), timeout: Some(std::time::Duration::from_secs(20)) })
        .ok()?;
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

fn build(t: Trial, deltas: Option<Deltas>) -> Harness<'static, Trial> {
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

fn trial() -> Trial {
    ecad_cadcraft::install();
    ecad_cadcraft::views::isolate_for_tests();
    let mut t = Trial::new(None).unwrap();
    t.hover_delay = 0.0;
    t
}

fn ok(h: &mut Harness<'static, Trial>, line: &str) {
    let e = h.state_mut().submit_command(line);
    assert!(e.is_none(), "{line}: {e:?}");
    h.run_steps(2);
}

fn doc(h: &mut Harness<'static, Trial>) -> ecad_model::Document {
    let t = h.state_mut();
    let p = t.project().unwrap().clone();
    p.read(&mut t.cad.session, |p, _| p.ledger.document().clone()).unwrap()
}

fn head(h: &mut Harness<'static, Trial>) -> u64 {
    let t = h.state_mut();
    let p = t.project().unwrap().clone();
    p.read(&mut t.cad.session, |p, _| p.ledger.head()).unwrap()
}

fn last_ops(h: &mut Harness<'static, Trial>) -> (String, Vec<Op>) {
    let t = h.state_mut();
    let p = t.project().unwrap().clone();
    p.read(&mut t.cad.session, |p, _| p.ledger.log().last().map(|x| (x.label.clone(), x.ops.clone())).unwrap()).unwrap()
}

fn active(h: &mut Harness<'static, Trial>) -> Option<PageId> {
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

fn click(h: &mut Harness<'static, Trial>, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(3);
}

fn key(h: &mut Harness<'static, Trial>, key: Key, modifiers: Modifiers) {
    h.event(Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers });
    h.event(Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers });
    h.run_steps(2);
}

fn pointer(h: &mut Harness<'static, Trial>, at: Pos2, pressed: Option<bool>) {
    h.event(Event::PointerMoved(at));
    if let Some(pressed) = pressed {
        h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    }
    h.run_steps(1);
}

/// The screen rectangle of the panel row labelled `label`.
fn row_rect(h: &Harness<'static, Trial>, label: &str) -> egui::Rect {
    h.get_by_label(label).rect()
}

/// Fixture P: Supply (C1 leaving at (65,5), `/2.1`), Field (C1 arriving at (20,5), `/1.2`), Spare.
fn fixture_p(h: &mut Harness<'static, Trial>) -> (PageId, PageId, PageId) {
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

fn shots() -> Option<std::path::PathBuf> {
    std::env::var_os("ECAD_TESTER_SHOTS").map(std::path::PathBuf::from)
}

fn take(h: &Harness<'static, Trial>, deltas: &Deltas, name: &str) -> Option<egui::ColorImage> {
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
    let shown = h.state().cad.session.doc().unwrap().model.len();
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
    assert_eq!(h.state().cad.cmd.buffer, "l", "and reaches the command line");
    h.state_mut().cad.cmd.buffer.clear();

    // Screenshot 016-panel: Field active, cursor on Spare.
    click(&mut h, "Page 2: Field");
    key(&mut h, Key::ArrowDown, Modifiers::NONE);
    let panel = take(&h, &deltas, "016-panel.png");

    // B: New page records exactly `newpage`'s transaction, titled with its number, and opens
    // its rename; Enter keeps the title (one transaction in total).
    let before = head(&mut h);
    click(&mut h, project_panel::NEW_PAGE);
    assert_eq!(head(&mut h), before + 1, "one transaction");
    let (label, ops) = last_ops(&mut h);
    assert_eq!(label, "New page");
    assert!(matches!(ops.as_slice(), [Op::AddPage { page, index: 3, .. }] if page.title == "4"), "{ops:?}");
    assert!(h.state().panel.rename.is_some(), "the new page's title opens for renaming");
    let new = active(&mut h).unwrap();
    key(&mut h, Key::Enter, Modifiers::NONE);
    assert!(h.state().panel.rename.is_none());
    assert_eq!(head(&mut h), before + 1, "Enter on the unchanged title writes nothing");
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
    h.event(Event::Key {
        key: Key::A,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    });
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
    assert!(
        matches!(ops.as_slice(), [Op::RemovePage { id, index: 2, .. }, Op::AddPage { index: 0, .. }] if *id == spare),
        "{ops:?}"
    );
    assert_eq!(titles(&mut h), ["Spare", "Supply", "Field", "4"]);
    // The reference texts follow the move at once (010): Supply's now says /3.1.
    let texts: Vec<String> =
        ecad_cadcraft::references::markers_on_page(&doc(&mut h), supply).into_iter().map(|m| m.text).collect();
    assert_eq!(texts, ["/3.1"]);
    // Alt+Down on the cursor row (Spare, row 1): `movepage {index: 2}`.
    click(&mut h, "Page 1: Spare");
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

    // Collapse to the strip and back.
    click(&mut h, project_panel::COLLAPSE);
    assert!(h.state().panel.collapsed && h.query_by_label("Page 1: Supply").is_none());
    let collapsed = take(&h, &deltas, "016-collapsed.png");
    click(&mut h, project_panel::EXPAND);
    assert!(!h.state().panel.collapsed);

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
    assert_eq!(h.state().preview.as_ref().unwrap().title, "page 1: Field", "the title follows the move");
    ok(&mut h, &format!(r#"renamepage {{"page":"{}","title":"Field X"}}"#, field.hex()));
    h.run_steps(2);
    assert_eq!(h.state().preview.as_ref().unwrap().title, "page 1: Field X");
    ok(&mut h, "u");
    ok(&mut h, "u");
    h.run_steps(2);
    assert_eq!(h.state().preview.as_ref().unwrap().title, "page 2: Field", "undo");
    ok(&mut h, "redo");
    h.run_steps(2);
    assert_eq!(h.state().preview.as_ref().unwrap().title, "page 1: Field", "redo");
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
    click(&mut h, "Page 2: Supply");
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
    h.state_mut().panel.collapsed = true;
    let hidden = time(&mut h);
    h.state_mut().panel.collapsed = false;
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
    assert!(second.status().unwrap().file.read_only.is_some());
    let mut h = build(second, None);
    let before = head(&mut h);
    click(&mut h, "Page 2: Field");
    let field = doc(&mut h).page_order[1];
    assert_eq!(active(&mut h), Some(field), "switching works");
    // New page, Move up and Move down are disabled and named by why.
    let disabled = h.get_all_by_label(project_panel::READ_ONLY).count();
    assert_eq!(disabled, 3, "New page, Move up and Move down");
    h.get_all_by_label(project_panel::READ_ONLY).for_each(|n| n.click());
    h.run_steps(3);
    key(&mut h, Key::ArrowUp, Modifiers::ALT);
    key(&mut h, Key::F2, Modifiers::NONE);
    assert!(h.state().panel.rename.is_none(), "no rename");
    assert_eq!(head(&mut h), before, "nothing recorded");
    assert_eq!(doc(&mut h).page_order.len(), 2);
    drop(h);
    drop(first);
    let _ = std::fs::remove_dir_all(&dir);
}
