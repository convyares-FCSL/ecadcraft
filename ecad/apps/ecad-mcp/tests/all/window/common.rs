//! Helpers shared by this crate's tests: the kittest harness around a [`App`], a
//! temporary folder, and a pointer click.
//!
//! [`harness`] takes [`Opts`]; the defaults are the window most tests used (1920×1200, CADCraft's
//! developer chrome on, a fresh app with the hover delay off). Tests that need a different shape
//! say so in the options instead of copying the builder.

#![allow(dead_code)]

use std::path::PathBuf;

use ecad_app::shell::app::App;
use ecad_engine::cables::views::Settings;
use eframe::egui::{self, Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;

use std::sync::{Arc, Mutex};

/// Texture updates in order: `Some(image)` sets, `None` frees.
pub(crate) type Deltas = Arc<Mutex<Vec<(egui::TextureId, Option<egui::epaint::ImageDelta>)>>>;

/// Keeps every texture delta so a frame can be rendered later.
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

/// Where a harness's app keeps remembered cable views.
pub(crate) enum Views {
    /// Leave the app's own setting (the tests isolate it from the developer's folder anyway).
    Untouched,
    /// Remember nothing.
    Nothing,
    /// Remember them in this folder.
    At(PathBuf),
}

/// What [`harness`] builds.
pub(crate) struct Opts {
    size: egui::Vec2,
    chrome: bool,
    views: Views,
    deltas: Option<Deltas>,
    app: Option<App>,
    panels: bool,
}

impl Opts {
    pub(crate) fn new() -> Self {
        Opts { size: egui::vec2(1920.0, 1200.0), chrome: true, views: Views::Untouched, deltas: None, app: None, panels: false }
    }

    pub(crate) fn size(mut self, w: f32, h: f32) -> Self {
        self.size = egui::vec2(w, h);
        self
    }

    /// CADCraft's Tool Sets, layout tabs and command line sit behind the shell's developer switch
    /// (brief 038); on by default.
    pub(crate) fn chrome(mut self, on: bool) -> Self {
        self.chrome = on;
        self
    }

    pub(crate) fn views_nothing(mut self) -> Self {
        self.views = Views::Nothing;
        self
    }

    pub(crate) fn views_at(mut self, dir: PathBuf) -> Self {
        self.views = Views::At(dir);
        self
    }

    /// Keep every texture delta, so a frame can be rendered later.
    pub(crate) fn deltas(mut self, deltas: Option<Deltas>) -> Self {
        self.deltas = deltas;
        self
    }

    /// Use this app instead of a fresh one.
    pub(crate) fn app(mut self, t: App) -> Self {
        self.app = Some(t);
        self
    }

    /// Open both side panels first (`project_panel::open_panels`).
    pub(crate) fn panels(mut self) -> Self {
        self.panels = true;
        self
    }
}

/// A kittest harness around an app, run for five frames.
pub(crate) fn harness(opts: Opts) -> Harness<'static, App> {
    ecad_engine::install();
    ecad_engine::cables::views::isolate_for_tests();
    let mut t = match opts.app {
        Some(t) => t,
        None => {
            let mut t = App::new(None).unwrap();
            t.hover_delay = 0.0;
            t
        }
    };
    if opts.panels {
        t.shell.state.left.collapsed = false;
        t.shell.state.right_collapsed = false;
    }
    match opts.views {
        Views::Untouched => {}
        Views::Nothing => {
            let p = t.project().unwrap().clone();
            p.configure(&mut t.cad.session, |p| p.settings = Settings::none()).unwrap();
        }
        Views::At(dir) => {
            let p = t.project().unwrap().clone();
            p.configure(&mut t.cad.session, move |p| p.settings = Settings::at(dir)).unwrap();
        }
    }
    let mut b = Harness::builder().with_size(opts.size);
    if let Some(d) = opts.deltas {
        b = b.renderer(Collect(d));
    }
    let mut h = b.build_ui_state(
        |ui, t: &mut App| {
            t.logic(&ui.ctx().clone());
            t.ui(ui);
        },
        t,
    );
    if opts.chrome {
        h.state_mut().set_cad_chrome(true);
    }
    h.run_steps(5);
    h
}

/// A temporary folder, removed when dropped.
pub(crate) struct Tmp(pub(crate) PathBuf);

impl Tmp {
    pub(crate) fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("ecad-ui-{name}-{}", ecad_store::disk::unique()));
        std::fs::create_dir_all(&p).unwrap();
        Tmp(p)
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Move the pointer to `at` and let two frames pass.
pub(crate) fn pointer_to(h: &mut Harness<'static, App>, at: Pos2) {
    h.event(Event::PointerMoved(at));
    h.run_steps(2);
}

/// Press and release the primary button at `at`.
pub(crate) fn click(h: &mut Harness<'static, App>, at: Pos2) {
    pointer_to(h, at);
    for pressed in [true, false] {
        h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    }
    h.run_steps(2);
}

/// Connection points at `at` on the active page (design 072: a wire runs between two connection
/// points): one drawn device holding one point per place, committed straight to the ledger.
/// Returns the points, in the order given.
pub(crate) fn points_at(t: &mut App, at: &[(f64, f64)]) -> Vec<ecad_model::ConnectionPointId> {
    use ecad_model::{Author, ConnectionPoint, ConnectionPointId, Device, DeviceId, Form, Op, Pt, Representation, RepresentationId, Subject};
    let p = t.project().unwrap().clone();
    p.configure(&mut t.cad.session, |p| {
        let page = p.active_page().unwrap_or(p.ledger.document().page_order()[0]);
        let device: DeviceId = p.ids.fresh();
        let mut ops = vec![Op::CreateDevice { id: device, device: Device::default() }];
        let mut cps = std::collections::BTreeMap::new();
        let mut out = Vec::new();
        for (i, (x, y)) in at.iter().enumerate() {
            let t: ConnectionPointId = p.ids.fresh();
            ops.push(Op::AddConnectionPoint {
                id: t,
                connection_point: ConnectionPoint { owner: device.into(), label: Some((i + 1).to_string()), key: None, accepts: Default::default() },
            });
            cps.insert(t, Pt::new(*x, *y));
            out.push(t);
        }
        let rep: RepresentationId = p.ids.fresh();
        ops.push(Op::LinkRepresentation {
            id: rep,
            rep: Representation {
                subject: Subject::Device(device),
                page,
                form: Form::Drawn,
                role: ecad_model::Role::Part(ecad_model::PartRole::Head),
                items: Vec::new(),
                connection_points: cps,
            },
        });
        p.ledger.commit(Author::User, "points", ops).unwrap();
        out
    })
    .unwrap()
}

/// `wire` through `points` (JSON form), first putting a connection point at each end that has
/// none (design 072: a line with a free end carries no wire). The command's JSON result.
pub(crate) fn wire_pp(t: &mut App, points: &[(f64, f64)]) -> serde_json::Value {
    let p = t.project().unwrap().clone();
    let shown: Vec<ecad_model::Pt> = p
        .read(&mut t.cad.session, |p, _| {
            let doc = p.ledger.document();
            let page = p.active_page().unwrap_or(doc.page_order()[0]);
            ecad_model::connectivity::derive::connection_point_positions(doc, page).into_iter().map(|(_, _, at)| at).collect()
        })
        .unwrap();
    let ends: Vec<(f64, f64)> = [points.first(), points.last()]
        .into_iter()
        .flatten()
        .copied()
        .filter(|(x, y)| !shown.iter().any(|q| q.near(ecad_model::Pt::new(*x, *y), 1e-6)))
        .collect();
    if !ends.is_empty() {
        points_at(t, &ends);
    }
    let pts: Vec<[f64; 2]> = points.iter().map(|(x, y)| [*x, *y]).collect();
    t.cad.run("wire", serde_json::json!({ "points": pts })).unwrap()
}

/// A reference end of `cable` on `page` at `at` (its Point and its `reference` representation,
/// styled like the cable's first reference), applied to the project as one edit: the end a cable
/// gains when it is continued on another page. Until lane F step F4 no command draws a cable on
/// two pages (`cablecontinue` needs wires leaving the page through the reference), so tests about
/// what two ends say of each other (texts, previews, page panels) pair them so.
pub(crate) fn reference_end(t: &mut App, cable: ecad_model::CableId, page: ecad_model::PageId, at: (f64, f64)) {
    use ecad_model::{Form, Item, ItemId, ItemKind, Op, Primitive, Pt, Representation, RepresentationId, Subject};
    let p = t.project().unwrap().clone();
    let (style, item, rep) = p
        .configure(&mut t.cad.session, |p| {
            let doc = p.ledger.document();
            let style = doc
                .representations_of(Subject::Cable(cable))
                .filter(|(_, r)| r.role == ecad_model::Role::Reference)
                .find_map(|(_, r)| r.items.first().and_then(|i| doc.items().get(i)).map(|i| i.style.clone()))
                .expect("the cable has a reference to pair with");
            let (item, rep): (ItemId, RepresentationId) = (p.ids.fresh(), p.ids.fresh());
            (style, item, rep)
        })
        .unwrap();
    let ops = vec![
        Op::AddItem { id: item, item: Item { page, style, kind: ItemKind::Primitive(Primitive::Point { at: Pt::new(at.0, at.1) }) } },
        Op::LinkRepresentation {
            id: rep,
            rep: Representation {
                subject: Subject::Cable(cable),
                page,
                form: Form::Drawn,
                role: ecad_model::Role::Reference,
                items: vec![item],
                connection_points: Default::default(),
            },
        },
    ];
    p.apply_ops(&mut t.cad.session, "Reference end", ops).unwrap();
}

/// Render the harness's last frame with egui-wgpu (the canvas uses its CPU path in tests, so the
/// frame is plain egui shapes) and save it as PNG. `None` when no adapter is available.
pub(crate) fn shot<S>(h: &Harness<'static, S>, deltas: &Deltas, path: &std::path::Path) -> Option<egui::ColorImage> {
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

pub(crate) fn shots() -> Option<std::path::PathBuf> {
    std::env::var_os("ECAD_TESTER_SHOTS").map(std::path::PathBuf::from)
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
