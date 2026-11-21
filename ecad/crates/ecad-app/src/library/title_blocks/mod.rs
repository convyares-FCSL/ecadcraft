//! Library ▸ Title blocks (brief 055, design 055 §4, §5): the title block and frame editor.
//!
//! - **Left**: the project's frames (built-ins first, then the standard's, then the project's), a
//!   thumbnail and a grey line each; **+ Title block** and **Import DXF**; the field palette.
//! - **Toolbar**: the Library tabs, Select, Line, Text, Field, Logo, Sample, Snap and Frame….
//! - **Main view**: the block on paper at print size with 20 mm of the frame around it; the
//!   footer says what Save reaches.
//! - **Right panel**: the selection's Value, Look and Box; with nothing selected, the block.
//!
//! The editor works on a **draft** ([`draft`]): nothing is written while editing; **Save** writes
//! one transaction (`titleblock {id, …, body}`) after the consequence line. Programmatic calls
//! never open it.
//!
//! # Update map
//! The list (and the open draft's base) is read again only when the project's head or page count
//! moved; the sample values when the draft's bindings or the page shown change. Editing the draft
//! reads nothing from the project.

pub mod canvas;
pub mod draft;
pub mod frame_popover;
pub mod inspector;
pub mod list;

use std::collections::BTreeMap;
use std::sync::Arc;

use ecad_electrical::cmd::title_block::{layout_of, pages_using};
use ecad_engine::frame::{FrameInput, FrameText};
use ecad_engine::styles::{self, Layer};
use ecad_model::book::Book;
use ecad_model::sheet;
use ecad_model::title_block::{Binding, BlockItem, TitleBlockLayout};
use ecad_model::{PageId, StyleRef};
use eframe::egui::{self, Rect};
use serde_json::{Value, json};

pub use draft::{Draft, Drafts, State};

use crate::App;

/// Element names (design 055 §5).
pub const LIST: &str = "library.tb.list";
pub const CANVAS: &str = "library.tb.canvas";
pub const SAMPLE: &str = "library.tb.sample";
pub const SNAP: &str = "library.tb.snap";
pub const FRAME: &str = "library.tb.frame";
pub const USED_BY: &str = "library.tb.usedby";
pub const RELOAD: &str = "library.tb.banner.reload";
pub const KEEP: &str = "library.tb.banner.keep";
/// Words (design 055 §4).
pub const FOOTER: &str = "Changes reach every page that uses this block. Fields fill themselves; nothing is typed on a page.";
pub const CAPTION: &str = "Zones and frame come from Frame…";
pub const EMPTY_HINT: &str = "Drag fields from the left, or draw lines";
pub const DROP_ON_BLOCK: &str = "Drop it on the block.";
pub const BUILTIN_NOTE: &str = "Built-in. Duplicate it to make your own.";
pub const WRITES_OFF: &str = "Writes are off for this project.";

/// The editor's tools (design 055 §4.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tool {
    #[default]
    Select,
    Line,
    Text,
    Field,
    Logo,
}

impl Tool {
    pub const ALL: [Tool; 5] = [Tool::Select, Tool::Line, Tool::Text, Tool::Field, Tool::Logo];

    pub fn label(self) -> &'static str {
        match self {
            Tool::Select => "Select",
            Tool::Line => "Line",
            Tool::Text => "Text",
            Tool::Field => "Field",
            Tool::Logo => "Logo",
        }
    }

    pub fn key(self) -> egui::Key {
        match self {
            Tool::Select => egui::Key::V,
            Tool::Line => egui::Key::L,
            Tool::Text => egui::Key::T,
            Tool::Field => egui::Key::F,
            Tool::Logo => egui::Key::G,
        }
    }

    /// Its element name (`library.tb.tool.select`).
    pub fn name(self) -> String {
        format!("library.tb.tool.{}", self.label().to_lowercase())
    }
}

/// One row of the list.
#[derive(Debug, Clone)]
pub struct Row {
    pub id: String,
    pub name: String,
    pub layer: Layer,
    /// `A3 landscape · zones · used by 10 pages`.
    pub line: String,
    pub pages: usize,
    pub layout: Arc<TitleBlockLayout>,
    pub state: State,
}

/// What the sample page shows (design 055 §4.5, Sample).
#[derive(Debug, Clone)]
pub struct Sample {
    pub input: FrameInput,
    pub props: BTreeMap<String, String>,
    pub newest: BTreeMap<String, String>,
    /// `on page 2`.
    pub page: String,
}

impl Sample {
    /// The value binding `b` shows on the sample page.
    pub fn value(&self, b: &Binding) -> String {
        use ecad_model::project::RevField;
        use ecad_model::title_block::RevisionColumn;
        match b {
            Binding::Project(k) => self.props.get(k.as_str()).cloned().unwrap_or_default(),
            Binding::Revision(RevisionColumn::Field(RevField::Rev)) => ecad_engine::title_block::bind::value(&self.input, b),
            Binding::Revision(c) => self.newest.get(c.word()).cloned().unwrap_or_default(),
            Binding::Page(_) | Binding::Print(_) => ecad_engine::title_block::bind::value(&self.input, b),
        }
    }
}

/// An open DXF prompt (Import DXF, or the Logo tool's box).
#[derive(Debug, Clone, Default)]
pub struct DxfAsk {
    pub path: String,
    pub sheet: String,
    /// The Logo tool's box (block mm), else a whole-block import.
    pub logo_at: Option<ecad_model::title_block::Rect>,
    pub error: Option<String>,
}

/// The editor's state.
#[derive(Debug)]
pub struct Editor {
    /// The frame shown.
    pub open: Option<String>,
    pub draft: Option<Draft>,
    pub drafts: Drafts,
    stored: String,
    pub tool: Tool,
    pub sample: bool,
    pub snap: bool,
    pub rows: Vec<Row>,
    /// The head and page count the rows were read at.
    rows_head: Option<(u64, usize)>,
    pub sample_data: Option<Sample>,
    pub frame: Option<frame_popover::FrameEdit>,
    pub canvas: canvas::CanvasState,
    /// A word for the person (a refusal).
    pub notice: Option<String>,
    pub ask_discard: bool,
    /// Another writer saved the open frame (design 055 §4.8).
    pub meanwhile: bool,
    pub new_menu: bool,
    pub dxf: Option<DxfAsk>,
    /// Project field…: the key being typed.
    pub project_key: Option<String>,
    /// A Checks row's item to select once its frame opens.
    pub pending_select: Option<usize>,
}

impl Default for Editor {
    fn default() -> Self {
        Editor {
            open: None,
            draft: None,
            drafts: Drafts::default(),
            stored: String::new(),
            tool: Tool::Select,
            sample: true,
            snap: true,
            rows: Vec::new(),
            rows_head: None,
            sample_data: None,
            frame: None,
            canvas: canvas::CanvasState::default(),
            notice: None,
            ask_discard: false,
            meanwhile: false,
            new_menu: false,
            dxf: None,
            project_key: None,
            pending_select: None,
        }
    }
}

impl Editor {
    /// Take the stored drafts.
    pub fn load(&mut self, stored: Option<&str>) {
        self.drafts = Drafts::from_stored(stored);
        self.stored = self.drafts.to_stored();
    }

    /// The drafts to store when they changed since the last call.
    pub fn to_save(&mut self) -> Option<String> {
        let s = self.drafts.to_stored();
        (s != self.stored).then(|| {
            self.stored = s.clone();
            s
        })
    }

    /// The row of the open frame.
    pub fn open_row(&self) -> Option<&Row> {
        let id = self.open.as_deref()?;
        self.rows.iter().find(|r| r.id == id)
    }
}

/// `A3 landscape · zones · used by 10 pages`.
fn row_line(sheet_text: &str, pages: usize) -> String {
    let used = match pages {
        0 => "not used yet".to_string(),
        1 => "used by 1 page".to_string(),
        n => format!("used by {n} pages"),
    };
    format!("{sheet_text} · zones · {used}")
}

/// The frame's fields as the draft keeps them (every catalogue field, resolved).
fn fields_of(doc: &ecad_model::Document, id: &StyleRef) -> BTreeMap<String, String> {
    let cat = &ecad_engine::styles::frame::CATALOGUE;
    cat.fields.iter().map(|f| (f.key.to_string(), styles::field(doc, cat, id, f, None).text())).collect()
}

/// Read the list from `doc` (design 055 §4.1).
pub fn read_rows(doc: &ecad_model::Document) -> Vec<Row> {
    let cat = &ecad_engine::styles::frame::CATALOGUE;
    let text = FrameText::of(doc);
    styles::rows(doc, cat)
        .into_iter()
        .map(|r| {
            let spec = sheet::frame_spec(doc, &r.id);
            let shown = spec.unwrap_or(doc.settings.sheet());
            let layout = layout_of(doc, &r.id, &shown, &text);
            let pages = pages_using(doc, &r.id).len();
            let sheet_text = match spec {
                Some(s) => format!("{} landscape", ecad_engine::title_block::sheet_name(&s)),
                None => "Any sheet".to_string(),
            };
            let state = State { name: r.name.clone(), fields: fields_of(doc, &r.id), layout: layout.clone() };
            Row { id: r.id.text(), name: r.name, layer: r.layer, line: row_line(&sheet_text, pages), pages, layout: Arc::new(layout), state }
        })
        .collect()
}

/// The sample page: the page open in Draw, else the first page.
fn sample_of(doc: &ecad_model::Document, active: Option<PageId>, layout: &TitleBlockLayout) -> Option<Sample> {
    let page = active.filter(|p| doc.pages.contains_key(p)).or_else(|| doc.page_order.first().copied())?;
    let book = Book::of(doc);
    let mut input = FrameInput::of(doc, page, &book);
    input.block.layout = Arc::new(layout.clone());
    let issued = ecad_engine::revisions::issued(doc).into_iter().next();
    let newest = ecad_engine::revisions::FIELDS
        .iter()
        .map(|f| f.to_string())
        .chain(ecad_engine::revisions::columns(doc).into_iter().map(|c| c.field))
        .map(|f| {
            let v = issued.as_ref().map(|r| r.get(&f).to_string()).unwrap_or_default();
            (f, v)
        })
        .collect();
    Some(Sample { page: format!("on page {}", book.number(doc, page)), props: doc.properties.clone(), newest, input })
}

impl App {
    /// Open Library ▸ Title blocks; `at`: a Checks row's page and item, selected in its frame.
    pub fn open_title_blocks(&mut self, at: Option<(PageId, u16)>) {
        self.library.browser.tab = super::browser::LibraryTab::TitleBlocks;
        self.set_workspace(crate::shell::Workspace::Library);
        if let Some((page, item)) = at {
            let id =
                self.project.clone().and_then(|p| p.read(&mut self.cad.session, |p, _| sheet::frame_style(p.ledger.document(), page).text()).ok());
            if let Some(id) = id {
                self.tb_refresh();
                self.tb_open(&id);
                self.library.tb.pending_select = Some(usize::from(item));
            }
        }
    }

    /// Read the list again when the head moved; keep the open draft's base current.
    pub fn tb_refresh(&mut self) {
        let Some(p) = self.project.clone().filter(|p| p.is_attached(&self.cad.session)) else {
            if self.library.tb.rows.is_empty() {
                self.library.tb.rows = read_rows(&ecad_model::Document::default());
            }
            return;
        };
        let active = self.panel.active;
        let head = p.read(&mut self.cad.session, |p, _| (p.ledger.head(), p.ledger.document().page_order.len())).ok();
        if head.is_none() || head == self.library.tb.rows_head {
            return;
        }
        let layout = self.library.tb.draft.as_ref().map(|d| d.now.layout.clone());
        let read = p.read(&mut self.cad.session, |p, _| {
            let doc = p.ledger.document();
            (read_rows(doc), layout.as_ref().and_then(|l| sample_of(doc, active, l)))
        });
        let Ok((rows, sample)) = read else { return };
        let ed = &mut self.library.tb;
        ed.rows = rows;
        ed.rows_head = head;
        if sample.is_some() {
            ed.sample_data = sample;
        }
        // Changed meanwhile: the project's record moved under the draft.
        if let (Some(d), Some(id)) = (ed.draft.as_mut(), ed.open.clone()) {
            match ed.rows.iter().find(|r| r.id == id) {
                Some(r) if r.state != d.base => {
                    if d.changed() {
                        ed.meanwhile = true;
                    } else {
                        let sel = std::mem::take(&mut d.selected);
                        *d = Draft::new(&id, r.layer == Layer::Builtin, r.state.clone());
                        d.selected = sel.into_iter().filter(|i| *i < d.now.layout.items.len()).collect();
                    }
                }
                Some(_) => {}
                None => {
                    ed.draft = None;
                    ed.open = None;
                }
            }
        }
    }

    /// The sample values again (the draft's bindings changed, or the page shown did).
    pub(crate) fn tb_sample(&mut self) {
        let Some(layout) = self.library.tb.draft.as_ref().map(|d| d.now.layout.clone()) else { return };
        let active = self.panel.active;
        if let Some(p) = self.project.clone().filter(|p| p.is_attached(&self.cad.session)) {
            self.library.tb.sample_data = p.read(&mut self.cad.session, |p, _| sample_of(p.ledger.document(), active, &layout)).ok().flatten();
        }
    }

    /// Open frame `id` in the editor: its kept draft, else as the project has it. The draft being
    /// left is kept (the list marks it with a dot).
    pub fn tb_open(&mut self, id: &str) {
        self.tb_keep();
        let key = self.drafts_key().unwrap_or_default();
        let ed = &mut self.library.tb;
        let Some(row) = ed.rows.iter().find(|r| r.id == id).cloned() else { return };
        let builtin = row.layer == Layer::Builtin;
        let mut d = match ed.drafts.get(&key, id) {
            Some(kept) if kept.base == row.state => kept.clone(),
            _ => Draft::new(id, builtin, row.state.clone()),
        };
        d.selected.clear();
        ed.draft = Some(d);
        ed.open = Some(id.to_string());
        ed.meanwhile = false;
        ed.ask_discard = false;
        ed.frame = None;
        ed.notice = None;
        if builtin {
            ed.tool = Tool::Select;
        }
        self.tb_sample();
    }

    /// Keep the open draft in storage (only when it changed).
    pub(crate) fn tb_keep(&mut self) {
        let Some(key) = self.drafts_key() else { return };
        if let Some(d) = self.library.tb.draft.clone() {
            self.library.tb.drafts.keep(&key, &d);
        }
    }

    /// Save (design 055 §4.2): one transaction with the draft's name, changed fields and block.
    pub(crate) fn tb_save(&mut self) {
        let Some(d) = self.library.tb.draft.clone() else { return };
        if d.builtin {
            self.library.tb.notice = Some(BUILTIN_NOTE.into());
            return;
        }
        let mut params = json!({"id": d.id, "body": serde_json::to_value(&d.now.layout).unwrap_or(Value::Null)});
        if d.now.name != d.base.name {
            params["name"] = json!(d.now.name);
        }
        let fields: serde_json::Map<String, Value> = d
            .now
            .fields
            .iter()
            .filter(|(k, v)| d.base.fields.get(*k) != Some(*v))
            .map(|(k, v)| (k.clone(), if v.trim().is_empty() { Value::Null } else { json!(v) }))
            .collect();
        if !fields.is_empty() {
            params["fields"] = Value::Object(fields);
        }
        let r = self.cad.run("titleblock", params);
        self.after_logic();
        match r {
            Ok(_) => {
                let key = self.drafts_key().unwrap_or_default();
                self.library.tb.drafts.forget(&key, &d.id);
                if let Some(x) = self.library.tb.draft.as_mut() {
                    x.base = x.now.clone();
                }
                self.library.tb.meanwhile = false;
                self.library.tb.notice = None;
                self.tb_refresh();
            }
            Err(e) => self.library.tb.notice = Some(e),
        }
    }

    /// Discard the draft: back to the project's frame.
    pub(crate) fn tb_discard(&mut self) {
        let key = self.drafts_key().unwrap_or_default();
        let ed = &mut self.library.tb;
        ed.ask_discard = false;
        ed.meanwhile = false;
        let Some(id) = ed.open.clone() else { return };
        ed.drafts.forget(&key, &id);
        if let Some(r) = ed.rows.iter().find(|r| r.id == id).cloned() {
            ed.draft = Some(Draft::new(&id, r.layer == Layer::Builtin, r.state));
        }
        self.tb_sample();
    }

    /// Run a writing command from the list (new, import, duplicate, promote, delete, use); a new
    /// frame it returns opens.
    pub(crate) fn tb_run(&mut self, command: &str, params: Value) {
        let r = self.cad.run(command, params);
        self.after_logic();
        match r {
            Ok(v) => {
                self.library.tb.notice = None;
                self.tb_refresh();
                if let Some(id) = v.get("id").and_then(Value::as_str).filter(|_| command == "titleblock" || command == "titleblockimport") {
                    let id = id.to_string();
                    self.tb_open(&id);
                }
            }
            Err(e) => self.library.tb.notice = Some(e),
        }
    }

    /// The title blocks' main view (design 055 §4.1).
    pub(crate) fn title_blocks_view(&mut self, ui: &mut egui::Ui, main: Rect) {
        self.tb_refresh();
        if self.library.tb.open.is_none()
            && let Some(id) = self.library.tb.rows.first().map(|r| r.id.clone())
        {
            self.tb_open(&id);
        }
        if let Some(i) = self.library.tb.pending_select.take()
            && let Some(d) = self.library.tb.draft.as_mut()
            && i < d.now.layout.items.len()
        {
            d.selected = [i].into();
        }
        let writable = self.project_open() && self.writable();
        let sheet = self.shell.state.sheet.tokens();
        let key = self.drafts_key().unwrap_or_default();
        let acts = crate::shell::in_rect(ui, main, CANVAS, |ui| {
            let mut acts = list::head(ui, main, &mut self.library.browser, &self.library.listing, &mut self.library.tb);
            acts.extend(list::show(ui, main, &mut self.library.tb, &key, writable));
            acts.extend(canvas::show(ui, main, &mut self.library.tb, sheet, writable));
            frame_popover::show(ui.ctx(), &mut self.library.tb);
            acts
        });
        self.tb_acts(acts);
        self.tb_keep();
    }

    /// The title blocks' right panel (design 055 §4.4).
    pub(crate) fn title_blocks_right(&mut self, ui: &mut egui::Ui) {
        let writable = self.project_open() && self.writable();
        let acts = inspector::show(ui, &mut self.library.tb, writable);
        self.tb_acts(acts);
    }

    fn tb_acts(&mut self, acts: Vec<Act>) {
        for a in acts {
            match a {
                Act::Open(id) => self.tb_open(&id),
                Act::Save => self.tb_save(),
                Act::Discard => self.tb_discard(),
                Act::Resample => self.tb_sample(),
                Act::Run(c, p) => self.tb_run(&c, p),
                Act::Logo { at, path } => self.tb_logo(at, &path),
            }
        }
    }

    /// The Logo tool (design 055 §4.5, §6): a DXF into the selected logo, or a new logo at `at`;
    /// the draft only (Save writes it).
    fn tb_logo(&mut self, at: Option<ecad_model::title_block::Rect>, path: &str) {
        let r = std::fs::read(path).map_err(|e| format!("Cannot read {path}: {e}.")).and_then(|b| ecad_engine::title_block::dxf::logo(&b));
        let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let ed = &mut self.library.tb;
        let prims = match r {
            Ok(p) => p,
            Err(e) => {
                if let Some(x) = ed.dxf.as_mut() {
                    x.error = Some(e);
                }
                return;
            }
        };
        let Some(d) = ed.draft.as_mut() else { return };
        let replace = d.selected.iter().next().copied().filter(|i| matches!(d.now.layout.items.get(*i), Some(BlockItem::Logo { .. })));
        match (replace, at) {
            (Some(i), _) => {
                d.step();
                if let Some(BlockItem::Logo { prims: p, name: n, .. }) = d.now.layout.items.get_mut(i) {
                    *p = prims;
                    *n = name;
                }
            }
            (None, Some(at)) => {
                d.add(BlockItem::Logo { at, name, prims });
            }
            (None, None) => {}
        }
        ed.dxf = None;
        ed.tool = Tool::Select;
    }
}

/// What the editor's parts ask of the window.
#[derive(Debug, Clone, PartialEq)]
pub enum Act {
    Open(String),
    Save,
    Discard,
    /// The draft's bindings changed: read the sample values again.
    Resample,
    /// A writing command (`titleblock {new}`, `titleblockimport`, `promote`, `deletestyle`,
    /// `setdefault`).
    Run(String, Value),
    /// A DXF logo into the selected logo, or a new one at `at`.
    Logo {
        at: Option<ecad_model::title_block::Rect>,
        path: String,
    },
}

/// Whether `id` is a built-in frame.
pub fn is_builtin(id: &str) -> bool {
    StyleRef::parse(id).is_some_and(|r| r.is_builtin())
}
