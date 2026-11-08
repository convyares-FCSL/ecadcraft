//! Independent tester's UI checks for brief 005 (traditional / fan cable view), written from the
//! brief, design 005 and the owner's answers only, driving the trial headless (egui_kittest,
//! elements by label) in a 1280×800 window.
//!
//! - Design §3.1: the **Cable View** entry (`ecad.cableview`, Wiring, after Cable Slash).
//! - AC5 / design checks A, C, D: the entry clicked by label, the slash clicked on the canvas, the
//!   fan painted, the banner's change count unchanged, `u` undoing the slash and not the switch;
//!   reference screenshots `005-traditional.png` and `005-fan.png` written to `$ECAD_TESTER_SHOTS`
//!   when set and a wgpu adapter exists, with pixel checks on the rendered frames.
//! - AC2 through the trial's own open path (`Trial::with_file`, which reads `ECAD_SETTINGS_DIR`),
//!   in a child process so the variable never leaks into another test and the owner's settings are
//!   never read: each cable reopens in its view; a garbage settings file opens all traditional with
//!   the one-line message.
//!
//! Every in-process harness points the project's settings at a temporary folder.

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
        let p = std::env::temp_dir().join(format!("ecad-005-ui-{name}-{}", ecad_store::disk::unique()));
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

fn fan_lines(h: &mut Harness<'static, Trial>) -> usize {
    let d = h.state_mut().cad.session.doc().unwrap();
    d.model.iter().filter(|e| e.common.layer == views::LAYER).count()
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

/// Design §9 fixture G, then `zoom e`.
fn fixture(h: &mut Harness<'static, Trial>) {
    component(h, "line -10,-5 0,-5 0,15 -10,15 c", 0.0);
    component(h, "line 60,-5 70,-5 70,15 60,15 c", 60.0);
    for line in [
        r#"wire {"points":[[0,0],[60,0]]}"#,
        r#"wire {"points":[[0,5],[60,5]]}"#,
        r#"wire {"points":[[0,10],[60,10]]}"#,
        r#"cableslash {"from":[28,-3],"to":[32,18],"tag":"C1"}"#,
        "zoom e",
    ] {
        ok(h, line);
    }
    h.state_mut().cad.session.set_selection(Vec::new());
    h.run_steps(3);
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

/// Design §3.1: the entry's id, label, group, tooltip and place.
#[test]
fn the_cable_view_entry_is_in_wiring_after_cable_slash() {
    let tool = tools::TOOLS.iter().find(|t| t.id == "ecad.cableview").expect("ecad.cableview");
    assert_eq!(tool.label, "Cable View");
    assert_eq!(tool.group, "Wiring");
    assert_eq!(
        tool.tooltip,
        "Click a cable to switch it between traditional and fan view (changes nothing in the project)"
    );
    assert!(matches!(tool.kind, tools::Kind::Start("cableview")));
    let tab = tools::tab();
    let wiring = tab.groups.iter().find(|g| g.name == "Wiring").expect("Wiring group");
    let ids: Vec<&str> = wiring.tools.iter().map(|t| t.id.as_str()).collect();
    let at = |id: &str| ids.iter().position(|x| *x == id).unwrap_or_else(|| panic!("{id} in {ids:?}"));
    assert_eq!(at("ecad.cableview"), at("ecad.cableslash") + 1, "{ids:?}");
}

/// AC5 and design checks A, C, D, K: by label, both ways, banner unchanged, `u` undoes the slash,
/// the reference screenshots.
#[test]
fn ac5_cable_view_by_label_and_reference_screenshots() {
    let out = std::env::var_os("ECAD_TESTER_SHOTS").map(PathBuf::from);
    let settings = Tmp::new("ac5");
    let deltas: Deltas = Arc::default();
    let mut h = harness(&settings, Some(deltas.clone()));
    fixture(&mut h);
    let cable = views_of(&mut h).as_object().unwrap().keys().next().cloned().expect("C1");
    let before = head(&mut h);
    let status = h.state_mut().status().unwrap();
    // The ECAD tab open, the pointer parked on empty canvas for the screenshots (no hover).
    h.get_by_label(tools::TAB).click();
    h.run_steps(3);
    let away = screen(&h, 40.0, 30.0);
    pointer_to(&mut h, away);
    h.run_steps(3);
    let trad_img = out.as_ref().and_then(|dir| {
        std::fs::create_dir_all(dir).unwrap();
        shot(&h, &deltas, &dir.join("005-traditional.png"))
    });
    // A running Cable Slash is cancelled by the Cable View click; the Command field is emptied.
    h.get_by_label("Cable Slash").click();
    h.run_steps(3);
    h.state_mut().command = "stale text".into();
    h.get_by_label("Cable View").click();
    h.run_steps(3);
    assert_eq!(prompt(&h), "Select a cable to switch its view:");
    assert!(h.state().command.is_empty(), "the Command field is left empty");
    assert_eq!(head(&mut h), before, "starting writes nothing");
    // Click the slash: fan on the same frame.
    let slash = screen(&h, 30.0, 7.5);
    click(&mut h, slash);
    assert_eq!(views_of(&mut h)[&cable], "fan");
    assert_eq!(fan_lines(&mut h), 16, "brief 009: trunk 2, box 2, spines 2, bends 4, tails 6");
    assert!(h.state().cad.session.log.iter().any(|l| l == "Cable C1 now shown in fan view."), "echo");
    assert_eq!(prompt(&h), "Select a cable to switch its view:", "the prompt repeats");
    pointer_to(&mut h, away);
    h.run_steps(3);
    let fan_img = out.as_ref().and_then(|dir| shot(&h, &deltas, &dir.join("005-fan.png")));
    // Clicks on things that are not cables.
    let empty = screen(&h, 30.0, 30.0);
    click(&mut h, empty);
    assert!(
        h.state()
            .cad
            .session
            .log
            .last()
            .is_some_and(|l| l.contains("Nothing there. Click a cable slash, a fan or a wire in a cable.")),
        "{:?}",
        h.state().cad.session.log.last()
    );
    let body = screen(&h, -10.0, 5.0);
    click(&mut h, body);
    assert!(
        h.state().cad.session.log.last().is_some_and(|l| l.contains("That is not a cable or a wire in a cable.")),
        "{:?}",
        h.state().cad.session.log.last()
    );
    h.key_press(Key::Escape);
    h.run_steps(2);
    assert!(h.state().cad.session.running.is_none());
    assert_eq!(views_of(&mut h)[&cable], "fan", "Esc keeps the switch");
    // Hover on the trunk (design §4.2): the cable, `View: fan`.
    // Brief 009: the trunk joins the middle of the spine, at y = 5, left of the label box.
    let trunk = screen(&h, 24.0, 5.0);
    pointer_to(&mut h, trunk);
    h.run_steps(5);
    let lines = h.state().hover_lines().map(<[String]>::to_vec).unwrap_or_default();
    assert!(lines.iter().any(|l| l == "View: fan"), "hover: {lines:?}");
    assert!(lines.iter().any(|l| l == "Cable C1"), "hover: {lines:?}");
    // Nothing recorded, the banner's change count and unsaved state unchanged.
    assert_eq!(head(&mut h), before);
    let now = h.state_mut().status().unwrap();
    assert_eq!((now.unsaved, now.pages), (status.unsaved, status.pages), "banner unchanged");
    // AC4 / check D: `u` undoes the slash, not the switch; redo restores it, still fan.
    ok(&mut h, "u");
    assert!(views_of(&mut h).as_object().unwrap().is_empty(), "the cable is gone with the undo");
    assert_eq!(fan_lines(&mut h), 0);
    ok(&mut h, "redo");
    assert_eq!(views_of(&mut h)[&cable], "fan");
    assert_eq!(fan_lines(&mut h), 16, "brief 009: trunk 2, box 2, spines 2, bends 4, tails 6");
    // The setting was remembered in the temporary folder only.
    let text = std::fs::read_to_string(settings.0.join(views::FILE_NAME)).unwrap();
    assert!(text.contains(&cable), "{text}");
    // Pixel checks when frames were rendered.
    let (Some(trad), Some(fan)) = (trad_img, fan_img) else {
        eprintln!("ECAD_TESTER_SHOTS not set or no wgpu adapter: screenshots skipped");
        return;
    };
    // Traditional: magenta crossing dots on the slash; W0 drawn at (40,0); no magenta on the trunk
    // line between fan-out points away from the slash.
    let crossing = |y: f64| screen(&h, 28.0 + 4.0 * (y + 3.0) / 21.0, y);
    for y in [0.0, 5.0, 10.0] {
        assert!(magenta_near(&trad, crossing(y), 2), "crossing dot at y {y} in 005-traditional.png");
    }
    assert!(drawn_near(&trad, screen(&h, 40.0, 0.0), 1), "W0 drawn in traditional");
    assert!(!magenta_near(&trad, screen(&h, 24.0, 5.0), 1), "no magenta trunk in traditional");
    // Fan (brief 009's look, joining the middle): magenta trunk at (24,5) and tails at (10,0) and
    // (50,10); the routes not drawn between the spines (x = 30); no crossing dots at the slash.
    assert!(magenta_near(&fan, screen(&h, 24.0, 5.0), 1), "trunk in 005-fan.png");
    assert!(magenta_near(&fan, screen(&h, 10.0, 0.0), 1), "tail in 005-fan.png");
    assert!(magenta_near(&fan, screen(&h, 50.0, 10.0), 1), "tail in 005-fan.png");
    // y = 5 is the trunk's line now, so only the outer routes are checked.
    for y in [0.0, 10.0] {
        assert!(!drawn_near(&fan, screen(&h, 30.0, y), 1), "route at y {y} hidden in 005-fan.png");
    }
    assert!(!magenta_near(&fan, crossing(0.0), 2), "no crossing dot in fan view");
}

/// AC2 through the trial's open path, in a child process (see the module docs).
#[test]
fn ac2_trial_reopens_each_cable_in_its_view() {
    let settings = Tmp::new("reopen-settings");
    let work = Tmp::new("reopen-work");
    let decoy = Tmp::new("reopen-decoy");
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "cable_view_tester::child_trial_reopen", "--ignored", "--nocapture", "--test-threads=1"])
        .env("ECAD_SETTINGS_DIR", &settings.0)
        .env("APPDATA", &decoy.0)
        .env("HOME", &decoy.0)
        .env("XDG_CONFIG_HOME", &decoy.0)
        .env("ECAD_TESTER_WORK", &work.0)
        .output()
        .unwrap();
    let text = format!("{}\n{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success() && text.contains("1 passed"), "child failed:\n{text}");
    assert_eq!(std::fs::read_dir(&decoy.0).unwrap().count(), 0, "nothing in APPDATA/HOME");
}

#[test]
#[ignore = "run by ac2_trial_reopens_each_cable_in_its_view in a child process"]
fn child_trial_reopen() {
    let Some(work) = std::env::var_os("ECAD_TESTER_WORK").map(PathBuf::from) else {
        panic!("run only as a child of ac2_trial_reopens_each_cable_in_its_view");
    };
    let settings = PathBuf::from(std::env::var_os("ECAD_SETTINGS_DIR").unwrap());
    ecad_cadcraft::install();
    ecad_cadcraft::views::isolate_for_tests();
    let mut t = Trial::new(None).unwrap();
    t.hover_delay = 0.0;
    let mut h = build(t, None);
    fixture(&mut h);
    for line in [
        r#"wire {"points":[[0,30],[60,30]]}"#,
        r#"wire {"points":[[0,35],[60,35]]}"#,
        r#"wire {"points":[[0,40],[60,40]]}"#,
        r#"cableslash {"from":[30,25],"to":[30,45],"tag":"C2"}"#,
    ] {
        ok(&mut h, line);
    }
    let path = work.join("trial.ecad").display().to_string();
    h.state_mut().save_as(&path).unwrap();
    let vs = views_of(&mut h);
    let mut ids: Vec<String> = vs.as_object().unwrap().keys().cloned().collect();
    ids.sort();
    // Which is C1: the cable whose fan has the worked example's trunk.
    let r = h.state_mut().cad.run("cableview", json!({"cable": ids[0], "view": "fan"})).unwrap();
    assert_eq!(r["remembered"], true, "{r}");
    let (fan_id, trad_id) = (ids[0].clone(), ids[1].clone());
    assert_eq!(h.state_mut().status().unwrap().unsaved, 0, "a switch is not an unsaved change");
    drop(h);
    // Reopen through the trial (reads ECAD_SETTINGS_DIR).
    ecad_cadcraft::views::isolate_for_tests();
    let t = Trial::with_file(None, &path).unwrap();
    let mut h = build(t, None);
    assert!(h.state().notice().is_none_or(|n| !n.contains("could not be read")), "{:?}", h.state().notice());
    let vs = views_of(&mut h);
    assert_eq!((vs[&fan_id].as_str(), vs[&trad_id].as_str()), (Some("fan"), Some("traditional")), "{vs}");
    assert_eq!(fan_lines(&mut h), 16, "brief 009: trunk 2, box 2, spines 2, bends 4, tails 6");
    assert_eq!(h.state_mut().status().unwrap().unsaved, 0);
    drop(h);
    // Garbage settings: all traditional, the message once, the open works.
    std::fs::write(settings.join(views::FILE_NAME), [0xde, 0xad, 0xbe, 0xef, 0x00, 0x7b]).unwrap();
    ecad_cadcraft::views::isolate_for_tests();
    let t = Trial::with_file(None, &path).unwrap();
    let mut h = build(t, None);
    let log = h.state().cad.session.log.clone();
    assert_eq!(log.iter().filter(|l| l.as_str() == views::UNREADABLE).count(), 1, "{log:?}");
    eprintln!("notice shown at open: {:?}", h.state().notice());
    let vs = views_of(&mut h);
    assert_eq!((vs[&fan_id].as_str(), vs[&trad_id].as_str()), (Some("traditional"), Some("traditional")));
    assert_eq!(fan_lines(&mut h), 0);
    // The next switch rewrites the file.
    let r = h.state_mut().cad.run("cableview", json!({"cable": trad_id, "view": "fan"})).unwrap();
    assert_eq!(r["remembered"], true, "{r}");
    let v: Value = serde_json::from_str(&std::fs::read_to_string(settings.join(views::FILE_NAME)).unwrap()).unwrap();
    assert_eq!(v["format"], views::FORMAT);
    h.state_mut().save().unwrap();
}
