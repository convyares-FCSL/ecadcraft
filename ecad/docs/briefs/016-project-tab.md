# Brief 016: Project tab: see and switch pages

Status: Approved (owner, 2026-11-08: "move onto project tab")
Branch: `ecad/project-tab`
Designer: **yes** (design spec first, in `docs/design/016.md`) · Size: M
Depends on: briefs 009, 010 and 011 (the stack in PRs #29, #28 and #27). This branch starts on top
of `ecad/agent-mcp`, and is rebased onto `dev` as they merge.

## Goal

Seeing and switching the project's pages is one click, not typing `openpage` with an ID copied out
of `ecadinspect`. The owner's words (2026-11-07): "a project tab, so you can switch pages will be
needed soon".

## What exists today

- **Pages** are model objects: a title, and a place in `page_order`. Every drawing item names its
  page. One `.ecad` holds every page.
- **Opening a project** shows only page 1 as a drawing tab. Other pages open with `openpage
  {"page": id}`, or by following a reference with **Go to Reference** (brief 010). Closing a page's
  drawing tab hides the page until it's reopened.
- **Page commands:**
  - `newpage {title}`;
  - `renamepage {page, title}` and `movepage {page, index}` (brief 010; each change is one
    undoable transaction);
  - deleting a page: check what exists, and whether it refuses a page that still holds objects.
- **The UI already has** the ECAD tab in Tool Sets (Components, Wiring, Edit, Pages, Inspect) and
  the read-only debugger panel on the right (`apps/ecad-trial/src/debugger.rs`). CADCraft gives
  drawing tabs along the top.

## Scope

1. **A Project panel** that is always available, dockable or collapsible as the designer decides. It
   lists the project's pages in `page_order`, each with:
   - its number and title;
   - whether it's open as a drawing tab;
   - the current page highlighted.
   One click opens the page (as `openpage` does) and makes it current. A page that's already open
   is brought to the front, never opened twice.
2. **Page actions from the panel,** all through the existing commands, so each change is one
   transaction and one undo step, and agents (MCP) get nothing new:
   - **New page**;
   - **Rename** (in-place edit, `renamepage`);
   - **Reorder** (drag, or move up and down, `movepage`);
   - **Delete**, if a delete command exists and refuses or warns as it does today. If none exists,
     the designer says so and it stays out of scope.
3. **Live.** The panel follows undo, redo, agent edits, reopening and page changes made by command,
   with no refresh button. Reference texts (`/2.1`) follow renames and moves, as brief 010 defines.
4. **Cheap at scale.** The large benchmark project has 500 pages: the panel must stay responsive
   (virtualised list or equivalent). It reads `page_order` and page titles only. No per-frame work
   may grow with the number of items in the project.
5. **The designer decides, in `docs/design/016.md`:**
   - placement (left dock, a tab beside Tool Sets, or another place), width, and how it collapses;
   - how open versus closed pages, and the current page, look;
   - interactions: click, double-click, rename, drag reorder, keyboard (up/down, Enter, F2);
   - what else the panel could later show (cables, components, references), noted as future, not
     built;
   - screenshots needed;
   - everything with click-only steps, as for briefs 008–010.
6. **The quick start:** a "Project panel" section with click-only steps.

Out of scope: lists of cables, components or references (future); page templates and title
blocks; multi-project tabs; any `.ecad` format change or new op.

## Acceptance criteria

1. The panel lists every page in order, and a click opens or fronts the page. Tested headless by
   clicking labels, as in the brief 008 to 010 tests.
2. New, rename and reorder from the panel record exactly the transactions their JSON forms record,
   with one `u` each. Undo and redo update the panel.
3. A page changed by command or MCP shows in the panel without any other action.
4. 500 pages (the large benchmark project, through `--keep`): the panel opens and scrolls without a
   measurable cost per frame. Add a bench or test figure for it.
5. Closing a page's tab leaves the page listed, and one click reopens it.
6. Reference screenshots, as the design lists them, approved by the owner.

## Needs owner check

- Approve the design spec before the worker starts.
- A click-only trial of the panel on `demo.ecad`.
