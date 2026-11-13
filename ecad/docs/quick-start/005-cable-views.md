# Brief 005: cable views

Part of the [quick start](../quick-start.md). A section number refers to a section of this file
when one is headed with it here, otherwise to the quick start's (1–10, 5A…).

## What this brief changed

**Branch `ecad/cable-views` (brief 005, design `ecad/docs/design/005.md`)**, cut from `dev`
after brief 004 was merged, adds **Cable View** to the ECAD tab: one click switches a cable
between the traditional view (every wire and the slash) and a fan view (one magenta trunk that
fans out to the wire ends), and back (section 5B). The switch is presentation only: it writes no
transaction and never changes the project's change count. Each cable's view is remembered for you
outside the `.ecad` file, in `cable-views.json` in your ECAD settings folder (section 9). Nothing
outside `ecad/` changes, and the file format is unchanged. For this branch there are automated
checks only (below), written with the branch and run by the tester and CI; nobody has run it on a
display yet.
