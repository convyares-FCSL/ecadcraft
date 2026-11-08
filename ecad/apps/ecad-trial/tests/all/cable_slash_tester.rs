//! Independent tester's UI checks for brief 004 (cable slash), written from the brief and design 004
//! only, driving the trial headless (egui_kittest, elements by label) in a 1280×800 window.
//!
//! - AC9 / design check A: the ECAD tab's **Cable Slash** entry (id `ecad.cableslash`, Wiring
//!   group, between Disconnect and Add to Cable) starts the interactive form with an empty Command
//!   field and the first prompt.
//! - Design checks D, G and N: two canvas clicks on fixture F make one cable of W0, W5, W10; the
//!   overlay paints one filled `ecad.cable` (magenta) 5 px dot at each member crossing and none on
//!   the plain line; a blocked second slash is refused and keeps the first point.
//! - AC9: reference screenshots `004-slash-preview.png`, `004-slash.png`, `004-slash-refused.png`,
//!   written to `$ECAD_TESTER_SHOTS` when set and a wgpu adapter exists; pixel checks then run on
//!   the rendered frames. `004-slash.png` is committed in `reference/` for the owner's approval.

use std::sync::{Arc, Mutex};

use cadcraft_engine::geom::Vec2;
use ecad_model::Document;
use ecad_trial::{Trial, tools};
use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

/// Texture updates in order: `Some(image)` sets, `None` frees.
type Deltas = Arc<Mutex<Vec<(egui::TextureId, Option<egui::epaint::ImageDelta>)>>>;

/// Keeps every texture delta so a frame can be rendered later (see `shot`).
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

fn harness_with(deltas: Option<Deltas>) -> Harness<'static, Trial> {
    ecad_cadcraft::install();
    ecad_cadcraft::views::isolate_for_tests();
    let mut t = Trial::new(None).unwrap();
    t.hover_delay = 0.0;
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

fn doc(h: &mut Harness<'static, Trial>) -> Document {
    let t = h.state_mut();
    let p = t.project().unwrap().clone();
    p.read(&mut t.cad.session, |p, _| p.ledger.document().clone()).unwrap()
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

fn button(h: &mut Harness<'static, Trial>, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(3);
}

fn prompt(h: &Harness<'static, Trial>) -> String {
    h.state().cad.session.current_prompt().map(|p| p.display()).unwrap_or_default()
}

fn ok(h: &mut Harness<'static, Trial>, line: &str) {
    let e = h.state_mut().submit_command(line);
    assert!(e.is_none(), "{line}: {e:?}");
    h.run_steps(2);
}

/// Design §10 fixture F, then `zoom e`.
fn fixture(h: &mut Harness<'static, Trial>) {
    ok(h, r#"wire {"points":[[0,0],[60,0]]}"#);
    ok(h, r#"wire {"points":[[0,5],[60,5]]}"#);
    ok(h, r#"wire {"points":[[0,10],[60,10]]}"#);
    h.state_mut().cad.run("line", json!({"points": [[0, 15], [60, 15]]})).unwrap();
    h.state_mut().cad.session.set_selection(Vec::new());
    ok(h, "zoom e");
    h.run_steps(3);
}

/// The overlay shapes the canvas painted this frame: (centre, size, colour) of circles and rects.
fn overlay_shapes(h: &Harness<'static, Trial>) -> Vec<(Pos2, f32, egui::Color32, bool)> {
    fn walk(s: &egui::Shape, out: &mut Vec<(Pos2, f32, egui::Color32, bool)>) {
        match s {
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
            egui::Shape::Circle(c) => {
                let filled = c.fill != egui::Color32::TRANSPARENT;
                out.push((c.center, c.radius * 2.0, if filled { c.fill } else { c.stroke.color }, filled));
            }
            egui::Shape::Rect(r) => out.push((r.rect.center(), r.rect.width(), r.stroke.color, false)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for c in &h.output().shapes {
        walk(&c.shape, &mut out);
    }
    out
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

/// Render the harness's last frame with egui-wgpu (the canvas uses its CPU path in tests, so the
/// frame is plain egui shapes) and save it as PNG. `None` when no adapter is available.
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

/// Whether a colour near `want` is painted within `r` pixels of `at`.
fn painted_near(img: &egui::ColorImage, at: Pos2, r: f32, want: egui::Color32) -> bool {
    let close = |c: egui::Color32| {
        c.r().abs_diff(want.r()) < 60 && c.g().abs_diff(want.g()) < 60 && c.b().abs_diff(want.b()) < 60
    };
    let ri = r.ceil() as i32;
    (-ri..=ri).any(|dx| (-ri..=ri).any(|dy| close(pixel(img, at + egui::vec2(dx as f32, dy as f32)))))
}

const MAGENTA: egui::Color32 = egui::Color32::from_rgb(255, 0, 255);
const GREEN: egui::Color32 = egui::Color32::from_rgb(0, 255, 0);
const RED: egui::Color32 = egui::Color32::from_rgb(255, 0, 0);

/// Fixture F: where the slash (28,-3) → (32,18) crosses the line at height `y` (design check N).
fn crossing(y: f64) -> (f64, f64) {
    (28.0 + 4.0 * (y + 3.0) / 21.0, y)
}

/// Design §3.1: the entry's id, label, group, place and tooltip; the Delete tooltip names cables.
#[test]
fn the_cable_slash_entry_sits_in_wiring_between_disconnect_and_add_to_cable() {
    let tool = tools::TOOLS.iter().find(|t| t.id == "ecad.cableslash").expect("ecad.cableslash");
    assert_eq!(tool.label, "Cable Slash");
    assert_eq!(tool.group, "Wiring");
    assert_eq!(tool.tooltip, "Draw a short line across wires to make them one cable");
    assert!(matches!(tool.kind, tools::Kind::Start("cableslash")));
    let tab = tools::tab();
    let wiring = tab.groups.iter().find(|g| g.name == "Wiring").expect("Wiring group");
    let ids: Vec<&str> = wiring.tools.iter().map(|t| t.id.as_str()).collect();
    let at = |id: &str| ids.iter().position(|x| *x == id).unwrap_or_else(|| panic!("{id} in {ids:?}"));
    assert_eq!(at("ecad.cableslash"), at("ecad.disconnect") + 1, "{ids:?}");
    // Design 005 §3.1 puts Cable View right after Cable Slash, before Add to Cable.
    assert_eq!(at("ecad.cableview"), at("ecad.cableslash") + 1, "{ids:?}");
    // Brief 009 puts Core Label and Cable Join after Cable View (design 009 §3, owner request), then
    // design 010 §3.1 puts Cable Reference, Continue Cable and Go to Reference, then Add to Cable
    // (combined order, overseer decision for the trial of 009 + 010).
    assert_eq!(at("ecad.cablecore"), at("ecad.cableview") + 1, "{ids:?}");
    assert_eq!(at("ecad.cablefan"), at("ecad.cablecore") + 1, "{ids:?}");
    assert_eq!(at("ecad.cableref"), at("ecad.cablefan") + 1, "{ids:?}");
    assert_eq!(at("ecad.cablecontinue"), at("ecad.cableref") + 1, "{ids:?}");
    assert_eq!(at("ecad.cablegoto"), at("ecad.cablecontinue") + 1, "{ids:?}");
    // Brief 016 puts Preview Reference after Go to Reference.
    assert_eq!(at("ecad.cablepreview"), at("ecad.cablegoto") + 1, "{ids:?}");
    assert_eq!(at("ecad.cableadd"), at("ecad.cablepreview") + 1, "{ids:?}");
    let delete = tools::TOOLS.iter().find(|t| t.id == "ecad.delete").unwrap();
    assert_eq!(
        delete.tooltip,
        "Delete a component, wire or cable with its connections (opens the ECAD command window)"
    );
}

/// AC9 and design checks A, D, G, N: the ECAD tab entry clicked by label, two clicks on the canvas,
/// one cable; crossing dots painted; a blocked second slash is refused; the reference screenshots.
#[test]
fn ac9_cable_slash_by_label_and_reference_screenshots() {
    let out = std::env::var_os("ECAD_TESTER_SHOTS").map(std::path::PathBuf::from);
    let deltas: Deltas = Arc::default();
    let mut h = harness_with(Some(deltas.clone()));
    // The slash is clicked between grid points: 5 mm snap (brief 018) off, as F9 does.
    h.state_mut().cad.session.settings.snapmode = false;
    fixture(&mut h);
    let before = head(&mut h);
    // Leave text in the Command field to check the click clears it (design §3.1).
    h.state_mut().command = "stale text".into();
    button(&mut h, tools::TAB);
    button(&mut h, "Cable Slash");
    assert_eq!(prompt(&h), "Specify first point of cable slash:");
    assert!(h.state().command.is_empty(), "the Command field is left empty");
    assert!(h.state().command_hint().is_none(), "no template note");
    assert_eq!(head(&mut h), before, "starting writes nothing");
    // First click, then the cursor at the second point: the preview (green rings, count).
    let a = screen(&h, 28.0, -3.0);
    click(&mut h, a);
    assert_eq!(prompt(&h), "Specify second point of cable slash or [Undo]:");
    let b = screen(&h, 32.0, 18.0);
    pointer_to(&mut h, b);
    h.run_steps(3);
    assert_eq!(head(&mut h), before, "the preview writes nothing");
    let preview_img = out.as_ref().and_then(|dir| {
        std::fs::create_dir_all(dir).unwrap();
        shot(&h, &deltas, &dir.join("004-slash-preview.png"))
    });
    // Commit.
    click(&mut h, b);
    assert_eq!(head(&mut h), before + 1, "one transaction");
    assert!(h.state().cad.session.running.is_none(), "the command ends");
    let d = doc(&mut h);
    assert_eq!(d.cables.len(), 1);
    let k = d.cables.values().next().unwrap();
    assert_eq!(k.members.len(), 3, "W0, W5, W10");
    let ys: Vec<f64> = k
        .members
        .iter()
        .map(|m| {
            let (_, prim) = ecad_model::derive::route_on_page(&d, m.conductor, d.page_order[0]).unwrap();
            prim.path_ends().unwrap().0.y
        })
        .collect();
    assert_eq!(ys, vec![0.0, 5.0, 10.0], "crossing order");
    // Design §7.3 / check M: hovering the slash lists the cable's wires in order; hovering writes
    // nothing.
    let (x, y) = crossing(7.5);
    let on_slash = screen(&h, x, y);
    pointer_to(&mut h, on_slash);
    h.run_steps(5);
    let lines = h.state().hover_lines().map(<[String]>::to_vec).unwrap_or_default();
    assert!(lines.iter().any(|l| l == "Cable (no tag)"), "hover on the slash: {lines:?}");
    assert!(lines.iter().any(|l| l == "Wires: 3"), "hover on the slash: {lines:?}");
    assert_eq!(head(&mut h), before + 1, "hovering writes nothing");
    // Pointer away (empty canvas, off the compass); dots at the three crossings, none on the plain
    // line, no end marks on the slash.
    let away = screen(&h, 10.0, -8.0);
    pointer_to(&mut h, away);
    h.run_steps(3);
    let shapes = overlay_shapes(&h);
    for y in [0.0, 5.0, 10.0] {
        let (x, y) = crossing(y);
        let at = screen(&h, x, y);
        assert!(
            shapes.iter().any(|s| (s.0 - at).length() < 1.0 && s.2 == MAGENTA && s.3 && (s.1 - 5.0).abs() < 0.01),
            "5 px magenta dot at {x},{y}: {:?}",
            shapes.iter().filter(|s| (s.0 - at).length() < 3.0).collect::<Vec<_>>()
        );
    }
    let (x, y) = crossing(15.0);
    let on_line = screen(&h, x, y);
    assert!(!shapes.iter().any(|s| (s.0 - on_line).length() < 3.0 && s.2 == MAGENTA), "no dot on the plain line");
    for (x, y) in [(28.0, -3.0), (32.0, 18.0)] {
        let at = screen(&h, x, y);
        assert!(!shapes.iter().any(|s| (s.0 - at).length() < 1.0), "the slash has no end marks at {x},{y}");
    }
    let done_img = out.as_ref().and_then(|dir| shot(&h, &deltas, &dir.join("004-slash.png")));
    // A second slash across W5 only: refused, the first point kept.
    button(&mut h, "Cable Slash");
    let (p, q) = (screen(&h, 40.0, 3.0), screen(&h, 40.0, 7.0));
    click(&mut h, p);
    pointer_to(&mut h, q);
    h.run_steps(3);
    let refused_img = out.as_ref().and_then(|dir| shot(&h, &deltas, &dir.join("004-slash-refused.png")));
    let head1 = head(&mut h);
    click(&mut h, q);
    assert_eq!(head(&mut h), head1, "refused: nothing written");
    assert_eq!(prompt(&h), "Specify second point of cable slash or [Undo]:", "the first point is kept");
    h.key_press(Key::Escape);
    h.run_steps(2);
    assert!(h.state().cad.session.running.is_none());
    assert_eq!(head(&mut h), head1);
    // Round 2 (design §5.2): a slash across no wires previews `-> no wires` in red. Rendered as
    // `004-slash-no-wires.png` (a tester check, not a reference image).
    button(&mut h, "Cable Slash");
    let (p, q) = (screen(&h, 10.0, -3.0), screen(&h, 14.0, -8.0));
    click(&mut h, p);
    pointer_to(&mut h, q);
    h.run_steps(3);
    let none_img = out.as_ref().and_then(|dir| shot(&h, &deltas, &dir.join("004-slash-no-wires.png")));
    let none_cursor = q;
    h.key_press(Key::Escape);
    h.run_steps(2);
    assert_eq!(head(&mut h), head1);
    if let Some(img) = &none_img {
        // Red text 10 px right of and 10 px above the cursor, and no green there.
        let count = |want: fn(egui::Color32) -> bool| {
            (8..160)
                .flat_map(|dx| (-30..0).map(move |dy| (dx, dy)))
                .filter(|(dx, dy)| want(pixel(img, none_cursor + egui::vec2(*dx as f32, *dy as f32))))
                .count()
        };
        let red = count(|c| c.r() > 180 && c.g() < 90 && c.b() < 90);
        let green = count(|c| c.g() > 180 && c.r() < 90 && c.b() < 90);
        assert!(red > 20 && green == 0, "`-> no wires` is red: {red} red, {green} green pixels");
    }
    // Pixel checks when frames were rendered.
    let Some(done) = done_img else {
        eprintln!("ECAD_TESTER_SHOTS not set or no wgpu adapter: screenshots skipped");
        return;
    };
    for y in [0.0, 5.0, 10.0] {
        let (x, y) = crossing(y);
        assert!(painted_near(&done, screen(&h, x, y), 2.0, MAGENTA), "magenta dot at {x},{y} in 004-slash.png");
    }
    if let Some(img) = preview_img {
        for y in [0.0, 5.0, 10.0] {
            let (x, y) = crossing(y);
            let ring = screen(&h, x, y) + egui::vec2(8.0, 0.0);
            assert!(painted_near(&img, ring, 2.0, GREEN), "green ring at {x},{y} in 004-slash-preview.png");
        }
    }
    if let Some(img) = refused_img {
        let ring = screen(&h, 40.0, 5.0) + egui::vec2(8.0, 0.0);
        assert!(painted_near(&img, ring, 2.0, RED), "red ring on W5 in 004-slash-refused.png");
    }
}
