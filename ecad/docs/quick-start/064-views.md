# 064: one update pipeline for derived views (R9)

Nothing to click: this lane changes how the program keeps its derived views (references, checks,
reports, the Devices and Properties feeds, the page tree) up to date after an edit, not what a
person sees. Every op is classified once (`ecad_model::effect::Effects::of`), and the three commit
paths (a command, a drawing edit, undo and redo) all run the same `Views::begin` / `finish` and
`Project::apply_changes`. Try any edit, undo and redo as before: the drawing, the lists and the
panels should look exactly as they did.

Progress (automated checks only, not owner-tried):

- Step 1: `project.rs` split into `project/` (a pure move).
- Step 2: `Effect` and `DerivedView` in ecad-model; an empty `Views` in ecad-electrical, wired into
  all three paths; `Document: Sync` holds (asserted at compile time), so the `parallel` feature of
  step 12 is possible. The views themselves arrive one per step as the type lanes they read land
  (design 072 §12.3).
- Step 3, first feed: the Pages tree feed (`TreeFeed`) decides when the Pages panel reads its list
  again. To try it: rename a page, move one into a folder, and undo; the Pages panel follows each
  change. Changing only a page's display settings leaves it alone. The Devices and Properties
  feeds follow once lane 072 B has moved device data into fields.
