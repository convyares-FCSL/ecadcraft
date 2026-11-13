# Brief 010: cable references

Part of the [quick start](../quick-start.md). A section number refers to a section of this file
when one is headed with it here, otherwise to the quick start's (1–10, 5A…).

## What this brief changed

**Branch `ecad/cable-references` (brief 010, design `ecad/docs/design/010.md`)**, cut from `dev`
after brief 005 was merged, lets one cable be drawn on two pages (section 5D). **Cable Reference**
marks where a cable leaves a page; **Continue Cable** draws the same cable on another page (short
wires that are the same wires, and a slash); each end shows the other end's page and column, such
as `/2.1`, worked out every time the page is drawn, so renumbering or moving pages never breaks
it. **Go to Reference** jumps between the ends, and **Rename Page** and **Move Page** are new in the
Pages group. Nothing outside `ecad/` changes, and the file format is unchanged. The decisions the
overseer took on the design's open questions while you were away (one conductor drawn on two
pages is one core, two references per cable, no continuation on the same page) are listed in the
pull request for you to confirm. For this branch there are automated checks only (below); nobody
has run it on a display yet. In this combined trial the marker is brief 009's flag around that
text, with 010's ring on the point.
