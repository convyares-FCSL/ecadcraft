//! The right panel (spec §2): details only, the inspector. It holds the ECAD properties section
//! that was in CADCraft's Properties palette ([`crate::sheets_ui::section`], design 019) and,
//! under it, the defaults that apply to what is selected (brief 042 step 4): each value from the
//! registry with a badge for where it comes from (built-in, standard, project, page, object,
//! symbol). A badge opens Settings ▸ Standards at that key. A value set at this object's level is
//! a field (a list for a choice), with Reset when it is overridden here; a value read from
//! further out is grey text that goes to where it is set. It collapses to a 44 px rail. The Text
//! panel opens over it ([`super::text_panel`]).
//!
//! **Update map** (the inspector's rows): rebuilt on a new selection or page, or after a change
//! that touched the selected object, the page or the project (`props_changes_since`); every other
//! commit leaves them.

use std::collections::{BTreeMap, BTreeSet};

use ecad_cadcraft::default_cmds::{self, Target};
use ecad_cadcraft::defaults::{self, Entry, Kind, Level, Scope, Source};
use ecad_cadcraft::project::Mapped;
use ecad_cadcraft::text_style::TextKind;
use ecad_model::{ObjectRef, PageId, RepresentationId, Subject};
use ecad_ui::tokens::{Icon, size, space};
use eframe::egui::{self, Rect, RichText, pos2, vec2};

use super::Slots;
use crate::Trial;

/// Accessible labels.
pub const COLLAPSE: &str = "Collapse the details panel";
pub const EXPAND: &str = "Details";
pub const TITLE: &str = "PROPERTIES";

/// Draw the right panel (or its rail, or the Text panel over it).
pub fn show(t: &mut Trial, ui: &mut egui::Ui, slots: &Slots) {
    let s = ecad_ui::SHELL;
    let rect = slots.right;
    super::fill(ui, rect, s.panel);
    ui.painter().vline(rect.left() + 0.5, rect.y_range(), egui::Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, ecad_ui::color(s.line)));
    if t.shell.state.text_panel && !slots.right_rail {
        super::text_panel::show(t, ui, rect);
        return;
    }
    if slots.right_rail {
        let expand = super::in_rect(ui, rect.shrink(space::S), "ecad.shell.right.rail", |ui| {
            ui.vertical_centered(|ui| ui.add(ecad_ui::prim::IconButton::new(Icon::ChevronLeft, EXPAND)).clicked()).inner
        });
        if expand {
            t.shell.state.right_collapsed = false;
        }
        return;
    }
    let head = Rect::from_min_size(rect.min, vec2(rect.width(), size::PANEL_HEADER));
    let collapse = super::in_rect(ui, head.shrink2(vec2(space::L, space::XS)), "ecad.shell.right.head", |ui| {
        ui.horizontal_centered(|ui| {
            ecad_ui::prim::read_value(ui, TITLE);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(ecad_ui::prim::IconButton::new(Icon::Collapse, COLLAPSE)).clicked()
            })
            .inner
        })
        .inner
    });
    if collapse {
        t.shell.state.right_collapsed = true;
    }
    let body = Rect::from_min_max(pos2(rect.left() + space::L, head.bottom()), pos2(rect.right() - space::M, rect.bottom()));
    sync(t);
    let writable = t.project_open() && !t.header().read_only;
    let section = &mut t.shell.right_section;
    let inspector = &mut t.settings.inspector;
    let acts = super::in_rect(ui, body, "ecad.shell.right.body", |ui| {
        egui::ScrollArea::vertical()
            .id_salt("ecad.shell.right.scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                section(ui);
                inspector.show(ui, writable)
            })
            .inner
    });
    for a in acts {
        t.inspector_action(a);
    }
}

/// Accessible names of the inspector's rows: `ecad.inspector.<key>` (the value),
/// `ecad.origin.<key>` (its badge), `ecad.inspector.reset.<key>`.
pub const DEFAULTS_TITLE: &str = "ecad.inspector";
pub const PLACEMENTS: &str = "ecad.inspector.placements";

/// What the inspector shows defaults for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum What {
    Page(PageId),
    Symbol(ecad_model::ComponentId, RepresentationId),
    Conductor(ecad_model::ConductorId),
}

/// One default as the inspector shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub key: &'static str,
    pub label: String,
    pub value: String,
    pub from: Source,
    /// The level and object this row sets, when the value is set here.
    pub here: Option<(Level, Target)>,
    /// The words a choice entry takes.
    pub choices: &'static [&'static str],
}

impl Row {
    /// Overridden at this object's level: Reset is offered.
    pub fn overridden_here(&self) -> bool {
        match self.here {
            Some((Level::Page, _)) => self.from == Source::Page,
            Some((Level::Object, Target::Symbol(_))) => self.from == Source::Symbol,
            Some((Level::Object, _)) => self.from == Source::Object,
            _ => false,
        }
    }
}

/// What the inspector asks the trial for.
#[derive(Debug, Clone, PartialEq)]
pub enum InspectorAction {
    /// Open Settings ▸ Standards at this key.
    Standards(&'static str),
    /// Run this `setdefault` line (the row's key, for its refusal).
    Run(&'static str, String),
    /// Select every placement of the device.
    Placements(Vec<RepresentationId>),
}

/// The inspector's rows and what they were built from.
#[derive(Debug, Default)]
pub struct Inspector {
    pub rows: Vec<Row>,
    what: Option<What>,
    deps: BTreeSet<ObjectRef>,
    /// (project serial, change count, page, selection).
    seen: Option<(u64, u64, Option<PageId>, Vec<cadcraft_engine::doc::Handle>)>,
    /// The device's placements (its symbols), for "Select all placements".
    placements: Vec<RepresentationId>,
    /// The values being typed, by key.
    edits: BTreeMap<&'static str, String>,
    /// A refusal per key.
    pub errors: BTreeMap<&'static str, String>,
    /// Full builds of the rows (tests: the update map).
    pub rebuilds: u64,
}

/// The kinds of a symbol's own texts.
const SYMBOL_KINDS: [TextKind; 4] = [TextKind::Tag, TextKind::PointLabel, TextKind::Description, TextKind::PartNumber];

/// A value as the inspector shows it: a show word in words (spec §3), else as the Standards table.
fn shown(e: &Entry, r: &defaults::Resolved<'_>) -> String {
    if e.key.ends_with(".show") {
        return match r.word() {
            "off" => "Hidden".into(),
            "first" => "Shown on the first of each run".into(),
            _ => "Shown on the page".into(),
        };
    }
    crate::lists::standards::value_text(e, r)
}

type Built = (Vec<Row>, BTreeSet<ObjectRef>, Vec<RepresentationId>);

fn rows_for(doc: &ecad_model::Document, what: What) -> Built {
    let mut deps = BTreeSet::from([ObjectRef::Project]);
    let (scope, entries, here): (Scope, Vec<&'static Entry>, Option<(Level, Target)>) = match what {
        What::Page(pg) => {
            deps.insert(ObjectRef::Page(pg));
            (Scope::page(pg), defaults::all().filter(|e| e.allows(Level::Page)).collect(), Some((Level::Page, Target::Page(pg))))
        }
        What::Symbol(c, rep) => {
            deps.insert(ObjectRef::Component(c));
            let mut es: Vec<&'static Entry> = Vec::new();
            for k in SYMBOL_KINDS {
                es.push(k.height_entry());
                es.push(k.weight_entry());
                es.extend(k.show_entry());
            }
            let starter = doc
                .components
                .get(&c)
                .and_then(|c| c.definition)
                .and_then(|d| doc.definitions.get(&d))
                .and_then(ecad_cadcraft::symbols::of_definition);
            es.extend(starter.and_then(|s| defaults::symbols::prefix_entry(s.key)));
            (Scope::symbol(doc, rep), es, Some((Level::Object, Target::Symbol(rep))))
        }
        What::Conductor(w) => {
            deps.insert(ObjectRef::Conductor(w));
            let es = defaults::all()
                .filter(|e| e.key == defaults::text::WIRE_LABEL_HEIGHT.key || e.group == "Wire labels" || e.key.starts_with("wire."))
                .collect();
            (Scope { conductor: Some(w), ..Scope::project() }, es, Some((Level::Object, Target::Conductor(w))))
        }
    };
    let rows = entries
        .into_iter()
        .map(|e| {
            let r = defaults::resolve(doc, e, scope);
            Row {
                key: e.key,
                label: e.label.to_string(),
                value: shown(e, &r),
                from: r.from,
                here: here.filter(|(l, _)| e.allows(*l)),
                choices: match e.kind {
                    Kind::Choice(w) => w,
                    _ => &[],
                },
            }
        })
        .collect();
    let placements = match what {
        What::Symbol(c, _) => doc.representations_of(Subject::Component(c)).map(|(id, _)| id).collect(),
        _ => Vec::new(),
    };
    (rows, deps, placements)
}

/// Bring the inspector's rows up to date (its update map).
fn sync(t: &mut Trial) {
    let Some(p) = t.project.clone().filter(|p| p.is_attached(&t.cad.session)) else {
        let i = &mut t.settings.inspector;
        (i.rows, i.what, i.seen) = (Vec::new(), None, None);
        return;
    };
    let owned = t.cad.session.state().is_ok_and(|st| t.cad.session.ext.owner_of(st.uid).is_some());
    let selection = if owned { t.cad.session.selection() } else { Vec::new() };
    let serial = t.project_serial;
    let i = &mut t.settings.inspector;
    let seen = i.seen.clone();
    let deps = i.deps.clone();
    let built = p.read(&mut t.cad.session, |p, _| {
        let page = p.active_page();
        let count = p.change_count();
        let same = seen.as_ref().is_some_and(|(s, _, pg, sel)| *s == serial && *pg == page && *sel == selection);
        let stale = !same
            || match seen.as_ref().map(|s| s.1) {
                Some(n) if n == count => false,
                Some(n) => p.props_changes_since(n).is_none_or(|t| t.iter().any(|o| deps.contains(o))),
                None => true,
            };
        if !stale {
            return (count, None);
        }
        let doc = p.ledger.document();
        let what = page.and_then(|pg| what_of(p, pg, &selection));
        (count, Some((page, what, what.map(|w| rows_for(doc, w)))))
    });
    let Ok((count, fresh)) = built else { return };
    match fresh {
        None => {
            if let Some(s) = &mut i.seen {
                s.1 = count;
            }
        }
        Some((page, what, rows)) => {
            if what != i.what {
                i.edits.clear();
                i.errors.clear();
            }
            i.what = what;
            let (rows, deps, placements) = rows.unwrap_or_default();
            (i.rows, i.deps, i.placements) = (rows, deps, placements);
            i.rebuilds += 1;
            i.seen = Some((serial, count, page, selection));
        }
    }
}

/// What the selection on `page` is, for the inspector: nothing selected is the page; one symbol
/// (or its tag text) is that symbol; one wire is that wire; anything else shows no defaults.
fn what_of(p: &ecad_cadcraft::Project, page: PageId, selection: &[cadcraft_engine::doc::Handle]) -> Option<What> {
    if selection.is_empty() {
        return Some(What::Page(page));
    }
    let doc = p.ledger.document();
    let map = &p.pages.get(&page)?.map;
    let mut found: Option<What> = None;
    for h in selection.iter().take(256) {
        let w = match map.get(*h)? {
            Mapped::Item(item) => match doc.representation_of_item(item) {
                Some((rid, r)) => match r.subject {
                    Subject::Component(c) => What::Symbol(c, rid),
                    Subject::Conductor(w) => What::Conductor(w),
                    Subject::Cable(_) => return None,
                },
                None => return None,
            },
            Mapped::Tag(r) | Mapped::Marker(r, _) | Mapped::Xref(r) => match doc.representations.get(&r).map(|x| x.subject) {
                Some(Subject::Component(c)) => What::Symbol(c, r),
                _ => return None,
            },
            Mapped::Label(w) => What::Conductor(w),
            _ => return None,
        };
        match found {
            None => found = Some(w),
            Some(f) if f == w => {}
            Some(_) => return None,
        }
    }
    found
}

impl Inspector {
    /// Draw the rows under the properties section. Returns what was asked for.
    pub fn show(&mut self, ui: &mut egui::Ui, writable: bool) -> Vec<InspectorAction> {
        let mut out = Vec::new();
        if self.rows.is_empty() {
            return out;
        }
        ui.add_space(space::M);
        ecad_ui::prim::read_value(ui, "DEFAULTS").widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, DEFAULTS_TITLE));
        let weak = ui.visuals().weak_text_color();
        for row in &self.rows {
            ui.add_space(space::XS);
            // Line one: the label, then on the right the origin badge and Reset (two lines fit
            // the panel's width; one line did not).
            ui.horizontal(|ui| {
                ui.label(RichText::new(&row.label).small().color(weak));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if writable
                        && row.overridden_here()
                        && let Some((level, target)) = row.here
                    {
                        let r = ui.small_button("Reset");
                        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("ecad.inspector.reset.{}", row.key)));
                        if r.clicked() {
                            out.push(InspectorAction::Run(row.key, default_cmds::line(row.key, level, target, None)));
                        }
                    }
                    let badge =
                        ecad_ui::prim::badge(ui, crate::lists::standards::source_word(row.from), crate::lists::standards::source_tone(row.from));
                    let b = ui.interact(badge.rect, egui::Id::unique(("ecad.origin", row.key)), egui::Sense::click());
                    b.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("ecad.origin.{}", row.key)));
                    if b.on_hover_text("Where this value comes from: open Standards").clicked() {
                        out.push(InspectorAction::Standards(row.key));
                    }
                });
            });
            // Line two: the value, set here (a field or a list) or read (grey, goes to Standards).
            let name = format!("ecad.inspector.{}", row.key);
            let width = ui.available_width();
            match row.here.filter(|_| writable) {
                Some((level, target)) if !row.choices.is_empty() => {
                    let mut pick = None;
                    let combo = egui::ComboBox::from_id_salt(&name).selected_text(&row.value).width(width.min(240.0)).show_ui(ui, |ui| {
                        for w in row.choices {
                            if ui.selectable_label(false, *w).clicked() {
                                pick = Some(*w);
                            }
                        }
                    });
                    combo.response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, &name));
                    if let Some(w) = pick {
                        out.push(InspectorAction::Run(row.key, default_cmds::line(row.key, level, target, Some(w))));
                    }
                }
                Some((level, target)) => {
                    let buf = self.edits.entry(row.key).or_insert_with(|| row.value.trim_end_matches(" mm").to_string());
                    let r = ui.add(egui::TextEdit::singleline(buf).desired_width(width.min(240.0)));
                    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, &name));
                    if r.lost_focus() && buf.trim() != row.value.trim_end_matches(" mm") {
                        out.push(InspectorAction::Run(row.key, default_cmds::line(row.key, level, target, Some(buf.trim()))));
                    }
                }
                None => {
                    let r = ui.add(egui::Label::new(RichText::new(&row.value).color(weak)).sense(egui::Sense::click()));
                    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Link, true, &name));
                    if r.on_hover_text("Set in Settings ▸ Standards").clicked() {
                        out.push(InspectorAction::Standards(row.key));
                    }
                }
            }
            if let Some(e) = self.errors.get(row.key) {
                ui.label(RichText::new(e).small().color(ui.visuals().error_fg_color));
            }
        }
        if self.placements.len() > 1 {
            let n = self.placements.len();
            let r = ui.small_button(format!("Select all {n} placements"));
            r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, PLACEMENTS));
            if r.clicked() {
                out.push(InspectorAction::Placements(self.placements.clone()));
            }
        }
        out
    }
}

impl Trial {
    /// Act on what the inspector asked for.
    pub fn inspector_action(&mut self, a: InspectorAction) {
        match a {
            InspectorAction::Standards(key) => self.open_standards(Some(key)),
            InspectorAction::Run(key, line) => {
                let r = self.run_line(&line);
                let i = &mut self.settings.inspector;
                i.edits.remove(key);
                match r {
                    Some(e) => {
                        i.errors.insert(key, e);
                    }
                    None => {
                        i.errors.remove(key);
                    }
                }
            }
            InspectorAction::Placements(reps) => {
                if let Err(e) = self.goto_symbols(&reps) {
                    self.cad.session.echo(e);
                }
            }
        }
    }
}
