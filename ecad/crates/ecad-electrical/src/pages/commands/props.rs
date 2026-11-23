//! The generic property and revision commands: `pageprops`, `projectprops` and `revision`. Their
//! rules are [`super::sheets`]; issuing a revision may Finalise numbering as a transaction of its
//! own first, so `issuerevision` stays the engine's.

use std::collections::BTreeMap;

use ecad_model::document::book::Book;
use ecad_model::{Document, PageId, ReferenceStyle};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::sheets::{
    PROJECT_KEYS, PageChange, ProjectChange, PropsPlan, RevisionFields, page_json, page_props, page_title, project_json, project_props,
    proposal_mode, release_line_refusal, revision_fields, revision_ops, tag_case,
};
use crate::commands::input::nullable;
use crate::commands::run::{Cx, Generic, Outcome, Pages, Slot, State};
use crate::error::{CmdError, msg};
use crate::pages::generated;
use crate::releases::revisions;
use crate::styles::text_style;
use crate::symbols::proposals;
use crate::views::Views;

/// `pageprops {page?, number?, title?, function?, location?, sheet?, proposal?}`: a page's
/// properties (each null clears; no fields: the current values, read only).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PagepropsParams {
    /// The page (default: the active page).
    #[serde(default)]
    page: Option<PageId>,
    /// The page number: 1 to 12 letters, digits or _ (null: back to the book position).
    #[serde(default, deserialize_with = "nullable")]
    number: Option<Option<String>>,
    /// The page title: 1 to 200 characters.
    #[serde(default, deserialize_with = "nullable")]
    title: Option<Option<String>>,
    /// The page's function (=…), or null for the project's.
    #[serde(default, deserialize_with = "nullable")]
    function: Option<Option<String>>,
    /// The page's location (+…), or null for the project's.
    #[serde(default, deserialize_with = "nullable")]
    location: Option<Option<String>>,
    /// The page's sheet ("A3", "A2 12x8"…), or null (or "project") for the project's.
    #[serde(default, deserialize_with = "nullable")]
    sheet: Option<Option<String>>,
    /// What a new function or location does to the page's device symbols: "propose" (default),
    /// "move" or "keep".
    #[serde(default)]
    proposal: Option<String>,
    /// Refused: a page's kind is set when it is made.
    #[serde(default, deserialize_with = "nullable")]
    generated: Option<Option<Value>>,
    /// Refused for `text.` keys (Text styles); page properties are fields of their own.
    #[serde(default)]
    props: Option<Map<String, Value>>,
}

/// How cross-references are written.
#[derive(Deserialize, JsonSchema, Clone, Copy)]
pub(crate) enum ReferenceStyleParam {
    /// Page and zone (`12.C4`).
    #[serde(rename = "page-zone")]
    Zone,
    /// Page, column and row.
    #[serde(rename = "page-column-row")]
    ColumnRow,
    /// Page and column.
    #[serde(rename = "page-column")]
    Column,
}

/// `projectprops {title?, drawing_number?, customer?, status?, function?, location?, sheet?,
/// page_number_step?, reference_style?, contact_mirror?, tag_padding?, tag_prefixes?,
/// colour_style?, label_height?, tag_dash?, tag_case?, revision_columns?, revision_pending?,
/// provisional?, props?, proposal?}`: the project's properties and settings, in one transaction
/// (no fields: everything, read only).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectpropsParams {
    /// The project title (null clears).
    #[serde(default, deserialize_with = "nullable")]
    title: Option<Option<String>>,
    /// The drawing number (null clears).
    #[serde(default, deserialize_with = "nullable")]
    drawing_number: Option<Option<String>>,
    /// The customer (null clears).
    #[serde(default, deserialize_with = "nullable")]
    customer: Option<Option<String>>,
    /// The status, any text (null clears).
    #[serde(default, deserialize_with = "nullable")]
    status: Option<Option<String>>,
    /// The project's function (=…; null clears).
    #[serde(default, deserialize_with = "nullable")]
    function: Option<Option<String>>,
    /// The project's location (+…; null clears).
    #[serde(default, deserialize_with = "nullable")]
    location: Option<Option<String>>,
    /// The sheet: "A3", "A2 12x8"…
    #[serde(default, deserialize_with = "nullable")]
    sheet: Option<Option<String>>,
    /// The page-number step, 1 to 1000.
    #[serde(default, deserialize_with = "nullable")]
    page_number_step: Option<Option<u64>>,
    /// How cross-references are written.
    #[serde(default, deserialize_with = "nullable")]
    reference_style: Option<Option<ReferenceStyleParam>>,
    /// Where contact tables go by default: "auto", "below", "right" or "off".
    #[serde(default, deserialize_with = "nullable")]
    contact_mirror: Option<Option<String>>,
    /// Digits of a tag's number, 1 to 6.
    #[serde(default, deserialize_with = "nullable")]
    tag_padding: Option<Option<u64>>,
    /// Tag prefixes by symbol: a prefix, or null for the built-in one.
    #[serde(default, deserialize_with = "nullable")]
    tag_prefixes: Option<Option<BTreeMap<String, Option<String>>>>,
    /// How label colours are written: "code2" (BN, BU, GN/YE), "code3" (BRN, BLU, GRN/YEL) or "name".
    #[serde(default, deserialize_with = "nullable")]
    colour_style: Option<Option<String>>,
    /// Wire label height in mm, 0.5 to 10.
    #[serde(default, deserialize_with = "nullable")]
    label_height: Option<Option<f64>>,
    /// Whether tags have a leading `-`.
    #[serde(default, deserialize_with = "nullable")]
    tag_dash: Option<Option<bool>>,
    /// "upper" or "typed": whether tags keep the case they are typed in.
    #[serde(default, deserialize_with = "nullable")]
    tag_case: Option<Option<String>>,
    /// The revision table's columns ("rev, date, ecr:ECR@20, description"), or null for the
    /// built-in ones.
    #[serde(default, deserialize_with = "nullable")]
    revision_columns: Option<Option<String>>,
    /// How the pending revision shows: "star", "draft" or "off".
    #[serde(default, deserialize_with = "nullable")]
    revision_pending: Option<Option<String>>,
    /// The revision scheme: `{kind: "letters"}` (A, B … the default), `{kind: "letters_dash"}` (–, A, B …) or
    /// `{kind: "numbered", prefix?: "P", width?: 2, start?: 1}` (P01, P02 …). Refused once a
    /// revision is issued.
    #[serde(default)]
    rev_scheme: Option<RevSchemeParam>,
    /// Provisional tags until Finalise.
    #[serde(default, deserialize_with = "nullable")]
    provisional: Option<Option<bool>>,
    /// Free properties: key: value, or null to clear.
    #[serde(default)]
    props: Option<BTreeMap<String, Option<String>>>,
    /// What a new function or location does to the device symbols: "propose" (default), "move" or
    /// "keep".
    #[serde(default, deserialize_with = "nullable")]
    proposal: Option<Option<String>>,
}

/// A revision scheme as `projectprops` takes it.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RevSchemeParam {
    kind: RevSchemeKind,
    /// Numbered: the text before the number (0 to 4 characters; default none).
    #[serde(default)]
    prefix: Option<String>,
    /// Numbered: the digits, at least (default 2).
    #[serde(default)]
    width: Option<u64>,
    /// Numbered: the first number (default 1).
    #[serde(default)]
    start: Option<u64>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum RevSchemeKind {
    /// `A`, `B` …
    Letters,
    /// `–`, `A`, `B` …
    LettersDash,
    /// `P01`, `P02` …
    Numbered,
}

impl RevSchemeParam {
    fn scheme(self) -> Result<ecad_model::RevScheme, CmdError> {
        match self.kind {
            RevSchemeKind::Letters => Ok(ecad_model::RevScheme::Letters { dash_first: false }),
            RevSchemeKind::LettersDash => Ok(ecad_model::RevScheme::Letters { dash_first: true }),
            RevSchemeKind::Numbered => {
                let prefix = self
                    .prefix
                    .as_deref()
                    .map(str::trim)
                    .filter(|p| !p.is_empty())
                    .map(|p| ecad_model::RevPrefix::new(p).map_err(|e| CmdError::refused(format!("The prefix is refused: {e}."))))
                    .transpose()?;
                let width = u8::try_from(self.width.unwrap_or(2)).ok().filter(|w| (1..=8).contains(w));
                let start = u32::try_from(self.start.unwrap_or(1)).ok();
                match (width, start) {
                    (Some(width), Some(start)) => Ok(ecad_model::RevScheme::Numbered { prefix, width, start }),
                    _ => Err(CmdError::refused("A numbered scheme has a width from 1 to 8 and a first number that is a whole number.")),
                }
            }
        }
    }
}

/// `revision {rev?, date?, description?, drawn?, checked?, approved?, fields?, n?}`: the pending
/// revision's fields, custom fields by name; an issued row is refused.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RevisionParams {
    /// The rev (null clears).
    #[serde(default, deserialize_with = "nullable")]
    rev: Option<Option<String>>,
    /// The date (null clears).
    #[serde(default, deserialize_with = "nullable")]
    date: Option<Option<String>>,
    /// The description (null clears).
    #[serde(default, deserialize_with = "nullable")]
    description: Option<Option<String>>,
    /// Who drew it (null clears).
    #[serde(default, deserialize_with = "nullable")]
    drawn: Option<Option<String>>,
    /// Who checked it (null clears).
    #[serde(default, deserialize_with = "nullable")]
    checked: Option<Option<String>>,
    /// Who approved it (null clears).
    #[serde(default, deserialize_with = "nullable")]
    approved: Option<Option<String>>,
    /// Custom fields by name: text, or null to clear.
    #[serde(default)]
    fields: Option<BTreeMap<String, Option<String>>>,
    /// The revision row meant; refused unless it is the pending one.
    #[serde(default)]
    n: Option<u64>,
}

impl ProjectpropsParams {
    /// Whether no field is given (`projectprops` then only reads).
    fn is_empty(&self) -> bool {
        let fields = [&self.title, &self.drawing_number, &self.customer, &self.status, &self.function, &self.location];
        fields.iter().all(|f| f.is_none())
            && self.sheet.is_none()
            && self.page_number_step.is_none()
            && self.reference_style.is_none()
            && self.contact_mirror.is_none()
            && self.tag_padding.is_none()
            && self.tag_prefixes.is_none()
            && self.colour_style.is_none()
            && self.label_height.is_none()
            && self.tag_dash.is_none()
            && self.tag_case.is_none()
            && self.revision_columns.is_none()
            && self.revision_pending.is_none()
            && self.rev_scheme.is_none()
            && self.provisional.is_none()
            && self.props.is_none()
            && self.proposal.is_none()
    }
}

impl RevisionParams {
    fn fields(self) -> RevisionFields {
        revision_fields([self.rev, self.date, self.description, self.drawn, self.checked, self.approved], self.n, self.fields)
    }
}

/// A plan's ops as one transaction `label`, with what it asks about kept as the conflicts waiting
/// when it moved nothing, and echoed. `answer` reads the JSON from the document once it is committed.
fn props_outcome(
    doc: &Document, label: &str, plan: PropsPlan, state: &mut Slot<'_>, answer: impl FnOnce(&Document, &mut Views) -> Value + 'static,
) -> Outcome {
    if plan.proposed.moved == 0
        && let Slot::Conflicts { set, .. } = state
    {
        **set = Some(plan.proposed.conflicts.clone());
    }
    let lines = proposals::conflict_words(doc, &plan.proposed.conflicts);
    Outcome::edit_then_derived(label, plan.ops, answer).echo_lines(lines)
}

impl Generic for PagepropsParams {
    const ID: &'static str = "pageprops";
    const TITLE: &'static str = "Page Properties";
    const HELP: &'static str = "{page?: id (default: the active page), number?, title?, function?, location?, sheet?} (each null \
clears; no fields: the current values, read only)";
    const PAGES: Pages = Pages::ReadAnywhere;
    const STATE: State = State::Conflicts;

    fn check(&self, _registry: &ecad_model::registry::data::Registry) -> Result<(), CmdError> {
        if self.generated.is_some() {
            return Err(CmdError::refused(generated::KIND_FIXED));
        }
        // A page's `text.` keys are written by `showtext` only, also when asked
        // for as `props`.
        if self.props.as_ref().is_some_and(|o| o.keys().any(|k| text_style::is_reserved_key(k))) {
            return Err(CmdError::refused(text_style::RESERVED_KEY));
        }
        match &self.title {
            None => Ok(()),
            Some(t) => page_title(t.as_deref().unwrap_or_default()).map(|_| ()),
        }
    }

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let title = match self.title {
            None => None,
            Some(t) => Some(page_title(t.as_deref().unwrap_or_default())?),
        };
        let mut change =
            PageChange { number: self.number, title, function: self.function, location: self.location, sheet: self.sheet, mode: Default::default() };
        let doc = cx.doc;
        if generated::has_bom_page(doc) {
            cx.derived.reports(doc);
        }
        let page = self.page.or(cx.page).ok_or(CmdError::input("`page` (hex id) is required: no page is shown"))?;
        if !doc.pages().contains_key(&page) {
            return Err(CmdError::refused(msg::NO_SUCH_PAGE));
        }
        if !change.writes() {
            return Ok(Outcome::answer(page_json(doc, &Book::of(doc), page, cx.derived.kept_reports())));
        }
        change.mode = proposal_mode(self.proposal.as_deref())?;
        let plan = page_props(doc, cx.ids, page, &change, cx.derived.kept_reports())?;
        if plan.ops.is_empty() {
            let mut v = page_json(doc, &Book::of(doc), page, cx.derived.kept_reports());
            v["changed"] = json!(false);
            return Ok(Outcome::answer(v));
        }
        Ok(props_outcome(doc, "Page properties", plan, &mut cx.state, move |doc, views| {
            let mut v = page_json(doc, &Book::of(doc), page, views.kept_reports());
            v["changed"] = json!(true);
            v
        }))
    }
}

impl Generic for ProjectpropsParams {
    const ID: &'static str = "projectprops";
    const TITLE: &'static str = "Project Properties";
    const HELP: &'static str = "{title?, drawing_number?, customer?, status?, function?, location?, sheet?: \"A3\"|\"A2 12x8\"|…, \
page_number_step?: n, reference_style?: \"page-zone\"|\"page-column-row\"|\"page-column\", contact_mirror?: \"auto\"|\"below\"|\"right\"|\"off\", tag_padding?: 1..6, \
tag_prefixes?: {symbol: prefix|null}, colour_style?: \"code2\"|\"code3\"|\"name\", label_height?: mm, \
tag_dash?: bool, tag_case?: \"upper\"|\"typed\", revision_columns?: \"rev, date, ecr:ECR@20, description\"|null, \
revision_pending?: \"star\"|\"draft\"|\"off\", props?: {key: value|null}} (one transaction; no fields: everything, read only)";
    const PAGES: Pages = Pages::ReadAnywhere;
    const STATE: State = State::Conflicts;

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let doc = cx.doc;
        if self.is_empty() {
            return Ok(Outcome::answer(project_json(doc)));
        }
        let given = [self.title, self.drawing_number, self.customer, self.status, self.function, self.location];
        let fields = PROJECT_KEYS.into_iter().zip(given).filter_map(|(k, v)| v.map(|v| (k, v))).collect();
        let mut change = ProjectChange {
            fields,
            props: self.props.map(|m| m.into_iter().collect()).unwrap_or_default(),
            provisional: self.provisional.flatten(),
            sheet: self.sheet.flatten(),
            page_number_step: self.page_number_step.flatten(),
            reference_style: self.reference_style.flatten().map(|r| match r {
                ReferenceStyleParam::Zone => ReferenceStyle::PageZone,
                ReferenceStyleParam::ColumnRow => ReferenceStyle::PageColumnRow,
                ReferenceStyleParam::Column => ReferenceStyle::PageColumn,
            }),
            contact_mirror: self.contact_mirror.flatten(),
            tag_padding: self.tag_padding.flatten(),
            tag_prefixes: self.tag_prefixes.flatten().map(|m| m.into_iter().collect()).unwrap_or_default(),
            colour_style: self.colour_style.flatten(),
            label_height: self.label_height.flatten(),
            revision_columns: self.revision_columns,
            revision_pending: self.revision_pending.flatten(),
            rev_scheme: self.rev_scheme.map(RevSchemeParam::scheme).transpose()?,
            tag_dash: self.tag_dash.flatten(),
            tag_case: None,
            mode: Default::default(),
        };
        change.mode = proposal_mode(self.proposal.flatten().as_deref())?;
        change.tag_case = self.tag_case.flatten().as_deref().map(tag_case).transpose()?;
        let plan = project_props(doc, cx.ids, &change)?;
        if plan.ops.is_empty() {
            let mut v = project_json(doc);
            v["changed"] = json!(false);
            return Ok(Outcome::answer(v));
        }
        let message = plan.message.clone();
        Ok(props_outcome(doc, "Project properties", plan, &mut cx.state, move |doc, _| {
            let mut v = project_json(doc);
            v["changed"] = json!(true);
            if revisions::widths(&revisions::columns(doc), revisions::A3_TABLE_MM).1 {
                v["narrowed"] = json!(revisions::NARROWED);
            }
            if let Some(m) = message {
                v["message"] = json!(m);
            }
            v
        }))
    }
}

impl Generic for RevisionParams {
    const ID: &'static str = "revision";
    const TITLE: &'static str = "Revision";
    const HELP: &'static str = "{rev?, date?, description?, drawn?, checked?, approved?, fields?: {field: text|null}, n?} (the pending revision's \
fields, custom fields by name; an issued row is refused)";

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let f = self.fields();
        if let Some(r) = release_line_refusal(cx.off_release_line.as_deref(), false) {
            return Err(CmdError::refused(r));
        }
        let doc = cx.doc;
        let ops = revision_ops(doc, &f)?;
        if ops.is_empty() {
            return Ok(Outcome::answer(json!({"changed": false, "pending_rev": revisions::pending_rev(doc)})));
        }
        Ok(Outcome::edit_then("Revision", ops, |doc| json!({"changed": true, "pending_rev": revisions::pending_rev(doc)})))
    }
}
