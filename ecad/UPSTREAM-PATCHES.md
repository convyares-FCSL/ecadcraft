# Files changed outside `ecad/`

Every file in the upstream tree that differs from the incorporated CADCraft commit
([`UPSTREAM.md`](UPSTREAM.md)) is listed here. `cargo xtask patches` fails if a changed file is
missing. Code patches are marked in the source with `// ECAD-SEAM(owner):`.

## ECAD-owned files outside `ecad/`

These are ours. On upstream merges, conflicts are resolved by keeping ours.

| File | Why |
| --- | --- |
| `README.md` | ECAD repository front page |
| `.github/workflows/ci.yml` | CI for `dev`: CADCraft workspace and ECAD workspace |
| `docs/concept-spec.md` | ECAD concept spec (moves to `ecad/docs/` once housekeeping R11 is approved) |
| `docs/decisions.md` | ECAD decision log |
| `docs/cadcraft-assessment.md` | Phase 0 assessment |
| `docs/document-architecture-proposal.md` | architecture proposal (PR #1) |
| `docs/ecad-architecture-foundations.md` | owner's foundations note |

## Code patches to upstream crates: the generic extension interface

Proposal §14, capabilities X1-X7, and X8 (Tool Sets tabs supplied by the embedder, approved
2026-10-15). One shape, one marker (`ECAD-SEAM(owner)`), nothing
ECAD-specific: the interface passes drawings, handles, command ids and provenance. Documents
without an owner behave exactly as before.

| File | Capability | What changes |
| --- | --- | --- |
| `crates/engine/src/ext.rs` | X2-X7 | **new file**: `DocumentOwner` trait, `Extensions` registry, start check (`refuse_start`, `Start`: the owner may refuse a command or a LISP evaluation before it starts, added with the 0.15.1 sync), commit gate, history/save/close/drag hand-over, provenance records, owner-aware `Session::doc_unsaved`, `close_needs_prompt`, `has_unsaved_work` |
| `crates/engine/src/lib.rs` | X2-X4 | `pub mod ext`; `Session::ext` field |
| `crates/engine/src/session/execute.rs` | X2-X4 | start check before the command runs; gate at the `execute` commit point, before LISP undo grouping, with no engine undo step for owned documents; `undo`/`redo` hand-over |
| `crates/engine/src/session/interactive.rs` | X2, X3, X7 | start check before an interactive command begins; gate at the `finish` commit point (after the transparent-command return, before LISP undo grouping); a new top-level interactive command starts with nothing recorded; drag preview contribution |
| `crates/engine/src/lisp.rs` | X2 | start check in `lisp_start`, the one entry of every LISP evaluation (command-line expressions, loaded files, `c:` commands), before the drawing changes |
| `crates/engine/src/cmd.rs` | X1 | `set_extension_commands`: embedder commands appended to the registry |
| `crates/engine/src/cmd/file.rs` | X5 | QSAVE/SAVEAS hand-over; CLOSE/CLOSEALL tell the owner, and stop with the document still open when the owner fails |
| `crates/engine/src/cmd/edit.rs` | X4 | UNDO options other than a count (Mark, Back, BEgin, End, Auto, Control) are reported as unsupported on owned documents |
| `crates/engine/src/autosave.rs` | X5 | owned documents are not autosaved as drawings |
| `crates/engine/src/cmd/modify/transform.rs` | X6 | provenance for COPY and the copy forms of MOVE-family transforms (`transform_entities`) |
| `crates/engine/src/cmd/clipboard.rs` | X6 | COPYCLIP reported with its serial; PASTECLIP provenance |
| `crates/ui-egui/src/closing.rs` | X5 | close and quit prompts ask the session (owner-aware) |
| `crates/ui-egui/src/chrome.rs` | X5 | file tab unsaved marker asks the session (owner-aware) |
| `crates/engine/tests/ext_owner.rs` | tests | **new file**: owner interface tests, including the start check (commands and LISP refused before the drawing changes; plain drawings unaffected; a panicking check refuses and recovers) |
| `crates/engine/tests/ext_commands.rs` | tests | **new file**: extension command registration test |
| `crates/ui-egui/src/palettes.rs` | X8 | Tool Sets tabs supplied by the embedder (`ExtraToolTab` of `ExtraToolGroup`s of `ExtraTool`s): shown after Drafting and Modeling, each with collapsible groups of text buttons; a click is queued, never started; without extra tabs the strip and palettes are unchanged |
| `crates/ui-egui/src/lib.rs` | X8 | `CadApp::extra_tool_tabs` (empty by default) and the click queue, `CadApp::take_tool_requests` (each click returned once) |
| `crates/ui-egui/tests/toolsets_extra.rs` | tests | **new file**: without extra tabs Drafting and Modeling are unchanged; an extra tab shows its groups, queues a click once without starting it, and keeps the selection; a tab named like a built-in one is ignored |

Not yet covered by provenance (copies through them are refused by the adapter until added):
ARRAY, grip copy (`gripcmds.rs`), MIRROR/ROTATE/SCALE through other code paths, PASTEBLOCK,
and any other command that creates entities from existing ones (see the census in the experiment
report). COPYBASE, CUTBASE and PASTEORIG go through the same clipboard copy and paste as
COPYCLIP and PASTECLIP, so they are covered.

Paths CADCraft 0.15.1 added that change a drawing outside the commit gate or keep engine undo
steps of their own are refused on owned documents by the start check instead (the ECAD adapter's
`REFUSED_COMMANDS` and `LISP_REFUSED`, owner decision 2026-10-31): the Block Editor (BEDIT, BSAVE,
BSAVEAS, BCLOSE), in-place reference editing (REFEDIT, REFSET, REFCLOSE), and LISP (expressions
typed at the command line, APPLOAD, the LISP command, `c:` commands and `(command …)` routines).
