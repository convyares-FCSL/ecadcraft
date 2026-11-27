//! `plcpage`: a new schematic page with every channel
//! of a card not drawn yet as cells in rows, and the card body after the last cell when the body is
//! drawn nowhere. One transaction.
//!
//! # Semantics
//! - The page goes after `after` (a page number as shown), else after the last page on which a card
//!   of the same rack (or the rack) is drawn, else after the last schematic page, else at the end.
//! - Its title is `plc.page.title` with the card's words filled in.
//! - Cells start `LEFT_MM` (ecad-electrical's PLC commands) from the left of the drawing area at 60 % of its height (outputs at
//!   180°, at 40 %); a row wider than the drawing area wraps into further rows `ROW_PITCH_MM`
//!   apart.
//!
//! The rules are [`ecad_electrical::plc::commands`]; this shim numbers the page (from the kept outputs)
//! and commits.
//!
//! Excludes: placing anything on an existing page (that is `plcchannels`).

use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::CmdError;
use ecad_electrical::plc::commands as rules;
use ecad_electrical::plc::commands::title;
use ecad_model::document::book::{self, Book};
use ecad_model::{Op, Page, PageId};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::commands::input::{Command, typed};
use crate::commands::{on_any_page, plain_style};
use ecad_electrical::commands::input::TextOrNumber;
use ecad_electrical::plc::channels::first_number;

/// The command `plcpage`.
pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec::new("plcpage", "Make Card Page", typed::<PlcpageParams>).noundo()]
}

/// A new page with every channel of a PLC card not drawn yet, and the card's body. One transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlcpageParams {
    /// The card: an ID or a tag.
    card: String,
    /// The page number the new page goes after (absent: after the rack's last card page).
    #[serde(default)]
    after: Option<TextOrNumber>,
}

impl Command for PlcpageParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let after = self.after.map(|t| t.text());
        on_any_page(s, |p, pg| {
            if ecad_electrical::pages::generated::has_bom_page(p.ledger.document()) {
                p.reports();
            }
            let doc = p.ledger.document();
            let pc = rules::page_card(doc, &self.card)?;
            let book = Book::of(doc);
            let before = rules::after_page(doc, &book, pc.card, after.as_deref())?;
            let outputs = p.kept_reports();
            let position = before.and_then(|b| book.position(b)).map_or(book.order.len() + 1, |x| x + 1);
            let number = ecad_electrical::pages::commands::sheets::default_number(doc, &book, before, position, outputs);
            let title = title(doc, pc.card, pc.ty);
            let mut new_page = Page::new(ecad_electrical::pages::commands::sheets::page_title(&title)?);
            new_page.number = ecad_model::PageNo::parse(&number);
            new_page.folder = before.and_then(|b| book::folder_of(doc, b)).filter(|f| doc.folders().contains_key(f));
            let index = before.and_then(|b| doc.page_order().iter().position(|x| *x == b)).map_or(doc.page_order().len(), |i| i + 1);
            let id: PageId = p.ids.fresh();
            let mut ops = vec![Op::AddPage { id, page: new_page, index }];
            let style = plain_style(p);
            rules::plan_page_contents(&p.ledger, &mut p.ids, &mut p.indexes.tag_high, id, style, &pc, &mut ops)?;
            p.apply_edit(pg, "plcpage", "Make card page", ops)?;
            p.pages.entry(id).or_default();
            let first = first_number(p.ledger.document(), pc.card);
            let message = rules::page_message(&number, &title, &pc.tag, first, &pc.missing);
            pg.echo(message.clone());
            Ok(json!({
                "page": id.hex(),
                "number": number,
                "title": title,
                "card": pc.card.hex(),
                "tag": pc.tag,
                "channels": pc.missing.iter().map(|i| i + first).collect::<Vec<_>>(),
                "body": pc.body,
                "message": message,
            }))
        })
    }
}

/// The ops that put the generated `plcrack` page ("PLC
/// racks") in the book, when `ops` place the project's first rack body and the book has no such
/// page; empty otherwise. They join the same transaction, so undoing the placement removes the
/// page too.
///
/// # Errors
/// The new page's own refusals (none expected: its number is the next one).
pub(crate) fn rack_page_ops(p: &mut crate::project::Project, ops: &[ecad_model::Op]) -> Result<(Option<PageId>, Vec<ecad_model::Op>), CmdError> {
    use ecad_model::{ItemKind, Op};
    let doc = p.ledger.document();
    let is_rack_def = |d: ecad_model::DefinitionId| {
        let def = doc.definitions().get(&d).or_else(|| {
            ops.iter().find_map(|op| match op {
                Op::EmbedDefinition { id, definition } if *id == d => Some(definition),
                _ => None,
            })
        });
        def.and_then(ecad_electrical::symbols::catalogue::of_definition)
            .and_then(ecad_electrical::plc::rack::CardType::of_symbol)
            .is_some_and(ecad_electrical::plc::rack::CardType::is_rack)
    };
    let places_rack = ops.iter().any(|op| match op {
        Op::AddItem { item, .. } => matches!(&item.kind, ItemKind::Symbol(s) if is_rack_def(s.definition)),
        _ => false,
    });
    if !places_rack {
        return Ok((None, Vec::new()));
    }
    let has_page = doc.page_order().iter().any(|pg| ecad_electrical::pages::generated::kind_of(doc, *pg) == Some(ecad_model::GenKind::PlcRack));
    let other_rack = doc.devices().keys().any(|c| ecad_electrical::plc::edit::station_of(doc, *c).is_some());
    if has_page || other_rack {
        return Ok((None, Vec::new()));
    }
    let kind = ecad_model::GenKind::PlcRack;
    let (id, _, mut more) = crate::pages::commands::sheets::new_page_ops_at(p, kind.title().to_string(), None, None, None)?;
    for op in &mut more {
        if let Op::AddPage { page, .. } = op {
            page.kind = ecad_model::PageKind::generated(kind);
        }
    }
    Ok((Some(id), more))
}
