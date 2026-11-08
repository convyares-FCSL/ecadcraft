//! Independent tester's UI checks for brief 010 (cable references across pages), written from the
//! brief and design 010 only, driving the trial headless (egui_kittest, elements by label, real
//! pointer events on the canvas) in a 1280×800 window.
//!
//! - Design §11 fixture R built with the buttons **Cable Reference** and **Continue Cable** and
//!   canvas clicks; the derived texts `/2.1` and `/1.2` (AC2), the hover lens (AC5), **Go to
//!   Reference** by label and back (AC5), the orphan after ERASE and `u` (AC3).
//! - Reference screenshots (design §11) written to `$ECAD_TESTER_SHOTS` when set and a wgpu
//!   adapter exists, with pixel checks on the rendered frames: `010-ref-source.png`,
//!   `010-ref-target.png`, `010-ref-orphan.png`, `010-ref-hover.png`, `010-continue-preview.png`.
//!
//! Every in-process harness points the project's view settings at a temporary folder.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use cadcraft_engine::geom::Vec2;
use ecad_cadcraft::views::Settings;
use ecad_model::{Document, PageId};
use ecad_trial::{Trial, tools};
use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

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

struct Tmp(PathBuf);

impl Tmp {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("ecad-010-ui-{name}-{}", ecad_store::disk::unique()));
        std::fs::create_dir_all(&p).unwrap();
        Tmp(p)
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn build(t: Trial, deltas: Option<Deltas>) -> Harness<'static, Trial> {
    let mut b = Harness::builder().with_size(egui::vec2(1280.0, 800.0));
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

/// A new-project harness whose views are remembered in `settings`.
fn harness(settings: &Tmp, deltas: Option<Deltas>) -> Harness<'static, Trial> {
    ecad_cadcraft::install();
    ecad_cadcraft::views::isolate_for_tests();
    let mut t = Trial::new(None).unwrap();
    t.hover_delay = 0.0;
    let p = t.project().unwrap().clone();
    let dir = settings.0.clone();
    p.configure(&mut t.cad.session, move |p| p.settings = Settings::at(dir)).unwrap();
    build(t, deltas)
}

fn head(h: &mut Harness<'static, Trial>) -> u64 {
    let t = h.state_mut();
    let p = t.project().unwrap().clone();
    p.read(&mut t.cad.session, |p, _| p.ledger.head()).unwrap()
}

fn screen(h: &Harness<'static, Trial>, x: f64, y: f64) -> Pos2 {
    h.state().cad.canvas.xf.expect("canvas laid out").to_screen(Vec2::new(x, y))
}

fn pointer_to(h: &mut Harness<'static, Trial>, at: Pos2) {
    h.event(Event::PointerMoved(at));
    h.run_steps(2);
}

fn click(h: &mut Harness<'static, Trial>, at: Pos2) {
    pointer_to(h, at);
    for pressed in [true, false] {
        h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    }
    h.run_steps(2);
}

fn prompt(h: &Harness<'static, Trial>) -> String {
    h.state().cad.session.current_prompt().map(|p| p.display()).unwrap_or_default()
}

fn ok(h: &mut Harness<'static, Trial>, line: &str) {
    let e = h.state_mut().submit_command(line);
    assert!(e.is_none(), "{line}: {e:?}");
    h.run_steps(2);
}

/// A drawn component: a closed rectangle typed, selected, converted (JSON form), three terminals.
fn component(h: &mut Harness<'static, Trial>, rectangle: &str, x: f64) {
    let t = h.state_mut();
    let before: Vec<_> = t.cad.session.doc().unwrap().model.iter().map(|e| e.handle).collect();
    t.cad.session.cmdline(rectangle).unwrap();
    let lines: Vec<String> = t
        .cad
        .session
        .doc()
        .unwrap()
        .model
        .iter()
        .map(|e| e.handle)
        .filter(|x| !before.contains(x))
        .map(|x| x.hex())
        .collect();
    assert_eq!(lines.len(), 4);
    t.cad.run("convertcomponent", json!({ "handles": lines })).unwrap();
    for y in [0, 5, 10] {
        t.cad.run("addterminal", json!({"handle": lines[0], "at": [x, y as f64]})).unwrap();
    }
    h.run_steps(1);
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

/// Anything brighter than the background (a drawn line) within `r` px.
fn drawn_near(img: &egui::ColorImage, at: Pos2, r: i32) -> bool {
    (-r..=r).any(|dx| {
        (-r..=r).any(|dy| {
            let c = pixel(img, at + egui::vec2(dx as f32, dy as f32));
            u32::from(c.r()) + u32::from(c.g()) + u32::from(c.b()) > 300
        })
    })
}

fn red_near(img: &egui::ColorImage, at: Pos2, r: i32) -> bool {
    (-r..=r).any(|dx| {
        (-r..=r).any(|dy| {
            let c = pixel(img, at + egui::vec2(dx as f32, dy as f32));
            c.r() > 180 && c.g() < 90 && c.b() < 90
        })
    })
}

fn doc(h: &mut Harness<'static, Trial>) -> Document {
    let t = h.state_mut();
    let p = t.project().unwrap().clone();
    p.read(&mut t.cad.session, |p, _| p.ledger.document().clone()).unwrap()
}

fn current_page(h: &mut Harness<'static, Trial>) -> PageId {
    let t = h.state_mut();
    let uid = t.cad.session.state().unwrap().uid;
    let p = t.project().unwrap().clone();
    p.read(&mut t.cad.session, |p, _| p.page_of(uid)).unwrap().expect("an ECAD page is current")
}

fn said(h: &Harness<'static, Trial>) -> String {
    h.state().cad.session.log.join("\n")
}

/// The derived text the current page's drawing shows for its reference.
fn shown_text(h: &mut Harness<'static, Trial>) -> String {
    let page = current_page(h);
    let d = doc(h);
    ecad_cadcraft::references::markers_on_page(&d, page).first().map(|m| m.text.clone()).unwrap_or_default()
}

/// Park the pointer on empty canvas (no hover) and let the frame settle.
fn park(h: &mut Harness<'static, Trial>) {
    let away = screen(h, 40.0, 40.0);
    pointer_to(h, away);
    h.run_steps(3);
}

/// Zoom to the fixture's page with room around it (a helper line, `zoom e`, `u`), selection cleared.
fn zoom_e(h: &mut Harness<'static, Trial>) {
    let page = current_page(h);
    let d = doc(h);
    let field = d.page_order.get(1) == Some(&page);
    let window = if field { r#"line {"points":[[5,-15],[115,25]]}"# } else { r#"line {"points":[[-15,-15],[85,25]]}"# };
    ok(h, window);
    ok(h, "zoom e");
    ok(h, "u");
    h.state_mut().cad.session.set_selection(Vec::new());
    h.run_steps(3);
}

/// The drawn handle of `page`'s reference point.
fn reference_point(h: &mut Harness<'static, Trial>, page: PageId) -> cadcraft_engine::doc::Handle {
    let d = doc(h);
    let r = ecad_cadcraft::references::references_on_page(&d, page).into_iter().next().expect("a reference");
    let t = h.state_mut();
    let p = t.project().unwrap().clone();
    p.read(&mut t.cad.session, |p, _| p.pages[&page].map.item(r.item.unwrap())).unwrap().expect("point drawn")
}

/// Tag every untagged component on `page` (`name`) and every untagged wire there by its first y.
fn tag_page(h: &mut Harness<'static, Trial>, page: PageId, name: &str) {
    let d = doc(h);
    let comps: Vec<_> =
        d.components.iter().filter(|(_, c)| !c.properties.contains_key("tag")).map(|(k, _)| *k).collect();
    for c in comps {
        let on_page = d.representations_of(ecad_model::Subject::Component(c)).any(|(_, r)| r.page == page);
        if on_page {
            ok(h, &format!(r#"setprop {{"component":"{}","key":"tag","value":"{name}"}}"#, c.hex()));
        }
    }
    let wires: Vec<_> =
        d.conductors.iter().filter(|(_, c)| !c.properties.contains_key("tag")).map(|(k, _)| *k).collect();
    for w in wires {
        if let Some((a, _)) = ecad_model::derive::route_on_page(&d, w, page).and_then(|(_, p)| p.path_ends()) {
            ok(h, &format!(r#"setprop {{"conductor":"{}","key":"tag","value":"W{}"}}"#, w.hex(), a.y));
        }
    }
}

/// Design §11 fixture R built by clicks, the reference screenshots, AC2, AC3 and AC5 in the UI.
#[test]
fn fixture_r_by_label_and_canvas_clicks_with_reference_screenshots() {
    let out = std::env::var_os("ECAD_TESTER_SHOTS").map(PathBuf::from);
    if let Some(dir) = &out {
        std::fs::create_dir_all(dir).unwrap();
    }
    let take = |h: &Harness<'static, Trial>, deltas: &Deltas, name: &str| {
        out.as_ref().and_then(|dir| shot(h, deltas, &dir.join(name)))
    };
    let settings = Tmp::new("r");
    let deltas: Deltas = Arc::default();
    let mut h = harness(&settings, Some(deltas.clone()));
    let supply = current_page(&mut h);
    ok(&mut h, &format!(r#"renamepage {{"page":"{}","title":"Supply"}}"#, supply.hex()));
    component(&mut h, "line -10,-5 0,-5 0,15 -10,15 c", 0.0);
    for line in [
        r#"wire {"points":[[0,0],[60,0]]}"#,
        r#"wire {"points":[[0,5],[60,5]]}"#,
        r#"wire {"points":[[0,10],[60,10]]}"#,
        r#"cableslash {"from":[28,-3],"to":[32,18],"tag":"C1"}"#,
    ] {
        ok(&mut h, line);
    }
    tag_page(&mut h, supply, "K1");
    h.get_by_label(tools::TAB).click();
    h.run_steps(3);
    zoom_e(&mut h);
    // Cable Reference: click the slash, then just right of the wire ends.
    let before = head(&mut h);
    h.get_by_label("Cable Reference").click();
    h.run_steps(3);
    assert_eq!(prompt(&h), "Select a cable to reference:");
    let slash = screen(&h, 30.0, 7.5);
    click(&mut h, slash);
    assert!(prompt(&h).starts_with("Specify reference point"), "{} at {slash:?}: {}", prompt(&h), said(&h));
    let at = screen(&h, 65.0, 5.0);
    click(&mut h, at);
    assert!(h.state().cad.session.running.is_none(), "the command ends: {}", said(&h));
    assert_eq!(head(&mut h), before + 1, "one transaction");
    assert!(said(&h).contains("Reference for cable C1 placed on page 1."), "{}", said(&h));
    assert_eq!(shown_text(&mut h), "/?", "an orphan until continued");
    // Field: K2 with three terminals, then Continue Cable: Enter, two clicks.
    ok(&mut h, r#"newpage {"title":"Field"}"#);
    let field = current_page(&mut h);
    assert_ne!(field, supply);
    component(&mut h, "line 100,-5 110,-5 110,15 100,15 c", 100.0);
    tag_page(&mut h, field, "K2");
    // Zoom so that both the continuation (x 15..40) and K2 (x 100..110) are on screen.
    zoom_e(&mut h);
    let before = head(&mut h);
    h.get_by_label("Continue Cable").click();
    h.run_steps(3);
    assert_eq!(prompt(&h), "Enter cable to continue or [List] <C1>:");
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert_eq!(prompt(&h), "Specify reference point:", "{}", said(&h));
    let p = screen(&h, 20.0, 5.0);
    click(&mut h, p);
    assert!(prompt(&h).starts_with("Specify direction and length of the wires"), "{}", prompt(&h));
    let toward = screen(&h, 40.0, 5.0);
    pointer_to(&mut h, toward);
    h.run_steps(3);
    let preview = take(&h, &deltas, "010-continue-preview.png");
    let preview_stubs = [0.0, 5.0, 10.0].map(|y| screen(&h, 30.0, y));
    assert_eq!(head(&mut h), before, "the preview records nothing");
    click(&mut h, toward);
    assert!(h.state().cad.session.running.is_none(), "{}", said(&h));
    assert_eq!(head(&mut h), before + 1, "one transaction");
    assert!(said(&h).contains("Cable C1 continued on page 2 with 3 wires"), "{}", said(&h));
    // Connect each stub's free end to K2 (JSON form; Connect's clicks are brief 008's).
    let d = doc(&mut h);
    let cable = *d.cables.keys().next().unwrap();
    let wires: Vec<_> = d.cables[&cable].members.iter().map(|m| m.conductor).collect();
    let k2: Vec<_> = ecad_model::derive::terminal_positions(&d, field).into_iter().map(|(t, _, p)| (t, p)).collect();
    for w in &wires {
        let end = ecad_model::derive::route_on_page(&d, *w, field).unwrap().1.path_ends().unwrap().1;
        let t = k2.iter().find(|(_, p)| (p.y - end.y).abs() < 1e-9).unwrap().0;
        ok(&mut h, &format!(r#"connect {{"conductor":"{}","end":"B","terminal":"{}"}}"#, w.hex(), t.hex()));
    }
    zoom_e(&mut h);
    assert_eq!(shown_text(&mut h), "/1.2");
    park(&mut h);
    let target = take(&h, &deltas, "010-ref-target.png");
    let target_ring = screen(&h, 20.0, 5.0);
    let target_text = screen(&h, 13.0, 5.0);
    // Soft checks, asserted at the end so the screenshots are still written.
    let mut soft: Vec<String> = Vec::new();
    // Go to Reference by label, click Field's reference point: Supply, its point selected.
    h.get_by_label("Go to Reference").click();
    h.run_steps(3);
    assert_eq!(prompt(&h), "Select a cable reference:");
    let p = screen(&h, 20.0, 5.0);
    click(&mut h, p);
    if current_page(&mut h) != supply {
        soft.push(format!("click on Field's reference point did not jump: {:?}", h.state().cad.session.log.last()));
        // Go on by the text, left of the point.
        let p = screen(&h, 13.0, 5.0);
        click(&mut h, p);
    }
    h.run_steps(3);
    assert_eq!(current_page(&mut h), supply, "Supply is current: {}", said(&h));
    let sp = reference_point(&mut h, supply);
    assert_eq!(h.state().cad.session.selection(), vec![sp], "the other end's point is selected");
    assert!(said(&h).contains(r#"Page 1 "Supply", at the other end of cable C1 (/1.2)."#), "{}", said(&h));
    assert_eq!(shown_text(&mut h), "/2.1");
    zoom_e(&mut h);
    park(&mut h);
    let source = take(&h, &deltas, "010-ref-source.png");
    let source_ring = screen(&h, 65.0, 5.0);
    let source_text = screen(&h, 72.0, 5.0);
    // Hover the reference: the lens names Field and K2.
    let r = screen(&h, 65.0, 5.0);
    pointer_to(&mut h, r);
    h.run_steps(6);
    let lines = h.state().hover_lines().map(<[String]>::to_vec).unwrap_or_default();
    assert!(lines.first().is_some_and(|l| l == "Cable reference  /2.1"), "hover: {lines:?}");
    assert!(lines.iter().any(|l| l == r#"Other end: page 2 "Field", column 1, 3 wires"#), "hover: {lines:?}");
    assert_eq!(lines.iter().filter(|l| l.contains(" -> K2:")).count(), 3, "hover: {lines:?}");
    let hover = take(&h, &deltas, "010-ref-hover.png");
    // Go back by pickfirst: Supply's point selected, the button jumps at once.
    park(&mut h);
    h.state_mut().cad.session.set_selection(vec![sp]);
    h.run_steps(2);
    let before = head(&mut h);
    h.get_by_label("Go to Reference").click();
    h.run_steps(3);
    assert_eq!(current_page(&mut h), field, "jumped straight to Field: {}", said(&h));
    assert_eq!(head(&mut h), before, "navigation records nothing");
    h.state_mut().cad.session.set_selection(Vec::new());
    h.run_steps(2);
    // Hover on Field's reference point (where the middle stub starts): the reference lens.
    let fpoint = screen(&h, 20.0, 5.0); // the view was centred on it by the jump;
    pointer_to(&mut h, fpoint);
    h.run_steps(6);
    let lines = h.state().hover_lines().map(<[String]>::to_vec).unwrap_or_default();
    if !lines.first().is_some_and(|l| l.starts_with("Cable reference")) {
        soft.push(format!("hover on Field's reference point shows {lines:?}, not the reference lens"));
    }
    park(&mut h);
    // AC2 in the UI: Move Page puts Field first; the text changes with one change recorded.
    ok(&mut h, &format!(r#"movepage {{"page":"{}","index":1}}"#, field.hex()));
    assert_eq!(head(&mut h), before + 1);
    assert_eq!(shown_text(&mut h), "/2.2");
    ok(&mut h, "u");
    assert_eq!(shown_text(&mut h), "/1.2");
    // AC3 in the UI: erase Field's reference point; Supply turns red; `u` restores.
    let fp = reference_point(&mut h, field);
    ok(&mut h, &format!(r#"erase {{"handles":["{}"]}}"#, fp.hex()));
    assert!(said(&h).contains("Cable C1's reference on page 1 now has no other end."), "{}", said(&h));
    ok(&mut h, &format!(r#"openpage {{"page":"{}"}}"#, supply.hex()));
    assert_eq!(current_page(&mut h), supply);
    assert_eq!(shown_text(&mut h), "/?");
    zoom_e(&mut h);
    park(&mut h);
    let orphan = take(&h, &deltas, "010-ref-orphan.png");
    let orphan_ring = screen(&h, 65.0, 5.0);
    let orphan_text = screen(&h, 71.0, 5.0);
    ok(&mut h, "u");
    assert_eq!(shown_text(&mut h), "/2.1", "undo restores the link");
    // Pixel checks when frames were rendered.
    let (Some(source), Some(target), Some(orphan), Some(_), Some(preview)) = (source, target, orphan, hover, preview)
    else {
        eprintln!("ECAD_TESTER_SHOTS not set or no wgpu adapter: screenshots skipped");
        assert!(soft.is_empty(), "{soft:#?}");
        return;
    };
    for at in preview_stubs {
        assert!(drawn_near(&preview, at, 2), "a previewed stub at {at:?} in 010-continue-preview.png");
    }
    assert!(magenta_near(&source, source_ring, 8), "magenta ring at Supply's reference");
    assert!(magenta_near(&source, source_text, 6), "magenta /2.1 right of Supply's reference");
    assert!(magenta_near(&target, target_ring, 8), "magenta ring at Field's reference");
    assert!(magenta_near(&target, target_text, 6), "magenta /1.2 left of Field's reference");
    assert!(red_near(&orphan, orphan_ring, 8), "red ring on the orphan");
    assert!(red_near(&orphan, orphan_text, 6), "red /? on the orphan");
    assert!(!magenta_near(&orphan, orphan_ring, 3), "no magenta ring on the orphan");
    assert!(soft.is_empty(), "{soft:#?}");
}
