# Revisions and release

A **revision** (Rev A, Rev B…) marks the project as issued: the title blocks show it, the revision
list records it, and the project as it was then can be opened read-only at any time. A **release**
does it all in one go: it checks the project, issues the pending revision and writes a folder you
can send, with the PDF book and every list stamped with that revision.

Revisions are issued on main only (see [history.md](history.md)), and the project must have been
saved once: `Save the project first, so the revision is kept with it and the folder has a place.`

## The Revisions window

Project menu (the project name at the top left) ▸ **Revisions**. The table lists every issued
revision (Rev, Date, Description, Drawn, Checked, Approved, **Folder**) and the pending one, whose
fields you fill in place. Rev, Description and Drawn are needed: until they are filled, the issue
buttons say `Fill in Rev, Description and Drawn first.`

- **Issue Rev B…** asks `Issue Rev B? The project as it is now is kept as "Rev B" and can be opened
  read-only.` **Issue** issues it as one step; the title block's revision box shows `B` and the
  revision list gains a row.
- **Release Rev B…** checks, issues and writes the release folder (below).
- **Reopen** on an issued row opens that revision read-only: a bar names it with **Back to
  current**, and the Book shows its pages as previews.
- An issued row's **Folder** cell has **Open** (the folder in the file manager) or **Write…** (write
  its folder again, below).

Typed: `revision {"description":"First issue","drawn":"CV"}` (the pending revision's
fields; custom fields go in `"fields"`), `issuerevision {…}` (the same fields, then issue),
`openrevision {"rev":"A"}`, `closerevision {}`.

## Pending changes

Once the drawing changes after the newest issue, the title block's revision box shows a marker.
**Settings ▸ Pages and sheets ▸ Revisions ▸ Pending changes** chooses it: **D*** (the last rev with a
star, the default), **E (draft)** (the next rev, marked draft) or **Off**. Undoing back to the
issued state removes the marker; filling in the next revision's fields does not count as a change.

**Columns** there sets the revision table's columns, with your own fields such as an ECR number:
`rev, date, ecr:ECR, description, drawn` (each `field[:Header][@width]`). A list that does not read
says why under the field and is not written. A custom field then has an input in the pending row.

Typed: `projectprops {"revision_pending":"draft"}`,
`projectprops {"revision_columns":"rev, date, ecr:ECR, description, drawn"}`,
`revision {"fields":{"ecr":"ECR-114"}}`.

## Release

Open the Release dialog from **Publish ▸ Release Rev B…** (foot of the left column), the History
panel's **Release B…**, or the Revisions window's **Release Rev B…**. The dialog `RELEASE REV B`
shows:

- the pending revision's fields (Date, Description, Drawn, Checked, Approved);
- the **Folder**: `release\Rev B` beside the project file, unless you type another;
- **Writes**: what will be written, and why a list is left out (`No terminal strips in this
  project.`);
- **CHECK**: the pre-release check (below). Click a row to show its place, ringed; the dialog stays
  open while you fix things, and the check follows.

**Release Rev B** is enabled once Rev, Description and Drawn are filled. Releasing closes the
dialog, saves the project and says `Rev B released to …: n sheets, n lists, … listed.`

The folder holds:

- `<project> Rev B.pdf`: the whole book, with the title blocks' revision box reading `B`;
- `… BOM.csv`, `… wires.csv`, `… cables.csv`, and, when the project has them, `… terminal plan.csv`,
  `… cable schedule.csv`, `… PLC racks.csv` and `… IO list.csv`;
- `… checks.csv`: every row of the check with its severity, group, kind, place and whether it was
  accepted;
- `release.json`: the revision, its point in the history, the app, and each file's SHA-256.

## The pre-release check

The check is the project's Checks, grouped for a release: **Errors**, **Open ends**, **Orphans**
(such as a cable reference with no other end), **Provisional** (provisional tags and wire numbers),
**? parts** (devices with no part number) and **Other**. Each group shows its first rows, with
**Show all n in Checks**. A clean project says `Nothing open. Ready to release.`

- **Errors stop a release.** Tick `Release with n (… listed in the folder's checks.csv)` to go on
  anyway; the folder's `checks.csv` records that they were accepted.
- **Provisional tags or wire numbers**: **Finalise numbering now** numbers them first; or release
  them as they are.
- Warnings and notes never stop a release; they are listed in `checks.csv`.

What each check means is in [checks.md](checks.md).

Typed: `prerelease {}` (the check as JSON, with what `release` would write or refuse; read only),
`release {}`, `release {"accept":"errors"}`, `release {"provisional":"finalise"}` (or `"issue"`),
`release {"folder":"C:/tmp/out/Rev B"}`, `release {"colour":"mono"}`.

## Write an issued revision's folder again

Click **Write…** in an issued revision's Folder cell (or **Open** after its folder was moved away):
`WRITE REV A'S FOLDER` writes the folder from Rev A as issued and records nothing. It also works on
a read-only project and on a branch.

A folder that already exists is never overwritten silently. The dialog asks:

- **Add missing files** (only when the rest is the same);
- **Replace**: the old folder is moved to `release\replaced\Rev A <date time>`, never deleted;
- **Keep both**: writes `Rev A (2)`;
- **Cancel**.

Typed: `release {"rev":"A"}`, `release {"rev":"A","existing":"keep-both"}` (`add-missing`,
`replace`).

## Undo an issue

A plain `u` (or Ctrl+Z) that would take back an issue stops and asks
`Rev A was issued on <date>. Undo the issue? This removes the release mark.` **Undo the issue**
takes it back; **Cancel** keeps it. After a release the folder stays on disk.

Typed: `undoissue {}` (undoes the last step when it issued a revision).

## Limits

- No transmittal, zip or upload; one folder layout; no DWG, DXF or XLSX in the folder.
- No approval or signatures: Checked and Approved are text fields. Settings ▸ Project info's
  people and approval card is a placeholder.
- The revision letter schemes in Settings ▸ Project info are a placeholder.
- No folder per branch.
