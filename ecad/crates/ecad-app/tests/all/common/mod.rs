//! Helpers shared by this crate's tests (brief 062): the kittest harness around a [`App`], a
//! temporary folder, and a pointer click.
//!
//! [`harness`] takes [`Opts`]; the defaults are the window most tests used (1920×1200, CADCraft's
//! developer chrome on, a fresh app with the hover delay off). Tests that need a different shape
//! say so in the options instead of copying the builder.

#![allow(dead_code)]

use std::path::PathBuf;

use ecad_app::App;
use ecad_engine::views::Settings;
use eframe::egui::{self, Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;

use crate::mouse_tester::{Collect, Deltas};

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
    ecad_engine::views::isolate_for_tests();
    let mut t = match opts.app {
        Some(t) => t,
        None => {
            let mut t = App::new(None).unwrap();
            t.hover_delay = 0.0;
            t
        }
    };
    if opts.panels {
        crate::project_panel::open_panels(&mut t);
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
pub(crate) fn points_at(t: &mut App, at: &[(f64, f64)]) -> Vec<ecad_model::TerminalId> {
    use ecad_model::{Author, Device, DeviceId, Form, Op, Pt, Representation, RepresentationId, Subject, Terminal, TerminalId};
    let p = t.project().unwrap().clone();
    p.configure(&mut t.cad.session, |p| {
        let page = p.active_page().unwrap_or(p.ledger.document().page_order[0]);
        let device: DeviceId = p.ids.fresh();
        let mut ops = vec![Op::CreateDevice { id: device, device: Device::default() }];
        let mut cps = std::collections::BTreeMap::new();
        let mut out = Vec::new();
        for (i, (x, y)) in at.iter().enumerate() {
            let t: TerminalId = p.ids.fresh();
            ops.push(Op::AddTerminal {
                id: t,
                terminal: Terminal { owner: device.into(), label: Some((i + 1).to_string()), key: None, accepts: Default::default() },
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
            let page = p.active_page().unwrap_or(doc.page_order[0]);
            ecad_model::derive::terminal_positions(doc, page).into_iter().map(|(_, _, at)| at).collect()
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
                .find_map(|(_, r)| r.items.first().and_then(|i| doc.items.get(i)).map(|i| i.style.clone()))
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
