# Brief 018: components and tags

Part of the [quick start](../quick-start.md). A section number refers to a section of this file
when one is headed with it here, otherwise to the quick start's (1–10, 5A…).

## What this brief changed

**Branch `feature/components` (brief 018, design `ecad/docs/design/018.md` with your answers of
2026-11-08)** makes placed symbols real devices with tags (section 5E). **Place Symbol** now asks
which symbol first (18 built-in IEC-style starter symbols: coil, contacts, MCBs, fuse, PSU, push
buttons, lamp, motors, transformer, terminal, PLC channels); each new device gets the next tag
of its prefix (`-K01`, `-Q01`…, highest + 1, two digits by default), and a contact can be added to
an existing device (`-K01`'s coil on one page, its contacts on another). Tags and connection point labels
are drawn beside every symbol. **Find Symbol**, **Device Properties** and **Delete Device** are
new in the Components group. Nothing outside `ecad/` changes. Existing files are written as before, but
one model rule is relaxed (I14, your decision of 2026-11-08): a project with a contact added to a
device is refused by older builds, and so is one whose tag settings were changed (two optional
project settings, your decision of 2026-11-08). For this branch there are automated checks only (below),
written and run by the worker; nobody has run it on a display yet.
