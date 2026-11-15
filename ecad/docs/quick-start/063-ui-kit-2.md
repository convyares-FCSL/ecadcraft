# 063: UI kit, round 2

What changed: the dialogs and windows below now use the shared `ecad_ui` components (the same
boxes, buttons, headers and colours as the Draw inspector and Project panel). Nothing they do
changed; only how they look. The trial's `lib.rs` was split into smaller files (no change).

How to try it, by clicking:

1. **Developer ▸ Primitives**: the gallery's new section, *Shared pieces (round 2)*, at the bottom:
   folding section, property rows, list rows, text styles, checkbox, radio, search field and a
   dialog footer. Switch light and dark from the top bar.
2. **History**: New version, New branch, Rename, Archive, Switch: each is now a dialog over the
   dimmed page with the primary button at the right; Esc or the cross closes it.
3. **History ▸ Merge** a branch with a conflict: radios for the choices, the Edit choice in a
   field box, one warning colour.
4. **Project ▸ Renumber** (or Renumber Wires): What is a segmented choice; Prefix and Folder are
   selectors; Start and Digits are boxes.
5. **Settings ▸ Project** (Info and Settings): every value in a box beside a label column;
   sections captioned SHEET, REFERENCES, TAGS, REVISIONS, NUMBERING, WIRES.
6. **Publish ▸ Release** and **Revisions**, **Reports**, **Print Book**: the same boxes, checks
   in their tones, folding check groups.
7. **Developer ▸ Debugger**: kit text, buttons, search field and list rows; the view picker is a
   selector.
8. Close a project with unsaved changes, open a file, add a page: the close question, file box and
   page-name box are dialogs too.
