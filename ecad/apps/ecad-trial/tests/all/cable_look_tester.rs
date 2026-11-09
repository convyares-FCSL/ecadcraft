//! Independent tester's UI checks for brief 009 (cable look and cores), written from the brief,
//! design 009 and the overseer's decision that reference ends use brief 010's model (a cable
//! representation with role `reference` holding one Point item, built directly here), driving the
//! trial headless (egui_kittest, elements by label) in a 1280×800 window.
//!
//! - Design §3 / check A: **Core Label** (`ecad.cablecore`) clicked by its label, a wire clicked
//!   on the canvas, a label typed: the prompt, the echo, one transaction.
//! - Check P / design §9: lineweights are shown when the trial opens a project.
//! - Reference screenshots (design §10), written to `$ECAD_TESTER_SHOTS` when set and a wgpu
//!   adapter exists, with pixel checks on the rendered frames: `009-traditional-cores.png`,
//!   `009-fan.png`, `009-fan-one-sided.png`, `009-fan-small-zoom.png`, `009-core-prompt.png`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use cadcraft_engine::geom::Vec2;
use ecad_cadcraft::views::{self, Settings};
use ecad_trial::{Trial, tools};
use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};

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
        let p = std::env::temp_dir().join(format!("ecad-009-ui-{name}-{}", ecad_store::disk::unique()));
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

fn views_of(h: &mut Harness<'static, Trial>) -> Value {
    h.state_mut().cad.run("cableview", json!({})).unwrap()["views"].clone()
}

#[allow(dead_code)]
fn fan_lines(h: &mut Harness<'static, Trial>) -> usize {
    let d = h.state_mut().cad.session.doc().unwrap();
    d.model.iter().filter(|e| e.common.layer != ecad_cadcraft::frame::LAYER).filter(|e| e.common.layer == views::LAYER).count()
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

/// A drawn component at `x`: a closed rectangle typed, converted (JSON form), terminals at
/// y = 0, 5, 10, 15.
fn component(h: &mut Harness<'static, Trial>, rectangle: &str, x: f64) {
    let t = h.state_mut();
    let before: Vec<_> =
        t.cad.session.doc().unwrap().model.iter().filter(|e| e.common.layer != ecad_cadcraft::frame::LAYER).map(|e| e.handle).collect();
    t.cad.session.cmdline(rectangle).unwrap();
    let lines: Vec<String> = t
        .cad
        .session
        .doc()
        .unwrap()
        .model
        .iter()
        .filter(|e| e.common.layer != ecad_cadcraft::frame::LAYER)
        .map(|e| e.handle)
        .filter(|x| !before.contains(x))
        .map(|x| x.hex())
        .collect();
    assert_eq!(lines.len(), 4);
    t.cad.run("convertcomponent", json!({ "handles": lines })).unwrap();
    for y in [0, 5, 10, 15] {
        t.cad.run("addterminal", json!({"handle": lines[0], "at": [x, y as f64]})).unwrap();
    }
    h.run_steps(1);
}

/// Fixture H of design §10 (H1 without the right-hand component), then `zoom e`. Returns the
/// cable's hex id and the wires' hex ids by y.
fn fixture(h: &mut Harness<'static, Trial>, right: bool) -> (String, Vec<String>) {
    component(h, "line -10,-5 0,-5 0,20 -10,20 c", 0.0);
    if right {
        component(h, "line 120,-5 130,-5 130,20 120,20 c", 120.0);
    }
    let mut wires = Vec::new();
    for (i, y) in [0, 5, 10, 15].into_iter().enumerate() {
        let r = h.state_mut().cad.run("wire", json!({"points": [[0, y], [120, y]]})).unwrap();
        let w = r["conductor"].as_str().unwrap().to_string();
        let tag = format!("W{}", i + 1);
        h.state_mut().cad.run("setprop", json!({"conductor": w, "key": "tag", "value": tag})).unwrap();
        wires.push(w);
    }
    let r = h.state_mut().cad.run("cableslash", json!({"from": [58, -3], "to": [62, 18], "tag": "C1"})).unwrap();
    let cable = r["cable"].as_str().unwrap().to_string();
    for (k, v) in [("designation", "MOTOR M1"), ("type", "4/C #10")] {
        h.state_mut().cad.run("setprop", json!({"cable": cable, "key": k, "value": v})).unwrap();
    }
    assert!(h.state_mut().zoom_to_drawing(), "zoom to the drawing");
    h.state_mut().cad.session.set_selection(Vec::new());
    h.run_steps(3);
    (cable, wires)
}

/// A reference end of the cable at `at` with brief 010's ops (one Point item in a `reference`
/// representation), one transaction.
fn reference_end(h: &mut Harness<'static, Trial>, cable: &str, at: (f64, f64)) {
    use ecad_model::{CableId, Color, Form, Item, ItemId, ItemKind, Op, Primitive, Pt, Representation, RepresentationId, Style, Subject};
    // Combined trial of 009 and 010: the first reference end of a cable on a page is placed with
    // 010's `cableref`; a second one there (010 refuses it; only a hand-edited file has one) is still
    // built with the ops.
    if h.state_mut().cad.run("cableref", json!({"cable": cable, "at": [at.0, at.1]})).is_ok() {
        h.run_steps(2);
        return;
    }
    let t = h.state_mut();
    let p = t.project().unwrap().clone();
    let (page, layer) = p.read(&mut t.cad.session, |p, _| (p.ledger.document().page_order[0], p.default_layer)).unwrap();
    let k = CableId::parse(cable).unwrap();
    let item = ItemId::from(0x0977_0000_0000_0000_0000_0000_0000_0001);
    let rep = RepresentationId::from(0x0978_0000_0000_0000_0000_0000_0000_0001);
    let style = Style { layer, color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 };
    let ops = vec![
        Op::AddItem { id: item, item: Item { page, style, kind: ItemKind::Primitive(Primitive::Point { at: Pt::new(at.0, at.1) }) } },
        Op::LinkRepresentation {
            id: rep,
            rep: Representation {
                subject: Subject::Cable(k),
                page,
                form: Form::Drawn,
                role: ecad_cadcraft::look::REFERENCE_ROLE.into(),
                items: vec![item],
                connection_points: Default::default(),
            },
        },
    ];
    p.apply_ops(&mut t.cad.session, "Cable reference", ops).unwrap();
    h.run_steps(2);
}

/// Pixels of `img` along a vertical line at `x` from `y0` to `y1` that are magenta.
fn magenta_run(img: &egui::ColorImage, x: f32, y0: f32, y1: f32) -> usize {
    let (a, b) = (y0.min(y1).round() as i32, y0.max(y1).round() as i32);
    (a..=b)
        .filter(|y| {
            let c = pixel(img, Pos2::new(x, *y as f32));
            c.r() > 180 && c.b() > 180 && c.g() < 90
        })
        .count()
}

/// Check P / design §9: a project opened in the trial shows lineweights.
#[test]
fn lineweights_are_shown_when_the_trial_opens_a_project() {
    let settings = Tmp::new("lw");
    let h = harness(&settings, None);
    assert!(h.state().cad.session.settings.lwdisplay, "LWDISPLAY on after opening a project");
}

/// Design §3: the entry's id, label, group, tooltip and place.
#[test]
fn the_core_label_entry_is_in_wiring_after_cable_view() {
    let tool = tools::TOOLS.iter().find(|t| t.id == "ecad.cablecore").expect("ecad.cablecore");
    assert_eq!(tool.label, "Core Label");
    assert_eq!(tool.group, "Wiring");
    assert_eq!(tool.tooltip, "Click a wire in a cable and type its core number or colour");
    assert!(matches!(tool.kind, tools::Kind::Start("cablecore")));
    let tab = tools::tab();
    let wiring = tab.groups.iter().find(|g| g.name == "Wiring").expect("Wiring group");
    let ids: Vec<&str> = wiring.tools.iter().map(|t| t.id.as_str()).collect();
    let at = |id: &str| ids.iter().position(|x| *x == id).unwrap_or_else(|| panic!("{id} in {ids:?}"));
    assert_eq!(at("ecad.cablecore"), at("ecad.cableview") + 1, "{ids:?}");
    // Combined trial of briefs 009 and 010 (overseer decision): Core Label, Cable Join, then 010's
    // three buttons, then Add to Cable.
    assert_eq!(at("ecad.cablefan"), at("ecad.cablecore") + 1, "{ids:?}");
    // Brief 016 puts Preview Reference after Go to Reference.
    assert_eq!(at("ecad.cablepreview"), at("ecad.cablegoto") + 1, "{ids:?}");
    assert_eq!(at("ecad.cableadd"), at("ecad.cablepreview") + 1, "{ids:?}");
}

/// Check A and the reference screenshots of design §10 for fixture H.
#[test]
fn check_a_core_label_by_label_and_reference_screenshots() {
    let out = std::env::var_os("ECAD_TESTER_SHOTS").map(PathBuf::from);
    if let Some(dir) = &out {
        std::fs::create_dir_all(dir).unwrap();
    }
    let settings = Tmp::new("shots");
    let deltas: Deltas = Arc::default();
    let mut h = harness(&settings, Some(deltas.clone()));
    let (cable, wires) = fixture(&mut h, true);
    h.get_by_label(tools::TAB).click();
    h.run_steps(3);
    let away = screen(&h, 60.0, 45.0);
    // Check A: Core Label by its label, click W3's route, type 3.
    let before = head(&mut h);
    h.state_mut().command = "stale".into();
    h.get_by_label("Core Label").click();
    h.run_steps(3);
    assert_eq!(prompt(&h), "Select a wire in a cable:");
    assert!(h.state().command.is_empty(), "the Command field is left empty");
    let w3 = screen(&h, 30.0, 10.0);
    click(&mut h, w3);
    assert_eq!(prompt(&h), "Core number or colour for W3 <none>:");
    pointer_to(&mut h, away);
    h.run_steps(3);
    let prompt_img = out.as_ref().and_then(|d| shot(&h, &deltas, &d.join("009-core-prompt.png")));
    ok(&mut h, "3");
    assert!(h.state().cad.session.log.iter().any(|l| l == "Wire W3 is now core 3 of cable C1."), "{:?}", h.state().cad.session.log.last());
    assert_eq!(head(&mut h), before + 1, "one transaction");
    assert_eq!(prompt(&h), "Select a wire in a cable:", "back to step 1");
    h.key_press(Key::Escape);
    h.run_steps(2);
    assert!(h.state().cad.session.running.is_none());
    // The other three by the JSON form.
    for (i, w) in wires.iter().enumerate() {
        if i != 2 {
            h.state_mut().cad.run("cablecore", json!({"conductor": w, "core": format!("{}", i + 1)})).unwrap();
        }
    }
    h.run_steps(2);
    pointer_to(&mut h, away);
    h.run_steps(3);
    let trad_img = out.as_ref().and_then(|d| shot(&h, &deltas, &d.join("009-traditional-cores.png")));
    // Fan view by clicking Cable View and the slash.
    h.get_by_label("Cable View").click();
    h.run_steps(3);
    let slash = screen(&h, 60.0, 7.5);
    click(&mut h, slash);
    assert_eq!(views_of(&mut h)[&cable], "fan");
    h.key_press(Key::Escape);
    h.run_steps(2);
    pointer_to(&mut h, away);
    h.run_steps(3);
    let fan_img = out.as_ref().and_then(|d| shot(&h, &deltas, &d.join("009-fan.png")));
    // Pixel checks when frames were rendered.
    let (Some(trad), Some(fan), Some(prompt_img)) = (trad_img, fan_img, prompt_img) else {
        eprintln!("ECAD_TESTER_SHOTS not set or no wgpu adapter: screenshots skipped");
        return;
    };
    // Small zoom: about a quarter of the size (taken last: it changes the view the checks use).
    let small_zoom = |h: &mut Harness<'static, Trial>| {
        ok(h, "zoom s 0.25x");
        pointer_to(h, away);
        h.run_steps(3);
        out.as_ref().and_then(|d| shot(h, &deltas, &d.join("009-fan-small-zoom.png")))
    };
    // Traditional: magenta core labels just past the slash crossings (design §6).
    for y in [0.0, 5.0, 10.0, 15.0] {
        let x = 58.0 + 4.0 * (y + 3.0) / 21.0;
        assert!(magenta_near(&trad, screen(&h, x + 2.0, y + 2.0), 6), "core label at y {y}");
    }
    // Fan: the routes hidden between the spines (x = 80, clear of the label box, which now sits on
    // the wires' rows); the trunk (thick) at y = 7.5 left of the box; the spines; tails (thin) at
    // y = 5 near x = 10.
    for y in [0.0, 5.0, 10.0, 15.0] {
        assert!(!drawn_near(&fan, screen(&h, 80.0, y), 1), "route at y {y} hidden");
    }
    // The trunk joins the middle of each spine (owner request of 2026-11-07): y = 7.5.
    assert!(magenta_near(&fan, screen(&h, 35.0, 7.5), 1), "trunk");
    assert!(magenta_near(&fan, screen(&h, 22.5, 10.0), 1), "left spine");
    assert!(magenta_near(&fan, screen(&h, 97.5, 10.0), 1), "right spine");
    let thick = magenta_run(&fan, screen(&h, 35.0, 7.5).x, screen(&h, 35.0, 9.0).y, screen(&h, 35.0, 6.0).y);
    let thin = magenta_run(&fan, screen(&h, 10.0, 5.0).x, screen(&h, 10.0, 6.0).y, screen(&h, 10.0, 4.0).y);
    assert!(thin >= 1 && thick > thin, "thick trunk ({thick} px) and thin tail ({thin} px)");
    // Design §5.1 step 2: "The picked wire is highlighted." W3 (y = 10) must look different from
    // W2 (y = 5) somewhere between x = 20 and x = 50 while the prompt is shown.
    let brightest = |at: Pos2| {
        (-2..=2)
            .map(|dy| pixel(&prompt_img, at + egui::vec2(0.0, dy as f32)))
            .max_by_key(|c| u32::from(c.r()) + u32::from(c.g()) + u32::from(c.b()))
            .unwrap_or_default()
    };
    let differs = (0..=60).any(|i| {
        let x = 20.0 + f64::from(i) * 0.5;
        brightest(screen(&h, x, 10.0)) != brightest(screen(&h, x, 5.0))
    });
    let small = small_zoom(&mut h);
    assert!(small.is_some(), "009-fan-small-zoom.png");
    assert!(differs, "W3 is not highlighted in 009-core-prompt.png (design 009 §5.1 step 2)");
}

/// Design §10 screenshot 3: H1 in fan view with a reference end on the right.
#[test]
fn one_sided_fan_reference_screenshot() {
    let out = std::env::var_os("ECAD_TESTER_SHOTS").map(PathBuf::from);
    if let Some(dir) = &out {
        std::fs::create_dir_all(dir).unwrap();
    }
    let settings = Tmp::new("one-sided");
    let deltas: Deltas = Arc::default();
    let mut h = harness(&settings, Some(deltas.clone()));
    let (cable, wires) = fixture(&mut h, false);
    let cores: Vec<Value> = (1..=wires.len()).map(|i| json!(i.to_string())).collect();
    h.state_mut().cad.run("cablecore", json!({"cable": cable, "cores": cores})).unwrap();
    reference_end(&mut h, &cable, (125.0, 21.25));
    h.state_mut().cad.run("cableview", json!({"cable": cable, "view": "fan"})).unwrap();
    assert!(h.state_mut().zoom_to_drawing(), "zoom to the drawing");
    h.get_by_label(tools::TAB).click();
    h.run_steps(3);
    let away = screen(&h, 60.0, 45.0);
    pointer_to(&mut h, away);
    h.run_steps(3);
    let Some(img) = out.as_ref().and_then(|d| shot(&h, &deltas, &d.join("009-fan-one-sided.png"))) else {
        eprintln!("ECAD_TESTER_SHOTS not set or no wgpu adapter: screenshot skipped");
        return;
    };
    // Left side fans (spine at x = 22.5); the trunk runs on to the flag at x ≈ 123.75; no spine on
    // the right; the routes are hidden at x = 110.
    assert!(magenta_near(&img, screen(&h, 22.5, 10.0), 1), "left spine");
    assert!(magenta_near(&img, screen(&h, 110.0, 22.5), 1), "trunk to the reference");
    assert!(!magenta_near(&img, screen(&h, 97.5, 10.0), 1), "no right spine");
    for y in [0.0, 5.0, 10.0, 15.0] {
        assert!(!drawn_near(&img, screen(&h, 110.0, y), 1), "route at y {y} hidden near the reference end");
    }
}

/// Owner request of 2026-11-07: fixture H in fan view with each end's join moved by `cablefan`
/// (left to the top core, right to the bottom core), screenshot `009-fan-moved-join.png`.
#[test]
fn moved_join_reference_screenshot() {
    let out = std::env::var_os("ECAD_TESTER_SHOTS").map(PathBuf::from);
    if let Some(dir) = &out {
        std::fs::create_dir_all(dir).unwrap();
    }
    let settings = Tmp::new("moved-join");
    let deltas: Deltas = Arc::default();
    let mut h = harness(&settings, Some(deltas.clone()));
    let (cable, wires) = fixture(&mut h, true);
    let cores: Vec<Value> = (1..=wires.len()).map(|i| json!(i.to_string())).collect();
    h.state_mut().cad.run("cablecore", json!({"cable": cable, "cores": cores})).unwrap();
    h.state_mut().cad.run("cableview", json!({"cable": cable, "view": "fan"})).unwrap();
    let r = h.state_mut().cad.run("cablefan", json!({"cable": cable, "end": "A", "at": [22.5, 100]})).unwrap();
    assert_eq!(r["at"], json!([22.5, 15.0]), "clamped to the top core");
    let r = h.state_mut().cad.run("cablefan", json!({"cable": cable, "end": "B", "at": [97.5, -100]})).unwrap();
    assert_eq!(r["at"], json!([97.5, 0.0]), "clamped to the bottom core");
    assert!(h.state_mut().zoom_to_drawing(), "zoom to the drawing");
    h.get_by_label(tools::TAB).click();
    h.run_steps(3);
    let away = screen(&h, 60.0, 45.0);
    pointer_to(&mut h, away);
    h.run_steps(3);
    let Some(img) = out.as_ref().and_then(|d| shot(&h, &deltas, &d.join("009-fan-moved-join.png"))) else {
        eprintln!("ECAD_TESTER_SHOTS not set or no wgpu adapter: screenshot skipped");
        return;
    };
    // The trunk leaves the left spine at the top core (y = 15) and the right one at the bottom
    // core (y = 0): magenta at both junction rows next to their spines.
    assert!(magenta_near(&img, screen(&h, 25.0, 15.0), 1), "left join at the top");
    assert!(magenta_near(&img, screen(&h, 95.0, 0.0), 1), "right join at the bottom");
}

/// Round 2 (review nit): the trial turns lineweights on at the first project it opens, but a user
/// who turned them off with Show/Hide Lineweight keeps that choice when the next project opens.
#[test]
fn lineweight_display_keeps_the_users_choice_across_projects() {
    let settings = Tmp::new("lw-toggle");
    let work = Tmp::new("lw-toggle-work");
    let mut h = harness(&settings, None);
    assert!(h.state().cad.session.settings.lwdisplay, "on after the first project opens");
    // The user turns it off (LWDISPLAY, as Show/Hide Lineweight does).
    h.state_mut().cad.session.settings.lwdisplay = false;
    let path = work.0.join("lw.ecad").display().to_string();
    h.state_mut().save_as(&path).unwrap();
    // Closing the project (the Project panel's Close; nothing unsaved, so nothing is asked).
    h.state_mut().close_project();
    h.run_steps(2);
    assert!(!h.state().project_open());
    h.state_mut().open_project(&path).unwrap();
    h.run_steps(2);
    assert!(!h.state().cad.session.settings.lwdisplay, "the user's choice is kept on the next open");
    // And a new project after that keeps it too.
    // Closing the project (the Project panel's Close; nothing unsaved, so nothing is asked).
    h.state_mut().close_project();
    h.state_mut().new_project().unwrap();
    assert!(!h.state().cad.session.settings.lwdisplay, "kept for a new project");
}
