# Commands: one generic runner (owner decision 5, 2026-11-21)

Commands are generic by default. One runner in ecad-engine does the whole path of a generic
command; an engine command file exists only where the command needs the session (the drawing, the
cursor, the selection or CADCraft entities). Ids, JSON forms, answers, MCP schemas and refusal
texts do not change.

## 1. The engine's `src/cmd/` today

**G** generic (moves onto the runner, its engine file goes), **S** session-specific (keeps an
engine file), **G/S** mixed (the generic commands move, the file keeps the session ones).

| File | | Commands | Reason |
| --- | --- | --- | --- |
| accessory.rs | G | accessory, jumper | Plan on the document plus the derived strip models; the interactive forms stay in `interact`. |
| bom.rs | G | partline, bomitem, article | Plan and commit; `bomitem`'s interactive form stays in `interact`. |
| cable.rs | G | cablenew, cableedit, cableremove | Plan on the document and the tag high-water mark. |
| cable_draw.rs | S | cableslash, cableadd, cablecore, cablefan, cableview | Cable views and fan joints are drawing state; the slash and fan measure the drawn text. |
| connect.rs | G | connect, disconnect | Rules return ops; the answer is fixed. |
| connector.rs | S | connector, pinlabel | `connector` places symbols through the drawing's place planner and snap tolerance. |
| continuation.rs | G | contcaption | Plan and commit; remembers the last caption (project state, not session). |
| default.rs | G | setdefault | Plan and commit. |
| designation.rs | G | designations, designationadd/edit/merge/remove | Plan on the document and the designation-use index. |
| device.rs | G/S | G: symbols, devices, deviceprops, ecaddelete, devicedelete, tagsettings. S: place, convertcomponent, addterminal | Placing and converting work on CADCraft entities, the selection and snapping. |
| device_link.rs | G | linkdevices, unlinkdevices, userfield | Plan and commit. |
| level.rs | G | termlabel, toposition | Plan and commit. |
| location.rs | G | conflicts, setlocation, setfunction | Plan and commit; keeps the waiting conflicts (project state). |
| numbering.rs | G | renumber, finalise, numbering | Plan on the document in drawing order (the order is in the model). |
| output.rs | G/S | G: locationbox, functionbox, bom, wirelist, cablelist. S: printbook, reports | Print Book renders pages; `reports` opens report pages. |
| page.rs | G/S | G: renamepage, movepage. S: newpage, openpage, ecadinspect | Opening and creating page views, and the live drawing facts of the inspector. |
| page_type.rs | G/S | G: folderprops, renumberpages. S: deletepage | Deleting closes the page's view. |
| prop.rs | G | setprop | Many routes, all plan and commit on the document and the tag high-water mark. |
| rating.rs | G | rating, ratingtable, ratings | Plan and commit; `ratingtable` declares a CSV read or write. |
| reference.rs | G/S | G: cableref, cablecontinue, cablerefs. S: cablegoto | `cablegoto` opens a page, zooms and selects. |
| release.rs | S | prerelease, release | The release folder renders every page to PDF (export). |
| sheet.rs | G/S | G: pageprops, projectprops, newfolder, renamefolder, movefolder, deletefolder, revision, issuerevision. S: openrevision, closerevision, undoissue | Revision views redraw pages; `undoissue` drives the engine's undo history. |
| style.rs | G | standard, setstyle, deletestyle, promote, styles | Plan and commit; `standard` declares reading another `.ecad`. |
| symbol.rs | G/S | G: symbolnew, symboledit, symbolimpact, symbolupdate. S: makesymbol | `makesymbol` reads the selection's CADCraft entities. |
| terminal.rs | G/S | G: terminals, stripplan, cablecores. S: bridge | `bridge` places through the drawing's place planner. |
| text.rs | G/S | G: textstyle, pagetexts, defaults, movetext. S: showtext, hiddentext | Showing hidden text is a drawing setting that redraws open pages. |
| title_block.rs | G | titleblocks, titleblock, titleblockimport | Plan and commit; `titleblockimport` declares a DXF read. |
| wire.rs | G/S | G: nets, net, wirenumber, potential, category, wiresettings, renumberwires, checks. S: wire, joinwires | Drawing wires snaps to the page's geometry with the drawing tolerance. |
| xref.rs | G/S | G: xrefs, contacts, mirrormove. S: refgoto | `refgoto` opens a page, zooms and selects. |
| plc/ | G/S | G: plcslot, plcracks, plcchannel, iolist, plcaddress, plcmaketags, plcexport, plcimport, plccardtype. S: plcchannels, plcpage | Channel cells are placed through the drawing's planner; `plcpage` creates a page view. |

## 2. The runner

```rust
// ecad-electrical::cmd::run (Core: no engine, no session)
pub struct Cx<'a> {                    // what a generic command may read and change
    pub doc: &'a Document, pub ids: &'a mut IdAllocator, pub page: PageId,
    pub tag_high: &'a mut TagHigh, pub uses: &'a mut designations::Uses,
    pub derived: &'a mut Derived,      // report models, built on demand
    pub conflicts: &'a mut Vec<Conflict>, pub last_caption: &'a mut Option<String>,
}
pub struct Outcome { pub edit: Option<Edit>, pub answer: Value }      // Edit { label, ops }
pub enum Page { Available, Any }       // refuse an unavailable page, or not (inspect, open)
pub enum Io { None, Read(&'static str), Write(&'static str) }   // the path parameter, by name
pub trait Generic: DeserializeOwned + JsonSchema {
    const ID: &'static str; const TITLE: &'static str; const HELP: &'static str;
    const PAGE: Page = Page::Available; const IO: Io = Io::None;
    const ECHO: bool = false;          // echo the answer's `message`
    fn run(self, cx: &mut Cx<'_>, io: Files) -> Result<Outcome, CmdError>;
}
```

- **Params:** the command's `#[derive(Deserialize, JsonSchema)]` struct moves next to its rules in
  ecad-electrical and implements `Generic`. Parsing and refusal wording (`cmd::input`) move with it.
- **Runner (ecad-engine `commands.rs`):** `generic::<P>()` builds the `CommandSpec` (id, title,
  help, `.noundo()`) and the `Typed` entry. Its run: parse, lock the project (`try_lock`), check
  the page, do the declared reads, call `P::run`, commit `edit` as one transaction under `P::ID`
  (`apply_edit`: undo, views, history), do the declared writes, echo, answer.
- **File I/O:** `Io` names the path parameter; the runner reads it (bytes, refused in one wording)
  or writes the `Outcome`'s bytes after the commit. Rules never touch the disk.
- **MCP schema:** `schema::<P>()` from the same struct, so `schema_of(id)` is unchanged.
- **Agent gate:** unchanged and outside the command: `ecad-mcp`'s policy and the engine's
  write gate see the same id and the same `Author`; the runner records the session's author.
- **Interactive forms** stay in `interact`; the engine table adds `.interactive(f)` to a generic
  entry. A session command keeps its engine file and the `Command` trait.
