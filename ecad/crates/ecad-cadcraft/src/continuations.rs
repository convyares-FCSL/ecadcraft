//! Continuation cross-references (brief 049, design 049): every continuation arrow shows where its
//! other ends are, and arrows that go to the same place can share a caption.
//!
//! # Semantics
//! - A **continuation arrow** is 022's tagless component, `kind = continuation`, that joins its net
//!   to every arrow with the same name. Its **other ends** are the other arrows of that name, on the
//!   reference index as one [`Group::Continuation`] (FP4: keyed by the lowest component ID, with
//!   implicit edges).
//! - Each arrow's **reference text** lists its other ends' places in book order, then column, then
//!   row (`102-C-2 103-B-5`), at most `continuation.places.max` of them then `+n`, past the arrow's
//!   tip and reading along the wire ([`parts_on_page`]). The sorted list of a name is worked out
//!   once per derivation, so a group of m arrows costs O(m log m), never O(pages) per arrow.
//! - A **caption** is the component property [`CAPTION_KEY`] (FP1). On each page, captioned arrows
//!   with the same caption and direction whose tips are no more than `continuation.caption.run_gap`
//!   apart form a **run**, drawn under one bracket with the caption ([`runs`]).
//! - Everything drawn here is derived, never stored, and refused as an edit ([`TEXT_DERIVED`]).
//!
//! # Update map (design 049 §8)
//! [`names_named`] says which names a commit's ops touch (arrows created, deleted, renamed,
//! captioned, moved, linked or unlinked, their definitions); `xref::Watch` compares what those
//! names show ([`shown`]) before and after, page by page, so only pages whose texts or runs changed
//! are redrawn. A renumbered or re-sheeted page adds the names with an arrow on it ([`names_on`]);
//! page order changes add those on the pages whose position changed; the reference style, sheet,
//! length unit and this area's keys ([`is_wide_key`]) redraw every page with an arrow. Every other
//! op names nothing.
//!
//! Excludes: commands (`crate::continuation_cmds`), the arrow's name text (`crate::wires`), checks.

use std::collections::{BTreeMap, BTreeSet};

use cadcraft_engine::doc::{HAlign, VAlign};
use ecad_model::connectivity::{CONTINUATION, KIND, NAME, continuation_key, name_key};
use ecad_model::{ComponentId, Document, Form, ObjectRef, Op, PageId, Placement, Pt, Representation, RepresentationId, Subject};

use crate::defaults::{self, Scope};
use crate::text_style::{TextKind, TextTarget, text_shown, text_style};
use crate::xref::{Group, Part, Places, RefIndex, Shape};

/// The component property holding a continuation's caption (FP1).
pub const CAPTION_KEY: &str = "caption";
/// Most characters of a caption.
pub const MAX_CAPTION_CHARS: usize = 80;
/// Most lines a caption wraps to.
pub const MAX_CAPTION_LINES: usize = 3;
/// The narrowest a caption wraps to, millimetres (design 049 §3.2).
pub const MIN_WRAP_MM: f64 = 25.0;
/// How far a bracket runs past the first and last arrow, millimetres.
pub const BRACKET_OVERHANG_MM: f64 = 1.5;
/// The gap between a bracket and its caption text, millimetres.
pub const CAPTION_TEXT_GAP_MM: f64 = 1.0;
/// Hover lines listing other ends before `+N more`.
pub const HOVER_ENDS: usize = 6;
/// Refusal of an edit of an arrow's reference text, bracket or caption (design 049 §10, 11).
pub const TEXT_DERIVED: &str = "The continuation reference is derived; move or rename the continuation instead.";
/// The caption rule, in words (design 049 §3.1).
pub const CAPTION_RULE: &str = "A caption is 1 to 80 characters on one line.";

/// The part index of an arrow's reference text; a run's bracket, ticks and caption lines follow.
const REF_INDEX: u32 = 0;
const BRACKET_INDEX: u32 = 1;
const CAPTION_INDEX: u32 = 4;

// ---------------------------------------------------------------- what an arrow is

/// Whether component `c` is a continuation arrow.
pub fn is_continuation(doc: &Document, c: ComponentId) -> bool {
    doc.components.get(&c).is_some_and(|x| x.properties.get(KIND).map(String::as_str) == Some(CONTINUATION))
}

/// The name key of continuation `c` (`24V`), when it is a named continuation.
pub fn name_of(doc: &Document, c: ComponentId) -> Option<String> {
    continuation_key(&doc.components.get(&c)?.properties)
}

/// The name of continuation `c` as typed.
fn display_name(doc: &Document, c: ComponentId) -> String {
    doc.components.get(&c).and_then(|x| x.properties.get(NAME)).cloned().unwrap_or_default()
}

/// The caption of continuation `c`, when it has one.
pub fn caption_of(doc: &Document, c: ComponentId) -> Option<&str> {
    doc.components.get(&c)?.properties.get(CAPTION_KEY).map(String::as_str).filter(|t| !t.trim().is_empty())
}

/// The group of the arrows named `key` in `doc`: keyed by their lowest component ID.
pub fn group_key(doc: &Document, key: &str) -> Option<Group> {
    doc.continuations_named(key).first().map(|c| Group::Continuation(*c))
}

/// The group of the arrows named `key` and its members: every symbol representation of each, in
/// component then representation order. O(arrows of the name).
pub fn group_named(doc: &Document, key: &str) -> Option<(Group, Vec<RepresentationId>)> {
    let ids = doc.continuations_named(key);
    let g = Group::Continuation(*ids.first()?);
    let members: Vec<RepresentationId> =
        ids.iter().flat_map(|c| doc.representations_of(Subject::Component(*c))).filter(|(_, r)| r.form == Form::Symbol).map(|(id, _)| id).collect();
    (!members.is_empty()).then_some((g, members))
}

/// The group a representation is a member of, when it is a continuation arrow's symbol.
pub fn group_of_rep(doc: &Document, r: &Representation) -> Option<Group> {
    let Subject::Component(c) = r.subject else { return None };
    if r.form != Form::Symbol {
        return None;
    }
    group_key(doc, &name_of(doc, c)?)
}

/// Where an arrow points, snapped to a quarter turn (design 049 §2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Direction {
    Down,
    Up,
    Left,
    Right,
}

impl Direction {
    /// The unit vector along the arrow.
    fn unit(self) -> Pt {
        match self {
            Direction::Down => Pt::new(0.0, -1.0),
            Direction::Up => Pt::new(0.0, 1.0),
            Direction::Left => Pt::new(-1.0, 0.0),
            Direction::Right => Pt::new(1.0, 0.0),
        }
    }

    /// Whether the arrow runs up or down (its text is rotated and its runs are along x).
    fn vertical(self) -> bool {
        matches!(self, Direction::Down | Direction::Up)
    }
}

/// The direction of an arrow placed at `pl`: from its connection point (local (0, 0)) to its tip
/// (local (0, −5)), snapped. `None` for a degenerate placement.
pub fn direction(pl: &Placement) -> Option<Direction> {
    let (o, t) = (pl.apply(Pt::new(0.0, 0.0)), pl.apply(Pt::new(0.0, -5.0)));
    let (dx, dy) = (t.x - o.x, t.y - o.y);
    if !(dx.is_finite() && dy.is_finite()) || (dx == 0.0 && dy == 0.0) {
        return None;
    }
    Some(if dx.abs() > dy.abs() {
        if dx > 0.0 { Direction::Right } else { Direction::Left }
    } else if dy < 0.0 {
        Direction::Down
    } else {
        Direction::Up
    })
}

/// One drawn arrow: its symbol, its component, its page, its tip and its direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Arrow {
    pub rep: RepresentationId,
    pub component: ComponentId,
    pub page: PageId,
    pub tip: Pt,
    pub direction: Direction,
}

/// Arrow `rid` as drawn, when it is a continuation symbol with a valid placement.
pub fn arrow(doc: &Document, rid: RepresentationId) -> Option<Arrow> {
    let r = doc.representations.get(&rid)?;
    let Subject::Component(c) = r.subject else { return None };
    if r.form != Form::Symbol || !is_continuation(doc, c) {
        return None;
    }
    let (_, pl) = crate::devices::rep_definition(doc, r)?;
    if !pl.is_valid() {
        return None;
    }
    let tip = pl.apply(Pt::new(0.0, -5.0));
    tip.is_finite().then_some(())?;
    Some(Arrow { rep: rid, component: c, page: r.page, tip, direction: direction(&pl)? })
}

// ---------------------------------------------------------------- settings (design 049 §7)

/// The `continuation` area's values for a project, resolved once per derivation.
#[derive(Debug, Clone, Copy)]
struct Settings {
    max: usize,
    gap: f64,
    run_gap: f64,
    caption_gap: f64,
    tick: f64,
    overhang: f64,
    text_gap: f64,
    min_wrap: f64,
    ref_height: f64,
    caption_height: f64,
}

impl Settings {
    fn of(doc: &Document) -> Settings {
        use crate::defaults::continuations as c;
        let len = |e: &defaults::Entry| defaults::resolve(doc, e, Scope::project()).length(doc);
        let mm = |x: f64| match doc.settings.length_unit {
            ecad_model::LengthUnit::Millimetre => x,
            ecad_model::LengthUnit::Inch => x / 25.4,
        };
        let max = defaults::resolve(doc, &c::PLACES_MAX, Scope::project()).number();
        Settings {
            max: if max.is_finite() { (max.round() as usize).clamp(1, 10) } else { 3 },
            gap: len(&c::REFERENCE_GAP),
            run_gap: len(&c::CAPTION_RUN_GAP),
            caption_gap: len(&c::CAPTION_GAP),
            tick: len(&c::CAPTION_TICK),
            overhang: mm(BRACKET_OVERHANG_MM),
            text_gap: mm(CAPTION_TEXT_GAP_MM),
            min_wrap: mm(MIN_WRAP_MM),
            ref_height: text_style(doc, TextKind::Reference).height,
            caption_height: text_style(doc, TextKind::ContinuationCaption).height,
        }
    }
}

// ---------------------------------------------------------------- other ends (design 049 §2.2)

/// One other end: its symbol, its sort key (`None` when it cannot be located) and its text.
#[derive(Debug, Clone, PartialEq)]
pub struct End {
    pub rep: RepresentationId,
    pub page: PageId,
    pub key: Option<(usize, u32, u32)>,
    pub text: String,
}

/// The arrows named `key`, sorted (book position, column, row, then ID; unlocated last, shown `?`).
/// O(m log m).
pub fn ends_named(doc: &Document, places: &mut Places<'_>, key: &str) -> Vec<End> {
    let Some((_, members)) = group_named(doc, key) else { return Vec::new() };
    let unlocated = ecad_model::derive::unlocated(doc.settings.reference_style).to_string();
    let mut out: Vec<End> = members
        .into_iter()
        .filter_map(|rid| {
            let r = doc.representations.get(&rid)?;
            let (key, text) = match places.sorted(r.page, crate::xref::anchor(doc, r)) {
                Some((k, t)) => (Some(k), t),
                None => (None, unlocated.clone()),
            };
            Some(End { rep: rid, page: r.page, key, text })
        })
        .collect();
    out.sort_by(|a, b| match (a.key, b.key) {
        (Some(x), Some(y)) => x.cmp(&y).then(a.rep.cmp(&b.rep)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.rep.cmp(&b.rep),
    });
    out
}

/// The text arrow `me` shows from its name's sorted ends: the others, at most `max`, then `+n`.
/// Empty when it has no other end.
/// `member`: `me` is one of `ends` (always so for a drawn arrow), so the others are one fewer.
fn text_for(ends: &[End], me: RepresentationId, member: bool, max: usize) -> String {
    // O(max) per arrow, never O(group): the first `max` others, and how many more there are.
    let mut words: Vec<&str> = ends.iter().filter(|e| e.rep != me).take(max).map(|e| e.text.as_str()).collect();
    let others = ends.len().saturating_sub(usize::from(member));
    let more = others.saturating_sub(words.len());
    let tail = format!("+{more}");
    if more > 0 {
        words.push(&tail);
    }
    words.join(" ")
}

/// The other ends of arrow `rep` (every one, sorted), for hover, Properties and Go to.
pub fn other_ends(doc: &Document, rep: RepresentationId) -> Vec<End> {
    let Some(a) = arrow(doc, rep) else { return Vec::new() };
    let Some(key) = name_of(doc, a.component) else { return Vec::new() };
    let mut places = Places::new(doc);
    ends_named(doc, &mut places, &key).into_iter().filter(|e| e.rep != rep).collect()
}

/// The sorted ends of each name, worked out at most once per derivation.
struct Cache {
    settings: Settings,
    ends: BTreeMap<String, Vec<End>>,
    /// The text styles and fonts, resolved on the first measurement (only caption runs measure).
    look: Option<crate::text_style::Look>,
}

impl Cache {
    fn new(doc: &Document) -> Cache {
        Cache { settings: Settings::of(doc), ends: BTreeMap::new(), look: None }
    }

    /// The width of `value` in `kind`'s style, with the fonts resolved once per derivation.
    fn width(&mut self, doc: &Document, kind: TextKind, value: &str) -> f64 {
        let look = self.look.get_or_insert_with(|| crate::text_style::Look::new(doc));
        crate::text_style::width_in(look.fonts_for(kind), look.style(kind), value)
    }

    fn ends(&mut self, doc: &Document, places: &mut Places<'_>, key: &str) -> &[End] {
        self.ends.entry(key.to_string()).or_insert_with(|| ends_named(doc, places, key))
    }

    /// The reference text arrow `a` shows (empty when lonely).
    fn text(&mut self, doc: &Document, places: &mut Places<'_>, a: &Arrow) -> String {
        let Some(key) = name_of(doc, a.component) else { return String::new() };
        let max = self.settings.max;
        text_for(self.ends(doc, places, &key), a.rep, true, max)
    }
}

// ---------------------------------------------------------------- drawing (design 049 §2.3, §3.2)

/// Arrow `a`'s reference text as a part: past its tip, reading along the wire (§2.3).
fn reference_part(a: &Arrow, value: String, s: &Settings) -> Part {
    let at = Pt::new(a.tip.x + a.direction.unit().x * s.gap, a.tip.y + a.direction.unit().y * s.gap);
    let (halign, rotation) = match a.direction {
        Direction::Down => (HAlign::Right, std::f64::consts::FRAC_PI_2),
        Direction::Up => (HAlign::Left, std::f64::consts::FRAC_PI_2),
        Direction::Right => (HAlign::Left, 0.0),
        Direction::Left => (HAlign::Right, 0.0),
    };
    Part {
        rep: a.rep,
        index: REF_INDEX,
        shape: Shape::Text { at, value, height: s.ref_height, halign, valign: VAlign::Middle, rotation },
        // Ink 2 of the sheet in use ([`colour`]), not a cable colour (review of #85, 3).
        aci: None,
        row: None,
        bar: false,
    }
}

/// Colour a drawn continuation part (review of #85, 3; `ui-design-spec.md`, canvas colours): an
/// arrow's other-end text in Ink 2 of the sheet the page is projected for, as zone labels, pin
/// labels and references are; the bracket and caption stay ByLayer (ink). Other parts unchanged.
pub fn colour(doc: &Document, part: &Part, e: &mut cadcraft_engine::doc::Entity) {
    if part.index == REF_INDEX && arrow(doc, part.rep).is_some() {
        let ink2 = crate::look::projection_sheet().color(ecad_tokens::Meaning::Ink2);
        e.common.color = cadcraft_engine::doc::color::Color::True(crate::look::rgb(ink2));
    }
}

/// The arrows drawn on `page`, in representation order.
fn arrows_on(doc: &Document, page: PageId) -> Vec<Arrow> {
    doc.representations_on(page).filter_map(|(rid, _)| arrow(doc, rid)).collect()
}

/// The caption runs among `arrows` (one page): same caption, same direction, ordered across the
/// direction, split where neighbouring tips are more than the run gap apart.
fn runs<'a>(doc: &'a Document, arrows: &[Arrow], run_gap: f64) -> Vec<(&'a str, Vec<Arrow>)> {
    let mut by: BTreeMap<(&'a str, Direction), Vec<Arrow>> = BTreeMap::new();
    for a in arrows {
        if let Some(c) = caption_of(doc, a.component) {
            by.entry((c.trim(), a.direction)).or_default().push(*a);
        }
    }
    let mut out = Vec::new();
    for ((caption, dir), mut list) in by {
        let across = |a: &Arrow| if dir.vertical() { a.tip.x } else { a.tip.y };
        list.sort_by(|a, b| across(a).total_cmp(&across(b)).then(a.rep.cmp(&b.rep)));
        let mut run: Vec<Arrow> = Vec::new();
        for a in list {
            if run.last().is_some_and(|l| across(&a) - across(l) > run_gap) {
                out.push((caption, std::mem::take(&mut run)));
            }
            run.push(a);
        }
        if !run.is_empty() {
            out.push((caption, run));
        }
    }
    out
}

/// A caption wrapped to `width` at word breaks, at most [`MAX_CAPTION_LINES`] lines, the last
/// ending with `…` when the caption is longer.
fn wrap(doc: &Document, cache: &mut Cache, caption: &str, width: f64) -> Vec<String> {
    let mut fits = |t: &str| cache.width(doc, TextKind::ContinuationCaption, t) <= width;
    let mut lines: Vec<String> = Vec::new();
    for word in caption.split_whitespace() {
        match lines.last_mut() {
            Some(l) if fits(&format!("{l} {word}")) => {
                l.push(' ');
                l.push_str(word);
            }
            _ => lines.push(word.to_string()),
        }
    }
    if lines.len() > MAX_CAPTION_LINES {
        lines.truncate(MAX_CAPTION_LINES);
        if let Some(last) = lines.last_mut() {
            while !last.is_empty() && !fits(&format!("{last}…")) {
                last.pop();
            }
            let trimmed = last.trim_end().to_string();
            *last = format!("{trimmed}…");
        }
    }
    lines
}

/// One run's bracket, ticks and caption lines, as parts of its lowest-ID arrow (§3.2).
fn run_parts(doc: &Document, places: &mut Places<'_>, cache: &mut Cache, caption: &str, run: &[Arrow], page: PageId) -> Vec<Part> {
    let Some(owner) = run.iter().map(|a| a.rep).min() else { return Vec::new() };
    let Some(first) = run.first() else { return Vec::new() };
    let dir = first.direction;
    let d = dir.unit();
    let s = cache.settings;
    // How far past each tip its reference text reaches, along the arrow.
    let mut reach = f64::NEG_INFINITY;
    for a in run {
        let value = cache.text(doc, places, a);
        let len = if value.is_empty() { 0.0 } else { s.gap + cache.width(doc, TextKind::Reference, &value) };
        reach = reach.max(a.tip.x * d.x + a.tip.y * d.y + len);
    }
    let along = reach + s.caption_gap;
    let across: Vec<f64> = run.iter().map(|a| if dir.vertical() { a.tip.x } else { a.tip.y }).collect();
    let lo = across.iter().copied().fold(f64::INFINITY, f64::min) - s.overhang;
    let hi = across.iter().copied().fold(f64::NEG_INFINITY, f64::max) + s.overhang;
    // A point at `t` across and `u` along the arrow.
    let at = |t: f64, u: f64| if dir.vertical() { Pt::new(t, u * d.y) } else { Pt::new(u * d.x, t) };
    let (a, b) = (at(lo, along), at(hi, along));
    let back = |p: Pt| Pt::new(p.x - d.x * s.tick, p.y - d.y * s.tick);
    if !(a.is_finite() && b.is_finite()) {
        return Vec::new();
    }
    let part = |index: u32, shape: Shape| Part { rep: owner, index, shape, aci: None, row: None, bar: false };
    let mut out = vec![
        part(BRACKET_INDEX, Shape::Line(a, b)),
        part(BRACKET_INDEX + 1, Shape::Line(a, back(a))),
        part(BRACKET_INDEX + 2, Shape::Line(b, back(b))),
    ];
    if text_shown(doc, TextTarget::page(page), TextKind::ContinuationCaption) == crate::text_style::Shown::Off {
        return out;
    }
    let lines = wrap(doc, cache, caption, (hi - lo).max(s.min_wrap));
    let h = s.caption_height;
    // A cap height and the space under it.
    let pitch = h * 1.6;
    let n = lines.len() as f64;
    let mid = (lo + hi) / 2.0;
    for (i, line) in lines.into_iter().enumerate() {
        let i_f = i as f64;
        let (p, halign, valign) = match dir {
            Direction::Down => (Pt::new(mid, a.y - s.text_gap - i_f * pitch), HAlign::Center, VAlign::Top),
            Direction::Up => (Pt::new(mid, a.y + s.text_gap + (n - 1.0 - i_f) * pitch), HAlign::Center, VAlign::Bottom),
            Direction::Right => (Pt::new(a.x + s.text_gap, mid + ((n - 1.0) / 2.0 - i_f) * pitch), HAlign::Left, VAlign::Middle),
            Direction::Left => (Pt::new(a.x - s.text_gap, mid + ((n - 1.0) / 2.0 - i_f) * pitch), HAlign::Right, VAlign::Middle),
        };
        if p.is_finite() {
            let index = CAPTION_INDEX.saturating_add(u32::try_from(i).unwrap_or(u32::MAX));
            out.push(part(index, Shape::Text { at: p, value: line, height: h, halign, valign, rotation: 0.0 }));
        }
    }
    out
}

/// Every continuation part drawn on `page` (design 049 §4): each arrow's reference text, then
/// each caption run's bracket and caption. O(the page's arrows and their names' arrows).
pub fn parts_on_page(doc: &Document, places: &mut Places<'_>, page: PageId) -> Vec<Part> {
    let arrows = arrows_on(doc, page);
    if arrows.is_empty() {
        return Vec::new();
    }
    let mut cache = Cache::new(doc);
    page_parts(doc, places, &mut cache, page, &arrows)
}

fn page_parts(doc: &Document, places: &mut Places<'_>, cache: &mut Cache, page: PageId, arrows: &[Arrow]) -> Vec<Part> {
    let mut out = Vec::new();
    for a in arrows {
        let value = cache.text(doc, places, a);
        if !value.is_empty() {
            out.push(reference_part(a, value, &cache.settings));
        }
    }
    for (caption, run) in runs(doc, arrows, cache.settings.run_gap) {
        out.extend(run_parts(doc, places, cache, caption, &run, page));
    }
    out
}

/// What the arrows of names `names` show, by page: their reference texts, and every caption run
/// part on a page that holds one of them. For comparing before and after a commit.
pub fn shown(doc: &Document, places: &mut Places<'_>, names: &BTreeSet<String>) -> BTreeMap<PageId, Vec<Part>> {
    let mut out: BTreeMap<PageId, Vec<Part>> = BTreeMap::new();
    if names.is_empty() {
        return out;
    }
    let mut cache = Cache::new(doc);
    let mut pages: BTreeSet<PageId> = BTreeSet::new();
    for n in names {
        let Some((_, members)) = group_named(doc, n) else { continue };
        for rid in members {
            let Some(a) = arrow(doc, rid) else { continue };
            pages.insert(a.page);
            let value = cache.text(doc, places, &a);
            if !value.is_empty() {
                out.entry(a.page).or_default().push(reference_part(&a, value, &cache.settings));
            }
        }
    }
    for page in pages {
        let arrows: Vec<Arrow> = arrows_on(doc, page).into_iter().filter(|a| caption_of(doc, a.component).is_some()).collect();
        for (caption, run) in runs(doc, &arrows, cache.settings.run_gap) {
            let parts = run_parts(doc, places, &mut cache, caption, &run, page);
            out.entry(page).or_default().extend(parts);
        }
    }
    out
}

// ---------------------------------------------------------------- per commit (design 049 §8)

/// The continuation names `ops` could change the texts or runs of, looked up in `doc` (called on
/// the state before and after the ops).
pub fn names_named(doc: &Document, ops: &[Op]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let comp = |out: &mut BTreeSet<String>, c: ComponentId| out.extend(name_of(doc, c));
    let rep = |out: &mut BTreeSet<String>, r: &Representation| {
        if let Subject::Component(c) = r.subject {
            comp(out, c);
        }
    };
    for op in ops {
        match op {
            Op::AddItem { id, .. } | Op::ModifyItem { id, .. } | Op::RemoveItem { id, .. } => {
                if let Some((_, r)) = doc.representation_of_item(*id) {
                    rep(&mut out, r);
                }
            }
            Op::LinkRepresentation { rep: r, .. } | Op::UnlinkRepresentation { rep: r, .. } => rep(&mut out, r),
            Op::ModifyRepresentation { before, after, .. } => {
                rep(&mut out, before);
                rep(&mut out, after);
            }
            Op::CreateComponent { component, .. } | Op::DeleteComponent { component, .. } => {
                out.extend(continuation_key(&component.properties));
            }
            Op::SetProperty { object: ObjectRef::Component(c), key, before, after } if key == NAME || key == KIND || key == CAPTION_KEY => {
                comp(&mut out, *c);
                if key == NAME && is_continuation(doc, *c) {
                    out.extend([before, after].into_iter().flatten().map(|n| name_key(n)).filter(|k| !k.is_empty()));
                }
            }
            Op::EmbedDefinition { id, .. } | Op::RemoveDefinition { id, .. } => {
                for c in doc.components_using(*id) {
                    comp(&mut out, c);
                }
            }
            _ => {}
        }
    }
    out
}

/// The names of the arrows on `page` (through the page index and the reference index).
pub fn names_on(doc: &Document, idx: &RefIndex, page: PageId) -> BTreeSet<String> {
    doc.representations_on(page)
        .filter(|(rid, _)| matches!(idx.member(*rid), Some((Group::Continuation(_), _))))
        .filter_map(|(_, r)| match r.subject {
            Subject::Component(c) => name_of(doc, c),
            _ => None,
        })
        .collect()
}

/// Whether a project key redraws every page with an arrow: this area's keys, and the reference
/// and caption texts' styles and the project font.
pub fn is_wide_key(key: &str) -> bool {
    key.starts_with("continuation.") || key.starts_with("text.reference.") || key.starts_with("text.continuation_caption.") || key == "text.font"
}

// ---------------------------------------------------------------- captions (design 049 §3.1)

/// The caption to store on component `c` for a typed `value`: `None` removes it.
///
/// # Errors
/// `c` is not a continuation arrow, or the caption breaks [`CAPTION_RULE`].
pub fn caption_value(doc: &Document, c: ComponentId, value: Option<&str>) -> Result<Option<String>, String> {
    if !is_continuation(doc, c) {
        let what = match crate::devices::tag_of(doc, c) {
            Some(t) => format!("{t} is a device"),
            None if doc.components.contains_key(&c) => "that is not one".to_string(),
            None => return Err("no such component".into()),
        };
        return Err(format!("Only continuation arrows have a caption; {what}."));
    }
    caption_text(value)
}

/// A typed caption, checked: `None` (remove) for nothing or blanks.
///
/// # Errors
/// [`CAPTION_RULE`]: a control character (a line break included), or more than 80 characters.
pub fn caption_text(value: Option<&str>) -> Result<Option<String>, String> {
    let Some(v) = value else { return Ok(None) };
    if v.chars().any(char::is_control) {
        return Err(CAPTION_RULE.into());
    }
    let t = v.trim();
    if t.is_empty() {
        return Ok(None);
    }
    if t.chars().count() > MAX_CAPTION_CHARS {
        return Err(CAPTION_RULE.into());
    }
    Ok(Some(t.to_string()))
}

// ---------------------------------------------------------------- reading (design 049 §5)

/// The other end Go to Reference goes to from arrow `source` (design 049 §5.3): `target` when it
/// is one, else the one after `last` (the place it went to last time) in sorted order, wrapping,
/// else the first.
pub fn goto(doc: &Document, source: RepresentationId, target: Option<RepresentationId>, last: Option<(PageId, Pt)>) -> Option<RepresentationId> {
    let ends = other_ends(doc, source);
    if let Some(t) = target {
        return ends.iter().any(|e| e.rep == t).then_some(t);
    }
    let at = |e: &End| doc.representations.get(&e.rep).and_then(|r| crate::xref::anchor(doc, r));
    let i = last.and_then(|(page, p)| ends.iter().position(|e| e.page == page && at(e).is_some_and(|a| a.x == p.x && a.y == p.y)));
    let next = match i {
        Some(i) => ends.get(i.saturating_add(1)).or_else(|| ends.first()),
        None => ends.first(),
    };
    next.map(|e| e.rep)
}

/// One other end in words: `102-C-2  page 102 "Supply"`.
pub fn end_words(doc: &Document, e: &End) -> String {
    format!("{}  {}", e.text, crate::references::page_words(doc, e.page))
}

/// Hover lines for arrow `rep` of continuation `c` (design 049 §5.4).
pub fn hover_lines(doc: &Document, c: ComponentId, rep: RepresentationId) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(cap) = caption_of(doc, c) {
        lines.push(format!("Caption: {cap}"));
    }
    let name = display_name(doc, c);
    let ends = other_ends(doc, rep);
    lines.push(match ends.len() {
        0 => format!("Continuation {name}: no other end"),
        1 => format!("Continuation {name}: 1 other end"),
        n => format!("Continuation {name}: {n} other ends"),
    });
    lines.extend(ends.iter().take(HOVER_ENDS).map(|e| format!("  {}", end_words(doc, e))));
    if ends.len() > HOVER_ENDS {
        lines.push(format!("  +{} more", ends.len() - HOVER_ENDS));
    }
    lines
}

/// The arrows among `cs`, and how many are not arrows.
pub fn arrows_among(doc: &Document, cs: &[ComponentId]) -> (Vec<ComponentId>, usize) {
    let mut seen = BTreeSet::new();
    let arrows: Vec<ComponentId> = cs.iter().copied().filter(|c| is_continuation(doc, *c) && seen.insert(*c)).collect();
    let other = cs.iter().filter(|c| !is_continuation(doc, **c)).collect::<BTreeSet<_>>().len();
    (arrows, other)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_caption_rule() {
        assert_eq!(caption_text(Some("  TO CONTROL SUPPLY ")).unwrap().as_deref(), Some("TO CONTROL SUPPLY"));
        assert_eq!(caption_text(Some("   ")).unwrap(), None);
        assert_eq!(caption_text(None).unwrap(), None);
        assert_eq!(caption_text(Some("A\nB")).unwrap_err(), CAPTION_RULE);
        assert_eq!(caption_text(Some(&"x".repeat(81))).unwrap_err(), CAPTION_RULE);
        assert!(caption_text(Some(&"x".repeat(80))).is_ok());
        assert!(caption_text(Some(&"x".repeat(10_000))).is_err());
    }

    #[test]
    fn the_directions() {
        let at = |deg: f64, mirror: bool| direction(&Placement { origin: Pt::new(10.0, 10.0), rotation: deg.to_radians(), mirror, scale: 1.0 });
        assert_eq!(at(0.0, false), Some(Direction::Down));
        assert_eq!(at(180.0, false), Some(Direction::Up));
        assert_eq!(at(90.0, false), Some(Direction::Right));
        assert_eq!(at(270.0, false), Some(Direction::Left));
        assert_eq!(direction(&Placement { origin: Pt::new(0.0, 0.0), rotation: 0.0, mirror: false, scale: 0.0 }), None);
    }

    /// FP3 (overseer ruling, 2026-11-13): an unrotated reference part draws byte-identically to
    /// the entity drawn before `rotation` existed.
    #[test]
    fn an_unrotated_part_draws_as_before() {
        let part = Part {
            rep: RepresentationId(7),
            index: 0,
            shape: Shape::Text {
                at: Pt::new(1.0, 2.0),
                value: "100-B-3".into(),
                height: 2.5,
                halign: HAlign::Left,
                valign: VAlign::Top,
                rotation: 0.0,
            },
            aci: None,
            row: None,
            bar: false,
        };
        let h = cadcraft_engine::doc::Handle(0x42);
        let before = crate::devices::text_entity_with("100-B-3", Pt::new(1.0, 2.0), 2.5, HAlign::Left, VAlign::Top, h, crate::devices::TAG_LAYER);
        assert_eq!(format!("{:?}", crate::project::xref_entity(&part, h)), format!("{before:?}"));
    }

    #[test]
    fn the_text_lists_three_then_more() {
        let page = PageId(1);
        let end = |n: u128, t: &str| End { rep: RepresentationId(n), page, key: None, text: t.into() };
        let ends = vec![end(1, "1-A-1"), end(2, "2-A-1"), end(3, "3-A-1"), end(4, "4-A-1"), end(5, "5-A-1")];
        assert_eq!(text_for(&ends, RepresentationId(1), true, 3), "2-A-1 3-A-1 4-A-1 +1");
        assert_eq!(text_for(&ends, RepresentationId(9), false, 1), "1-A-1 +4");
        assert_eq!(text_for(&ends[..1], RepresentationId(1), true, 3), "");
    }
}
