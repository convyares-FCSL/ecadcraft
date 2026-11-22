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
// ecad-electrical::cmd::run (Core: no engine, no session, no disk)
pub trait Generic: DeserializeOwned + JsonSchema {
    const ID: &'static str; const TITLE: &'static str; const HELP: &'static str;
    const PAGES: Pages = Pages::Available;       // or Any (also an unavailable page)
    const ACCESS: Access = Access::Edits;        // or Reads: the agent gate lets it run with writes off
    const IO: Io = Io::NONE;                     // Io { reads: &[(param, max bytes)], writes: &[param] }
    const STATE: State = State::None;            // state only this command uses, e.g. LastCaption
    const ECHO: Echo = Echo::Nothing;            // or Message: echo the answer's `message`
    fn check(&self) -> Result<(), CmdError> { Ok(()) }   // before the project is taken
    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError>;
}
pub struct Cx<'a> { doc, ledger, ids, page, tag_high, designation_uses (uses()), derived,
                    read: &BTreeMap<param, Result<bytes, Unread>>, state: Slot<'a> }
pub struct Outcome { edit: Option<Edit { label, ops }>, answer: Answer, writes: Vec<(param, bytes)> }
pub enum Answer { Now(Value), Committed(Box<dyn FnOnce(&Document) -> Value>) }
```

- **Params:** the command's `#[derive(Deserialize, JsonSchema)]` struct lives next to its rules
  in ecad-electrical (schemars is a plain Core dependency, owner 2026-11-21). Reading and refusal
  wording are `cmd::input`.
- **Cx holds only shared state.** State one command uses is declared (`STATE`) and handed to it
  as a `Slot`; the engine puts it back when the edit is not committed.
- **Runner (ecad-engine `cmd::generic`):** `spec::<P>()` registers it (`.noundo()`; the table may
  add `.interactive(f)`), `typed::<P>()` lists it. `run`: parse, `check`, read the declared files,
  lock (`try_lock`), check the page, `P::run`, commit `edit` under `P::ID` (`apply_edit`: undo,
  views, history), write `writes` to the paths their parameters name, echo, answer.
- **File I/O:** declared as data; the engine alone opens files. Core has no `std::fs`.
- **MCP schema:** `schema::<P>()`, so `schema_of(id)` is unchanged.
- **Agent gate:** derived from the declaration: `ACCESS` (read commands run with writes off;
  `policy::is_read_command`) and `IO` (file parameters refused from an agent). The hand-listed
  `READ_COMMANDS` and `COMMAND_FILE_KEYS` keep only session commands and commands not moved yet.
- A session command keeps its engine file and the `Command` trait (`cmd::input`).
