# Brief 018: Components, tags and starter symbols

Status: Approved (owner, 2026-11-08: road to v0.1, feature F2, "start both")
Branch: `feature/components` (worktree `C:\dev\ecad-features\components`)
Designer: **yes** (design spec first, in `docs/design/018.md`) · Size: L
Depends on: nothing. It runs beside 016 (lane A), and the two rarely touch the same code.
Roadmap: `docs/roadmap-v0.1.md`, F2. F3 (device cross-references) and F5 (terminal strips) build
on it.

## Goal

A designer places real electrical devices, not drawn boxes. They pick a symbol from a search box
and drop it on the page, and it gets an IEC tag (`-K1`, `-Q3`, `-S2`). The tag is one identity
wherever the device appears: a contactor's coil on one page and its contacts on others are the
same `-K1`.

## What exists today

The model already has what this needs, so **no new op and no `.ecad` format change is expected**:
- `CreateComponent` and `DeleteComponent`;
- `LinkRepresentation`, `ModifyRepresentation` and `UnlinkRepresentation`: one component, many
  drawn representations;
- `EmbedDefinition`, with an origin, and `RemoveDefinition`;
- `AddTerminal` with a definition key (LIB-02);
- `SetProperty`.

The trial can already convert drawn lines into a component, add terminals, and place one coil
symbol (`Place Symbol`). If the design shows a new op or a format change is needed, the worker
stops and asks the owner (`ecad/CLAUDE.md` gates).

## Scope

1. **Tags (IEC 81346).** Every component has a tag:
   - a prefix by device kind (`-K`, `-Q`, `-F`, `-S`, `-H`, `-M`, `-G`, `-T`, `-X`, `-A`), then a
     number;
   - given automatically on placement (the next free number for that prefix) and editable;
   - duplicates refused with a clear message;
   - shown on the drawing beside every representation, and in the hover lens and the debugger.
2. **One device, many representations.** A placed symbol can be **added to an existing device**.
   For example, place a normally-open contact and choose `-K1`, so it becomes another
   representation of `-K1` with its own terminals from the definition: `13`/`14`, `21`/`22`.
   Moving or deleting one representation leaves the others. Deleting the device asks first.
3. **Starter symbols, built in.** About 15 IEC 60617 definitions ship with the app and are
   embedded into the project on first use, using today's `EmbedDefinition` with its origin:
   - a contactor or relay coil (`A1`/`A2`), and normally-open and normally-closed contacts;
   - a 3-pole power contact set;
   - an MCB (1-pole and 2-pole) and a fuse;
   - a PSU (L, N, PE, +, −);
   - a push button (NO and NC), a selector switch and an indicator lamp;
   - a motor (3-phase and single-phase);
   - a transformer;
   - a single terminal;
   - a PLC digital input and output channel.

   Each definition has named terminals, a default tag prefix and default properties. The
   `.ecadlib` library file stays out of scope (after v0.1).
4. **Properties.** Description, maker, part number and, for later, location (`+`) and function
   (`=`). Edited from the hover lens or a properties panel, as the designer decides. All through
   `SetProperty`.
5. **Search and place.** One search box finds symbols by name, tag prefix or kind ("contact",
   "-Q", "fuse"), and devices already in the project by tag. Placing a symbol follows the cursor,
   with rotation and snap as today's `Place Symbol` does.
6. **Agents.** MCP can list symbols, place them and set tags and properties through the same
   commands. No new MCP-only paths.
7. **Quick start:** a "Devices and tags" section with click-only steps.

Out of scope:
- cross-reference texts and contact mirror tables (F3);
- terminal strips (F5);
- the `.ecadlib` file and its UI, a parts catalogue, search by sketch, and harvesting symbols;
- location boxes on the drawing (F6).

## Speed

Follows `ecad/CLAUDE.md` "Derived views scale with what changed". Tag lookups and the next-free
number come from an index, never a scan. Placing a symbol is in the same class as today's draw
edits on the medium bench (about 1 ms).

## Acceptance criteria

1. Place each starter symbol from the search box. Each gets the right prefix and the next free
   number, and its terminals carry the definition's names.
2. A coil and two contacts on different pages share one `-K1`. Renaming the tag updates all three
   at once, as one undoable transaction.
3. A duplicate tag is refused, with a reply that names the device already using it.
4. Save, close and reopen: tags, properties and the shared device identity survive, and the
   embedded definitions are in the file with their origin.
5. MCP places a symbol and sets a tag with the same results.
6. Worker checks: fmt, clippy and `cargo xtask test-changed`. No new slow tests unless marked.
7. Reference screenshots, as the design lists them, approved by the owner.

## Needs owner check

- Approve the design spec (`docs/design/018.md`) before the worker starts.
- A click-only trial: place a contactor coil and its contacts on two pages, rename `-K1`, then save
  and reopen.
