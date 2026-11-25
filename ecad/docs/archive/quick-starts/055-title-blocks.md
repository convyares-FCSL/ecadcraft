Historical record (2026-11-14). Not current; see [user/pages.md](../../user/pages.md).

# Brief 055: Library ▸ Title blocks

Part of the [quick start](../quick-start.md). What brief 055 changed and how to try it by clicking.

**The built-in border changed in 0.2.2; pages keep their positions.**

## What this brief changed

**Branch `feature/title-blocks`**, cut from `dev` at `8b6c2ccd`.

| Where | What is there now |
| --- | --- |
| Every page | The built-in frame (`sheet`, "Schematic sheet") is the house block: zones labelled on all four sides, centre marks, and a 40 mm title block with a revision table (Rev, ECR, Date, Description, Drawn, Checked, Approved; newest at the top), a logo box with the company line, Project, Customer, a red **Status**, Title, Drawing no., Rev (with 033's pending marker), Function, Location, Size, and Page, Next and Of counters. The block is 40 mm high, as before, so no device, reference, wire or table moves |
| Revision columns | The built-in columns gain **ECR** after Rev. A project that set its own columns keeps them |
| Frames | A frame style may carry its own title block, and set the sheet it is drawn for (`sheet`), its margins (`margins`, `l,r,b,t`), centre marks and where the block sits. An own frame with a sheet decides its pages' paper and zones; a page whose own sheet differs gets a Checks warning with **Clear the page's sheet** |
| Library ▸ Title blocks | The editor: the frames on the left with **+ Title block**, **Import DXF** and the field palette; tools Select, Line, Text, Field, Logo, the Sample and Snap switches, **Frame…**; the block at print size; Value, Look and Box on the right. Edits go to a draft; **Save** writes one step after "Saving changes N pages." |
| Logos | Vector logos from a DXF, fitted to their box. PNG and JPEG logos come in 0.4.0 |
| Commands | `titleblocks` (read), `titleblock` (read, write the whole record, or `new`), `titleblockimport` (a block or a logo from a DXF) |
| Checks | `title-value-cut`, `title-binding-unknown`, `frame-sheet-differs` |

## Try it

1. Open the demo. Expected: every page shows the new border and block; devices sit where they did.
2. **Library ▸ Title blocks.** Expected: **Schematic sheet** is listed as built-in, read only, with
   "Built-in. Duplicate it to make your own." on the right.
3. **+ Title block ▸ From the built-in, for A3.** Expected: "A3 title block" opens with the house
   block, editable.
4. Drag **Project field…** from the palette, type `job_no`, **Add**. Drag the new field into
   place. Expected: it snaps to whole millimetres.
5. Select the Status field: Look shows **Red** and **Bold**. Select the logo box, then **Replace…**
   and type the path of a DXF. Expected: the drawing fits the box.
6. With nothing selected: "Saving changes 0 pages." **Save**.
7. Right-click the row ▸ **Use for this project**. Expected: every page shows your block. Set
   `job_no` in Settings ▸ Project info (or `projectprops {"props": {"job_no": "J-7"}}`) and the
   pages show it. `u` undoes each step.
8. **Frame…**: choose A2, **Apply**, **Save**. Expected: the pages turn A2 with 12 × 8 zones.

Known limits: Frame… edits a frame's own sheet only (portrait and A4 come with 054). The editor's
DXF prompts take a typed path.
