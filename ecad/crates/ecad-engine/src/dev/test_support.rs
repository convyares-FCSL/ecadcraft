//! A builder for the small projects most tests start from (brief 062), behind the `test-support`
//! cargo feature. Never part of a release or the desktop app: only tests enable the feature.
//!
//! ```ignore
//! let built = ProjectBuilder::new(7)
//!     .page()
//!     .coil("-K1", (50.0, 50.0))
//!     .coil("-K2", (150.0, 50.0))
//!     .wire("-K1", "-K2")
//!     .cable(json!({"cores": "4G"}))
//!     .build();
//! ```
//!
//! Every step is the command a person or an agent would run (`newpage`, `place`, `wire`,
//! `cablenew`), so the result is the same project the commands make, with the same ids for the same
//! seed. Steps run in order, at [`ProjectBuilder::build`]. A step that a command refuses panics with
//! the command and its message, which is what a test wants; this module is the one place in the
//! crate where the never-crash rules do not apply.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use cadcraft_engine::Session;
use ecad_model::{CableId, DeviceId, Document, PageId, Pt, Subject, WireId};
use serde_json::{Value, json};

use crate::{Project, ProjectHandle};

/// One end of a wire the builder draws.
#[derive(Debug, Clone)]
pub enum End {
    /// A point on the page.
    At(f64, f64),
    /// The first connection point of the device with this tag (`"-K1"`), wherever it was placed.
    Device(String),
}

impl From<(f64, f64)> for End {
    fn from(p: (f64, f64)) -> Self {
        End::At(p.0, p.1)
    }
}

impl From<&str> for End {
    fn from(tag: &str) -> Self {
        End::Device(tag.to_string())
    }
}

#[derive(Debug, Clone)]
enum Step {
    Page(Option<String>),
    Place { symbol: String, tag: Option<String>, at: (f64, f64) },
    Wire(End, End),
    Cable(Value),
    Command(String, Value),
}

/// Builds a project with the commands a user would run. See the module docs.
#[derive(Debug, Clone)]
pub struct ProjectBuilder {
    seed: u64,
    steps: Vec<Step>,
}

/// A built project, and what its steps made.
pub struct Built {
    /// The engine session (the project is a document owner in it).
    pub s: Session,
    /// The project, reachable through its lock boundary only.
    pub project: ProjectHandle,
    /// The pages the builder added with [`ProjectBuilder::page`], in order (the project's first
    /// page is `first_page`).
    pub added_pages: Vec<PageId>,
    /// The project's first page.
    pub first_page: PageId,
    /// Devices placed with a tag, by tag.
    pub devices: BTreeMap<String, DeviceId>,
    /// The wires drawn, in order.
    pub wires: Vec<WireId>,
    /// The cables made with [`ProjectBuilder::cable`], in order.
    pub cables: Vec<CableId>,
}

impl ProjectBuilder {
    /// A project that will be created with `seed` (named "Demo", on one empty page).
    pub fn new(seed: u64) -> Self {
        ProjectBuilder { seed, steps: Vec::new() }
    }

    /// Add a page (`newpage`).
    pub fn page(mut self) -> Self {
        self.steps.push(Step::Page(None));
        self
    }

    /// Add a page with this title.
    pub fn page_titled(mut self, title: &str) -> Self {
        self.steps.push(Step::Page(Some(title.to_string())));
        self
    }

    /// Place a symbol with an explicit tag, e.g. `.place("coil", "-K1", (50.0, 50.0))`.
    pub fn place(mut self, symbol: &str, tag: &str, at: (f64, f64)) -> Self {
        self.steps.push(Step::Place { symbol: symbol.into(), tag: Some(tag.into()), at });
        self
    }

    /// Place a relay coil tagged `tag` at `at`.
    pub fn coil(self, tag: &str, at: (f64, f64)) -> Self {
        self.place("coil", tag, at)
    }

    /// Place a symbol and let the project number it.
    pub fn place_untagged(mut self, symbol: &str, at: (f64, f64)) -> Self {
        self.steps.push(Step::Place { symbol: symbol.into(), tag: None, at });
        self
    }

    /// Draw a wire between two ends: points on the page, or the first connection point of a tagged
    /// device. Both ends are attached when they meet a connection point; a line with an end that
    /// meets none carries no wire (design 072) and adds nothing to [`Built::wires`].
    pub fn wire(mut self, a: impl Into<End>, b: impl Into<End>) -> Self {
        self.steps.push(Step::Wire(a.into(), b.into()));
        self
    }

    /// Make a cable that is not drawn yet (`cablenew`) with these parameters (`{"cores": "4G"}`).
    pub fn cable(mut self, params: Value) -> Self {
        self.steps.push(Step::Cable(params));
        self
    }

    /// Any other command, by its JSON form.
    pub fn command(mut self, name: &str, params: Value) -> Self {
        self.steps.push(Step::Command(name.into(), params));
        self
    }

    /// Run the steps. Panics, naming the step and the command's message, when a command refuses.
    pub fn build(self) -> Built {
        crate::install();
        // Never the developer's own cable-views.json (brief 005 review M4).
        crate::views::isolate_for_tests();
        let mut s = Session::empty();
        let project = Project::create(&mut s, "Demo", self.seed).expect("create the project");
        let doc = read_doc(&project, &mut s);
        let first_page = doc.page_order[0];
        let mut b = Built { s, project, added_pages: Vec::new(), first_page, devices: BTreeMap::new(), wires: Vec::new(), cables: Vec::new() };
        for step in &self.steps {
            b.run_step(step);
        }
        b
    }
}

fn read_doc(project: &ProjectHandle, s: &mut Session) -> Document {
    project.read(s, |p, _| p.ledger.document().clone()).expect("project not busy")
}

fn hex_id<T: From<u128>>(v: &Value, key: &str) -> T {
    T::from(u128::from_str_radix(v[key].as_str().expect("id in the result"), 16).expect("hex id"))
}

impl Built {
    /// A copy of the project's document.
    pub fn doc(&mut self) -> Document {
        read_doc(&self.project, &mut self.s)
    }

    /// The device placed with `tag`.
    pub fn device(&self, tag: &str) -> DeviceId {
        *self.devices.get(tag).unwrap_or_else(|| panic!("no device placed with tag {tag}"))
    }

    fn exec(&mut self, cmd: &str, params: Value) -> Value {
        self.s.execute(cmd, &params).unwrap_or_else(|e| panic!("builder step `{cmd} {params}` was refused: {e}"))
    }

    fn point(&mut self, end: &End) -> Pt {
        match end {
            End::At(x, y) => Pt { x: *x, y: *y },
            End::Device(tag) => {
                let c = self.device(tag);
                let doc = self.doc();
                let (rid, r) =
                    doc.representations_of(Subject::Device(c)).next().unwrap_or_else(|| panic!("device {tag} has no drawn representation"));
                let page = r.page;
                ecad_model::derive::terminal_positions(&doc, page)
                    .into_iter()
                    .find(|(_, r, _)| *r == rid)
                    .map(|(_, _, p)| p)
                    .unwrap_or_else(|| panic!("device {tag} has no connection point"))
            }
        }
    }

    fn run_step(&mut self, step: &Step) {
        match step {
            Step::Page(title) => {
                let params = match title {
                    Some(t) => json!({"title": t}),
                    None => json!({}),
                };
                let v = self.exec("newpage", params);
                self.added_pages.push(hex_id(&v, "page"));
            }
            Step::Place { symbol, tag, at } => {
                let mut params = json!({"symbol": symbol, "at": [at.0, at.1]});
                if let Some(t) = tag {
                    params["tag"] = json!(t);
                }
                let v = self.exec("place", params);
                if let Some(t) = tag {
                    self.devices.insert(t.clone(), hex_id(&v, "component"));
                }
            }
            Step::Wire(a, b) => {
                let (pa, pb) = (self.point(a), self.point(b));
                let v = self.exec("wire", json!({"points": [[pa.x, pa.y], [pb.x, pb.y]]}));
                if v["conductor"].is_string() {
                    self.wires.push(hex_id(&v, "conductor"));
                }
            }
            Step::Cable(params) => {
                let v = self.exec("cablenew", params.clone());
                self.cables.push(hex_id(&v, "cable"));
            }
            Step::Command(name, params) => {
                self.exec(name, params.clone());
            }
        }
    }
}
