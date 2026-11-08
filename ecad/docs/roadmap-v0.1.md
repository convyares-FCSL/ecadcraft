# Road to ECAD v0.1

Status: **Agreed direction** (owner, 2026-11-08: scope yes, IEC, A3, start both lanes). Sizes are
estimates and are revised after F1 and F2.

## What v0.1 means

**One real, small control panel drawn end to end in ECAD, printed, and its lists produced**, with
no other tool needed. For example: a few pages of power and control, perhaps 20 to 40 devices, a
couple of terminal strips and a handful of cables.

v0.1 is a working tool for one designer, not a product release. Anything that panel doesn't need
waits.

## What the target looks like

The owner's reference is an earlier panel book made in another ECAD tool. It is confidential, so
it is not stored in this repository; only its general shape is noted here. ECAD aims for the same
output, done better:
- **Frame:** A3 pages with a lettered column grid (A–H) and numbered rows (1–6). Every reference
  reads `page-column-row`, for example `110-B-5`. Page numbers are chosen by the designer (`100`,
  `105`, `110`…), not counted.
- **Title block:** a revision table with status, function (`=`) and location (`+`), project and
  drawing title, drawing number, and this, next and total page.
- **Conductors:** each one is drawn as it is wired. Its label shows the wire number, cross-section
  and colour (`L-01 2.5 Brown`), and every conductor on one potential shares the number (`24V-07`).
  Cable cores are labelled `W01 / BRN / 1.5` at their ends.
- **Devices:** IEC tags (`-K01`, `-Q03`, `-TB01`), dashed location boxes (`+09-02-A001`), contact
  references under contacts (`110-B-5`), contact mirror tables under coils, PLC cards with
  channel labels.
- **Generated pages:** cover, revision list and contents, BOM by location, a graphical page per
  terminal strip (links, cables and cores), a cable-type list and a cable list (from and to
  location, maker, reference, length).

## Where we are (2026-11-08)

Done, in `main`:
- one-file projects with pages, undo and redo, and safe saving on Windows and Linux;
- components drawn from lines, with terminals, and wires that stay connected;
- cables: slash, traditional and fan views, the look, cores, and cross-page `/page.column`
  references;
- MCP agent access;
- fast edits (about 1 ms on 500 pages);
- a cheap test setup (brief 017, merging now).

## Features to v0.1

Sizes are rough: agent working days, not counting your reviews. S is 1–2 days, M is 3–5 and L is
1–2 weeks. The **lane** says what can run at the same time. Lane A is mostly UI and drawing, lane B
mostly model and files, and they rarely touch the same code.

| # | Feature | What "done" means for v0.1 | Size | Lane | After |
|---|---|---|---|---|---|
| F1 | **Project manager** (016, under way) | Project panel, one active page, read-only preview, rename, reorder and new page from the panel | M | A | none |
| F2 | **Components, tags and starter symbols** (brief 018) | Every component has a tag (`-K1`, `-Q3`, `-S2`) that is one identity wherever it appears. Properties (description, part number, maker). About 15 starter symbols built in (the `.ecadlib` library file follows after v0.1, unless the design shows it is cheap): contactor, relay coil and contacts, MCB, fuse, PSU, push button, selector, lamp, motor, terminal, PLC input and output, transformer. Placed from a search box (name, tag prefix) | L | B | none |
| F3 | **Device cross-references** | Coil and contact references (`/3.4`), a contact mirror table under each coil, contacts over or under the coil's count flagged. Built on the general reference index (target to reference, agreed for components) | M | B | F2 |
| F4 | **Connectivity and wire numbers** | Nets (potentials) from what's drawn (T-junctions need a dot, crossings never connect). Conductors stay individual, drawn as wired; the **net carries the wire number** and every conductor on it shows it, with cross-section and colour (owner, 2026-11-08). Open ends and orphans listed | M–L | A | F1 |
| F5 | **Terminal strips and cable schedule** | Terminal strips (`-X1:1…n`) as devices. Cables track used and spare cores. A terminal plan and a cable schedule derived from the drawing | M | B | F2, F4 |
| F6 | **Page frame and outputs** | A3 frame (columns A–H, rows 1–6) and title block. References move to `page-column-row`. PDF of all pages. Generated pages for contents, BOM by location and cable list, and CSV of the BOM, wire list, cable schedule and terminal plan | L | A | F4 (lists need nets) |
| F7 | **Release, basic** | A pre-release check (open ends, orphans, `?` parts). Named revision (Rev A). PDF and CSVs written to one stamped folder | S | A or B | F6 |

**Order with two lanes:**
- **Lane A:** F1 → F4 → F6 → F7.
- **Lane B:** F2 → F3 → F5.

**Maintenance:** the upstream CADCraft sync and its policy slot in after F1, in lane A, before
F4. It is about M in size, and 473 commits have built up so far.

**QA gate:** one at v0.1, plus one mid-way after F1 and F2 if they help.

## Not in v0.1 (later, in rough order)

- XLSX export (CSV first), labels and DWG export.
- Search by sketch, harvesting symbols into libraries, update-available for library symbols,
  copy and paste between projects.
- A separate parts catalogue (v0.1 keeps part data as component properties).
- DWG import and `.wdp` adoption.
- AI changes as proposals, and AI-assisted drafting beyond today's MCP.
- Named versions beyond one revision, and compaction and checkpoints (stage 4).
- The 010 look pass and the owner's QA notes from 2026-11-08 (fitted into F1 or F6 where they touch).

## Owner decisions (2026-11-08)

1. Scope: yes.
2. Standard: IEC 81346 tags and IEC 60617 symbols.
3. Wire numbers: conductors are drawn individually, as wired; a net carries the wire number.
4. Parts: as component properties for v0.1.
5. Page format: A3.
6. Lanes: start both now (F1 and F2).
