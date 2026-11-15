//! The page type and folder commands (brief 054, design 054): `folderprops`, `renumberpages` and
//! `deletepage`. The rules (folder starts, renumbering, check rows) are [`crate::page_types`].

use std::collections::BTreeSet;

use cadcraft_engine::{CommandSpec, Session};
use ecad_model::book::{Book, NUMBER_KEY};
use ecad_model::{Document, FolderId, ObjectRef, Op, PageId};
use serde_json::{Value, json};

use crate::commands::{err, id, with_project};
use crate::generated;
use crate::page_types::*;

/// The commands this module adds (registered in [`crate::lanes`]).
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("folderprops", "Folder Properties", run_folderprops)
            .params("{folder: id, start?: \"100\"|null} (a folder's start number: 1 to 6 digits, leading zeros set the width; null clears; no start: read only)")
            .noundo(),
        CommandSpec::new("renumberpages", "Renumber Pages", run_renumberpages)
            .params("{folder?: id} (renumbers each folder from its start number, the rest from 1; with folder, that folder and its sub-folders only; one transaction)")
            .noundo(),
        CommandSpec::new("deletepage", "Delete Page", run_deletepage)
            .params("{page: id} or {pages: [id]} (generated pages only: a cover, a contents, a report…; one transaction)")
            .noundo(),
    ]
}

fn folder_param(doc: &Document, params: &Value) -> Result<Option<FolderId>, String> {
    let f: Option<FolderId> = crate::commands::opt_id(params, "folder")?;
    match f {
        Some(f) if !doc.folders.contains_key(&f) => Err("no such folder".into()),
        f => Ok(f),
    }
}

fn run_folderprops(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let folder: FolderId = id(params, "folder").map_err(err)?;
    let start = match params.get("start") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(Value::String(t)) if t.trim().is_empty() => Some(None),
        Some(Value::String(t)) if is_start(t.trim()) => Some(Some(t.trim().to_string())),
        Some(_) => return Err(err(BAD_START)),
    };
    with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let key = start_key(folder);
        // A start key whose folder is gone (a hostile or merged file) may only be cleared.
        let gone = !doc.folders.contains_key(&folder) && doc.properties.contains_key(&key) && start == Some(None);
        let name = match doc.folders.get(&folder) {
            Some(f) => f.name.clone(),
            None if gone => String::from("(gone)"),
            None => return Err("no such folder".into()),
        };
        let before = doc.properties.get(&key).cloned();
        let Some(after) = start else {
            return Ok(json!({"folder": folder.hex(), "name": name, "start": before}));
        };
        if before == after {
            return Ok(json!({"folder": folder.hex(), "name": name, "start": after, "changed": false}));
        }
        let ops = vec![Op::SetProperty { object: ObjectRef::Project, key, before, after: after.clone() }];
        p.apply_edit(pg, "folderprops", "Folder start number", ops)?;
        let line = match &after {
            Some(n) => format!("Folder \"{name}\" starts at {n}. Press Renumber pages to number its pages from it."),
            None => format!("Folder \"{name}\" has no start number now."),
        };
        pg.echo(line.clone());
        Ok(json!({"folder": folder.hex(), "name": name, "start": after, "changed": true, "message": line}))
    })
}

fn run_renumberpages(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    with_project(s, |p, pg, _| {
        let scope = folder_param(p.ledger.document(), params)?;
        if generated::has_bom_page(p.ledger.document()) {
            p.outputs();
        }
        let doc = p.ledger.document();
        let outputs = p.outputs.as_ref();
        let plan = renumber(doc, crate::draw::outputs::kept_of(outputs), scope);
        let overlaps: Vec<String> =
            overlaps(doc, crate::draw::outputs::kept_of(outputs)).into_iter().map(|(a, b, n)| overlap_words(doc, a, b, n)).collect();
        if plan.is_empty() {
            let line = "Page numbers already in order.".to_string();
            pg.echo(line.clone());
            return Ok(json!({"renumbered": 0, "message": line, "overlaps": overlaps}));
        }
        let ops: Vec<Op> = plan
            .iter()
            .map(|(page, n)| Op::SetProperty {
                object: ObjectRef::Page(*page),
                key: NUMBER_KEY.into(),
                before: doc.pages.get(page).and_then(|x| x.properties.get(NUMBER_KEY)).cloned(),
                after: Some(n.clone()),
            })
            .collect();
        let count = plan.len();
        let pages: Vec<Value> = plan.iter().map(|(pg, n)| json!({"page": pg.hex(), "number": n})).collect();
        p.apply_edit(pg, "renumberpages", "Renumber pages", ops)?;
        let line = format!("Renumbered {count} page{}.", if count == 1 { "" } else { "s" });
        pg.echo(line.clone());
        for o in &overlaps {
            pg.echo(o.clone());
        }
        Ok(json!({"renumbered": count, "pages": pages, "message": line, "overlaps": overlaps}))
    })
}

fn run_deletepage(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let ids: Vec<PageId> = if params.get("pages").is_some() {
        crate::commands::id_list(params, "pages", 1_000).map_err(err)?
    } else {
        vec![id(params, "page").map_err(err)?]
    };
    if ids.is_empty() {
        return Err(err("Give the page to delete."));
    }
    with_project(s, |p, pg, active| {
        let doc = p.ledger.document();
        let set: BTreeSet<PageId> = ids.iter().copied().collect();
        for page in &set {
            let x = doc.pages.get(page).ok_or("no such page")?;
            if generated::kind_of(doc, *page).is_none() || doc.representations_on(*page).next().is_some() {
                let _ = x;
                return Err(DRAWN_PAGE.into());
            }
        }
        let remaining: Vec<PageId> = doc.page_order.iter().copied().filter(|p| !set.contains(p)).collect();
        if remaining.is_empty() {
            return Err("A project keeps at least one page.".into());
        }
        // Remove from the back so each index is the page's place when its op is applied (and undo,
        // reversed, puts them back in order).
        let mut placed: Vec<(usize, PageId)> = doc.page_order.iter().enumerate().filter(|(_, p)| set.contains(p)).map(|(i, p)| (i, *p)).collect();
        placed.sort_by_key(|p| std::cmp::Reverse(p.0));
        let mut ops = Vec::new();
        let mut words = Vec::new();
        let book = Book::of(doc);
        for (index, page) in &placed {
            let Some(x) = doc.pages.get(page) else { continue };
            words.push(format!("{} ({})", x.title, book.number(doc, *page)));
            ops.push(Op::RemovePage { id: *page, page: x.clone(), index: *index });
        }
        words.reverse();
        // The page shown moves to the page before it when it goes (design 016 §4.3).
        if set.contains(&active) {
            let at = doc.page_order.iter().position(|p| *p == active).unwrap_or(0);
            let earlier = doc.page_order.get(..at).unwrap_or_default().iter().rev().find(|p| !set.contains(p)).copied();
            if let Some(to) = earlier.or(remaining.first().copied()) {
                p.open_page(pg, to)?;
            }
        }
        p.apply_edit(pg, "deletepage", "Delete page", ops)?;
        let line = format!("Removed {}. Press u to bring it back.", words.join(", "));
        pg.echo(line.clone());
        Ok(json!({"deleted": ids.iter().map(|p| p.hex()).collect::<Vec<_>>(), "message": line}))
    })
}
