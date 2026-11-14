# Brief 009: cable look and cores

Part of the [quick start](../quick-start.md). A section number refers to a section of this file
when one is headed with it here, otherwise to the quick start's (1–10, 5A…).

## What this brief changed

**Branch `ecad/cable-look` (brief 009, design `ecad/docs/design/009.md`)**, cut from `dev` after
brief 005 was merged, restyles the fan to read like a cable on an electrical drawing (a label box,
a thick trunk and spines, curved bends and thin tails with each core's label and wire number),
adds **Core Label** to the ECAD tab (a core number or colour per wire in a cable, shown just past
the slash in traditional view and at the tail start in fan view), and draws brief 010's cable
reference end as a marker where the fan stops (section 5C). At your request of 2026-11-07 the
trunk joins the middle of each spine, and **Cable Join** slides it up or down the spine. Nothing outside `ecad/` changes, and
the file format is unchanged: a core label is the existing member `core` field, set with the
existing member ops. For this branch there are automated checks only, written with the branch,
and reference screenshots rendered headless for the owner to approve
(`ecad/apps/ecad-trial/tests/reference/009-*.png`, and `005-fan.png` redrawn in the new look);
nobody has run it on a display yet. In this combined trial with brief 010 the marker shows 010's
live reference text (such as `/2.1`, or a red `/?` while the end has no other end).
