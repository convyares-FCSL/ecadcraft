# Brief 027: Release (basic): pre-release check, named revision, stamped folder

Status: Approved (owner, 2026-11-12: Q1–Q11 as recommended; overseer approved FP1–FP2; design §17)
Branch: `feature/release` (worktree `C:\dev\ecad-features\sheets`, historical name), from `dev`
`dc2472c6`.
Designer: **yes** (`docs/design/027.md`) · Size: S–M
Roadmap: `docs/roadmap-v0.1.md`, F7 "Release, basic". Lane A, after 024 (outputs, in `dev`).
026 (numbering, provisional tags) and 028 (terminal strips, cable schedule) run beside it; 027
uses 026's Checks kinds and Issue choice when they are in `dev`, and leaves two named slots for
028's CSVs.

## Goal

One action turns the project into an issued revision and a folder that can be sent: the
pre-release check is shown first, the pending revision is issued as `Rev B`, and the PDF book and
every list are written, stamped with that revision, into one folder that says what it holds and
what was knowingly left open. Doing it again never loses a folder silently.

## What exists today

- Revisions as project properties `rev.<n>.*`; `issuerevision` issues the pending one in one
  transaction (`rev.<n>.issued`, `NameVersion "Rev B"` at the transaction before it), only on a
  release line (020); `undoissue`, `openrevision` (019, 020).
- Checks (022, 024, 025): kept, updated from each commit's ops; the `checks` command reads them
  with the report models. Kinds include open end, loose wire, lonely continuation, unnumbered
  nets, devices without a part number or location.
- Cable reference ends know when they are orphans (`/?`, 010), but no Checks row lists them.
- `printbook` and the BOM, wire and cable reports with CSV (024), each callable on any
  `Document` (`printbook::print`, `Outputs::build`, `outputs::csv`).
- On `feature/numbering` (026, not yet in `dev`): provisional tags, Checks kinds
  `provisional-tag` and `provisional-wires`, and `issuerevision {provisional: "finalise" | "issue"}`.

## Scope

1. **Pre-release check**, shown before anything is written: the kept Checks of the state to be
   issued, grouped as errors, release gaps (open ends, orphans, provisional tags and wire numbers,
   `?` parts) and information, each row with go-to or its fix. A new Checks kind lists orphan
   cable reference ends.
2. **Never a dead end.** Warnings never stop a release; errors need one explicit "release anyway";
   every refusal (unsaved, read-only, on a branch, folder not writable, missing revision fields)
   says what to do and offers it.
3. **Named revision:** releasing issues the pending revision through `issuerevision` (with 026's
   provisional choice), then saves the project, so the file and the folder agree.
4. **One stamped folder** (`release/Rev B/` beside the project by default): the PDF book, BOM,
   wire and cable CSVs, the check rows accepted at release, and a manifest naming the revision,
   the log point, the app and each file with its SHA-256. Built from the project **as issued**,
   so the title block reads Rev B. Written to a staging folder and renamed into place last.
5. **028 slots:** the terminal plan and cable schedule are listed as "comes with terminal strips
   (brief 028)" until 028 provides them; 028 adds them by filling a slot, with no change to the
   release flow.
6. **Re-release:** an issued revision's folder can be written again from its issued state at any
   time (nothing recorded in the project). An existing folder is never overwritten silently:
   replace (old one moved aside), keep both, or add only missing files.
7. **MCP:** `prerelease` (read) and `release` (write) with contract tests.
8. **Quick start:** a click-only "Release" section on the demo.

Out of scope: approval workflows and signatures; transmittals, zip or upload; per-recipient
packages; DWG/DXF in the folder; XLSX; a release folder per branch; diffing two releases page by
page; the terminal plan and cable schedule themselves (028).

## Format and ops

**No new op.** Proposed for the overseer (design §2):
- **FP1.** Project property `rev.<n>.released` (the folder, relative to the project file), written
  in the issue transaction when a revision is issued by releasing. Old builds keep it as an
  unknown revision field.
- **FP2.** `release.json`, a versioned manifest written in the release folder (an output file,
  not part of `.ecad`).

## Acceptance criteria

1. The Release dialog and `prerelease` show the check of the state to be issued, grouped and
   counted, with go-to and fixes; orphan cable reference ends are a Checks kind of their own.
2. A release with only warnings proceeds; one with errors needs the explicit "release anyway"
   (`accept: "errors"`); provisional items follow 026's finalise-or-issue choice. Every refusal
   in design §6 shows its way out.
3. Releasing the pending revision writes, in order: the outputs to a staging folder, the issue
   transaction (with FP1), a save, the manifest, then the rename. A failure before the issue
   leaves the project and disk unchanged; a failure after it keeps the issue and says how to
   finish the folder.
4. The folder holds `<stem> Rev B.pdf`, `… BOM.csv`, `… wires.csv`, `… cables.csv`,
   `… checks.csv` and `release.json`; the PDF's title blocks and revision list show Rev B; file
   hashes in the manifest match.
5. Writing an issued revision's folder again records nothing in the project and gives the same
   CSVs; an existing folder is replaced (old moved aside), kept beside, or completed, by choice.
6. MCP: `prerelease` works with writes off and records nothing; `release` records the issue with
   `Author::Agent(<client>)` and is refused with writes off; re-writing an issued revision records
   nothing (contract tests).
7. Old files open unchanged; save and reopen keep FP1.
8. Bench rows of design §11; no ordinary edit gets slower.
9. Reference screenshots, approved by the owner.

## Needs owner check

- Approve the design (`docs/design/027.md`) and its questions (§15).
- A click-only trial on the demo (quick start "5L. Release").
