# Projects

A project is one `.ecad` file: every page, device, wire and cable, its settings and its whole
history. ECAD has one project open at a time.

## New, open, save, close

All of these are in the **Project menu**: click the project name at the far left of the top bar.

| Item | Does |
| --- | --- |
| **New project** | A new, unsaved project; an open one is closed first, as **Close project** does |
| **Open project…** | Lists the `.ecad` files in a folder to pick from, or takes a typed path; an open project is closed first |
| **Save** (Ctrl+S) | Saves; asks for a file name the first time, or when the project is read-only |
| **Save As…** | Saves under a new name; the old file keeps its last saved state |
| **Close project** | Closes; with unsaved changes it asks first |

From the command line: `ecad-desktop <project.ecad>` opens a file at start;
`ecad-desktop` alone starts a new project.

**The file dialog** is ECAD's own: type a path and press Enter. `.ecad` is added when missing.
In **Open project…** the `.ecad` files of the folder in the path are listed below it: click one to
open it, or type another folder (ending in `\` or `/`) to list that one.
`~/` and `~\` mean your home folder (on Windows your user profile). The dialog suggests
`C:\dev\ecad-projects` on Windows and `~/ecad-projects` elsewhere, and the first save into that
folder creates it. No other missing folder is created: saving into one fails with a message and
writes nothing. On Windows, names Windows reserves (`CON`, `NUL`, `COM1`…) and drive-relative
paths such as `C:demo` are refused.

**The save state** is at the left of the bottom bar: **Saved** (with the time it became saved in
this session), **Unsaved changes (n)**, **Not saved yet** or **Read-only**.

**Closing with unsaved changes** asks `Save changes to project "<name>"?`: **Save** writes them
then closes, **Discard** closes and the file keeps its last saved state, **Cancel** keeps
everything open. Closing the window asks the same.

**Undo history is saved.** After reopening, Ctrl+Z steps back through edits made before the save.
Each undo and redo is itself a change to save.

## Locks and a second window

Opening a project takes a writer lock, a file `<name>.ecad.lock` beside it. It is normal; leave it.

- A second window (or a second program) opening the same file gets it **read-only** and says that
  another session is editing it. Save is refused there; Save As saves a copy under another name.
  On Windows it cannot name the other session.
- If the program is killed or crashes, the next open reclaims the leftover lock and says so; the
  file opens at its last save. Nothing saved is lost; unsaved edits are.
- Right after the other session ends (notably on Windows), the lock can take a moment to be
  released: open again shortly.
- A file replaced or deleted behind an open project is detected, and Save is refused.

## Read-only projects

A project opens read-only, with the reason, when:

- another session is editing it (above);
- no writer lock can be taken at all (for example the folder cannot be written); this is not
  reported as another session;
- the file itself cannot be written (a read-only file, or one you may not change);
- the file is damaged: it is kept exactly as it is;
- it is of **another format version** than this build's (older or newer);
- it holds words this build does not know (for example a kind of object that no longer exists);
  the message names the word, the setting and where it is.

On a read-only project edits still work in memory. Save asks for another file name, so Save As
keeps your work in a new file and leaves the original untouched.

## Older and newer files

The `.ecad` format is a development format: before version 1.0 ECAD does not convert files from
other format versions. Such a file opens read-only. Locations and functions from projects made
before they became a project list show `?`; see [Locations](locations.md).
Details of the format: [File format](../reference/file-format.md).

## Where to keep projects

Keep projects in a local folder: `C:\dev\ecad-projects` (NTFS) on Windows, your Linux home folder
on Linux and under WSL. Other places are unverified, and file locking and the save guarantees are
not known to hold there: FAT and exFAT drives (USB sticks), network shares, ReFS (including Dev
Drives), OneDrive folders, Windows drives under `/mnt` in WSL, and macOS.

## Project info and project settings

- **Project menu ▸ Project info** opens **Settings ▸ Project info**: title, drawing number,
  customer, status, the default function and location, and other properties, with a preview of
  the title block. It also holds the project's list of functions and locations
  ([Locations](locations.md)). **Save** and **Cancel** at the foot.
- **Project menu ▸ Project settings** opens **Settings ▸ Pages and sheets** at the project's own
  rows. Its sections:
  - the sheet and references (see [Pages](pages.md)), and contact tables;
  - **Tags:** digits, upper case or as typed, the leading `-`, and each symbol's prefix
    ([Devices](devices.md#tag-settings));
  - **Revisions:** the pending-change marker and the revision columns ([Releases](releases.md));
  - **Numbering:** **Provisional tags until Finalise** ([Devices](devices.md#provisional-tags));
  - **Wires:** colour spelling, label height, which wires get a label, and wire numbers per net
    or per wire ([Wires](wires.md#wire-settings)).

  A second tab holds the text styles ([Styles](styles.md)).
- **Project menu ▸ Revisions** opens the revision list ([Releases](releases.md)).
- **Project menu ▸ Settings** and **Standards** open Settings itself. Settings covers the window
  between the same top and bottom bars; Esc, **Back to Draw** or choosing a workspace leaves it,
  with the workspace, page and selection as they were. Its list: Project info, Standards, Pages
  and sheets, Libraries, Agent access, Versions and backups; then, for this computer, Appearance
  and Shortcuts.

## What is kept outside the project

Kept per person on this computer, never in the `.ecad` file: the window's size and place, the
panels and each page's zoom, the interface theme and sheet colour, symbol drafts, and which
cables are shown as fans ([Cables](cables.md#remembered-views)).

## Limits

- One project at a time. Only `.ecad` projects are saved.
- No native file dialogs: paths are typed in ECAD's own dialog.
- `saveas {"path": …}` in the command window saves only into a folder that already exists; only
  the dialog creates the default folder.
- No conversion between format versions, and no checkpoints.
