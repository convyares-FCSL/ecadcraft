# 041: the wheel, the intent line and the seven tool groups (U4b)

What brief 041 changed, and how to try it by clicking. Listed in the index of
`docs/quick-start.md` (phase-end tidy-up after M1).

## What changed

- **One action table** (`ecad-cadcraft/src/actions.rs`) says where each existing command appears:
  in a tool group, in a slot of an object's wheel, on a key, and under which words Ctrl+K finds
  it. It defines no command; MCP's `list_commands` is unchanged.
- **Tool groups** (V W P C D B T) list only tools whose command exists today:

  | Group | Tools |
  | --- | --- |
  | Select (V) | Select |
  | Wire (W) | Wire |
  | Place (P) | Symbol, Terminal |
  | Cable (C) | Cable slash, Continue cable |
  | Draw (D) | Line, Polyline, Rectangle, Circle, Arc |
  | Box (B) | Location box |
  | Text (T) | Text, Note, Revision cloud |

  A group button shows its last-used tool; its key starts that tool, and pressed again while it
  runs steps to the next one.
- **The wheel.** Right-click an object on the page with no tool running: its wheel opens round the
  cursor with a chip naming it. Press, move and release over a slot, or click to open and click a
  slot. Bottom deletes, right goes to what it relates to, bottom right shows its row; the left
  side copies, turns or changes it. A greyed slot says why on hover and offers the next step.
  Releasing or clicking outside, or Esc, closes it. Right-click a page in the Pages tab for the
  page's wheel (Open, Rename, Page properties, Preview, New page, New folder).
- **The intent line** replaces the command line, at rest and while a tool prompts: the prompt, its
  keywords as chips you can click, what you type, and for Wire and Connect what the click will
  join ("joins A1 of -K01"). A refusal shows what, why and **Show it**.
- **Keys:** Del, F2 (edit the tag), R, X, Ctrl+C, Ctrl+V, Ctrl+D (duplicate), Ctrl+Z, Ctrl+Y act on
  the selection; Ctrl+K searches pages, tags, symbols and actions, and Enter or a click runs an
  action on the selection or starts its tool.
- The old ECAD Tool Sets tab (developer switch only) keeps only buttons no action covers.

## Try it

1. Start the trial (see `docs/quick-start.md`), with the developer switch off.
2. Press **P**, type `coil`, Enter, click on the page: `-K01`. Esc.
3. Press **W**: the bottom line reads `WIRE: …` with its keywords. Hover `-K01`'s A1: it adds
   "joins A1 of -K01". Click A1, type `@40,0`, Enter, then right-click: the wire is written and no
   wheel opens.
4. Right-click the coil: the device wheel opens with the chip `-K01`. Hover **Go to contacts**: it
   is greyed and says why. Click **Delete device**: one undo step (Ctrl+Z brings it back).
5. Right-click the wire: **Set potential** opens a short list.
6. Ctrl+K, type `fix number`, click the action row with the wire selected: Fix number starts on
   that wire.

Checked by the automated tests (`cargo test -p ecad-trial wheel_ui`, `-p ecad-ui wheel`,
`-p ecad-cadcraft actions_contracts`); the steps above have not been tried by the owner yet.

## Not in this brief

Lasso, Branch, Potential or bus line, Jump, Connector, Continuation arrow, Function box and Device
or PLC outline (no command yet); a wire's **Show row** (no Wires list yet); the electrical intent
for Place symbol and Cable slash (the line shows their prompt); a form for a terminal's **Edit
label** (it opens the Text panel's command with the terminal filled in).
