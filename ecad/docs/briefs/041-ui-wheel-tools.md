# Brief 041: Wheel, intent line and the seven tool groups (U4b)

Status: Approved (Carl, 2026-11-13).
Branch: `feature/ui-wheel`, from `origin/dev` (no wait: U2 and U3 have merged).
Designer: done (`docs/design/ui-architecture.md` §7, §11; the look and rules are spec §2 "Tool
groups", §5 "The radial wheel", "Keyboard that remains", "Refusals and errors") · Size: L, in
five steps
Roadmap: M1 **1.6** (radial wheel) and the tools part of **1.5**. Lane **U4b**; runs beside 040,
042 and 043.
Inputs: spec §2, §3 ("What exists today" table: where each old tool goes), §5, §6 step 5;
owner answers 7 (one key per group, stepping) and 8 (undo keys only); U0 Q4 (right-click while a
tool runs finishes the step; the wheel opens only when no tool runs).

## Goal

Tools only start new things; everything done to an existing thing is on that thing's wheel, in
the same place every time; and the click says what it will do before you make it. One command
registry feeds the wheel, the tool groups, the keys, Ctrl+K and the Text panel, so nothing is
defined twice.

## What exists today (`dev` after #74)

- ECAD's commands are CADCraft `CommandSpec`s (`ecad_cadcraft::commands::specs` plus
  `lanes.rs`); MCP runs exactly those (`execute`, `list_commands`).
- The tool strip (`shell/tool_strip.rs`) has the seven groups; their flyouts list today's ECAD
  Tool Sets buttons (`tools.rs` `TOOLS`, 39 entries, groups Components, Wiring, Numbering, Edit,
  Text) and some CADCraft drawing commands. Select's flyout also holds tools that act on existing
  things "until U4b moves them to the wheel"; a test checks every `TOOLS` id is in one flyout.
- Keys (`shell/keys.rs`): groups, Ctrl+1–4, Ctrl+K (opens today's symbol search), Ctrl+/,
  PgUp/PgDn, F, Alt+Left/Right, Esc, Del.
- Right-click on the page: US1's `canvas_menus` is off, so CADCraft opens no menu; the shell
  takes and drops `take_secondary_click` (`shell/mod.rs` `main_view`). While a command runs,
  right-click is Enter.
- **The command line still shows while a command prompts** (038 open point 2:
  `self.cad.ui.show_command_line = chrome || prompting`), because without it prompts and keywords
  were invisible. CADCraft's typed buffer (`CmdLine::buffer`) and the running prompt
  (`prompt_text`) are public, so the intent line needs no new seam.

## §0 Files touched

● owned by this lane; ○ shared, with the hunk named.

| File | Change |
| --- | --- |
| ● `ecad/crates/ecad-cadcraft/src/actions.rs` (new) | The action table (design §7.2): `Action { id, command, params, label, icon, group, wheel, key, search, enabled }`, `ObjectKind`, `Slot`, `ToolGroup`, `Enabled::{Ok, Greyed { why, next }}` |
| ● `ecad/crates/ecad-cadcraft/src/interact/kind.rs`, `interact/intent.rs` (new) | What is under the cursor (`ObjectKind` and its IDs, from the existing hover lookups); the intent sentence for the running tool |
| ○ `ecad/crates/ecad-cadcraft/src/interact/mod.rs` (two `mod` lines at the top only) | 040 edits `overlay_marks` in the same file |
| ○ `ecad/crates/ecad-cadcraft/src/lib.rs` (one `pub mod actions;` line) | |
| ● `ecad/crates/ecad-ui/src/wheel.rs` (new) | The wheel widget: 150 / 54 px radii, eight 41° slots with 4° gaps (`ecad-tokens::wheel`), press-move-release and click-click, greyed slots with their reason on hover, one short list (≤ 8) from a slot, cancel outside |
| ○ `ecad/crates/ecad-ui/src/lib.rs` (one `pub mod wheel;` line) | |
| ○ `ecad/crates/ecad-tokens/src/icon.rs` (variants appended in one block marked `// 041`) and `ecad/crates/ecad-ui/src/icons/tools.rs` | Icons for every action that has none |
| ● `ecad/apps/ecad-trial/src/shell/tool_strip.rs`, `shell/keys.rs` | Groups and flyouts from the action table; keys from it |
| ● `ecad/apps/ecad-trial/src/shell/intent.rs`, `shell/wheel.rs` (new) | The intent line; opening the wheel and running its action |
| ○ `ecad/apps/ecad-trial/src/shell/layout.rs` (one row: `intent`) | A 28 px intent strip at the bottom of the page's rect in Draw |
| ○ `ecad/apps/ecad-trial/src/shell/mod.rs` (`shell_ui`: the `show_command_line` line; `main_view`: the `take_secondary_click` line) | The command line never shows outside the developer switch; right-click opens the wheel |
| ○ `ecad/apps/ecad-trial/src/shell/top_bar.rs` (the search field's handler only) | Ctrl+K searches pages, tags, symbols and actions |
| ○ `ecad/apps/ecad-trial/src/project_panel.rs` (one `Action` variant appended: open a page row's wheel) | `fix/ui-followups` renamed `Action::GotoSymbol` to `GotoSymbols(Vec<…>)`; this lane appends a variant and does not touch that one |
| ● `ecad/apps/ecad-trial/src/search.rs` | Search over actions added beside today's symbol and page search |
| ● `ecad/apps/ecad-trial/src/tools.rs` | Old Tool Sets buttons removed group by group as actions cover their commands; the ECAD Tool Sets tab (developer switch only) keeps what remains |
| ● `ecad/crates/ecad-cadcraft/tests/all/lanes/lane_041.rs`, `ecad-ui/tests/all/lanes/lane_041.rs`, `ecad-trial/tests/all/lanes/lane_041.rs` | Tests; one `mod lane_041;` line in each `lanes/mod.rs` |
| ● `ecad/docs/quick-start/041-ui-wheel-tools.md` | How to try it |

No new command module is expected (actions run existing commands). If one is needed (for example
`duplicate` with the next free tag), it registers in `ecad-cadcraft/src/lanes.rs` with one line
`("041", crate::<module>::specs),`.

Not touched: the canvas look and markers (040), the right panel, Settings and Standards (042),
the banner and History (043), `project.rs`, upstream (no new seam: US1's switches are enough).

## Scope, in order

1. **Action table** (design §7.2) over the existing commands. Tests: every `Action::command` is a
   registered command id; every user-visible ECAD command is in an action or in
   `PROGRAMMATIC_ONLY` with a reason; no two actions share a key in one context; on every wheel,
   Bottom is a delete, Right a go-to, BottomRight "Show its row"; a kind with fewer actions leaves
   slots empty (nothing shifts).
2. **Tool groups** from the table (spec §2): Select (Select, Lasso), Wire (Wire, Branch, Potential
   or bus line, Jump), Place (Symbol, Terminal, Connector, Continuation arrow), Cable (Cable slash,
   Continue cable), Draw (Line, Polyline, Rectangle, Circle, Arc), Box (Location box, Function box,
   Device or PLC outline), Text (Text, Note, Revision cloud). A tool whose command does not exist
   yet is not listed (no dead entries). Last-used per group; the key picks it and steps (owner
   answer 7). Select's flyout loses the edit tools as their wheel slots land.
3. **Wheel.** Right-click on the page when no command runs (U0 Q4) classifies what is under the
   cursor (`interact::kind`: device, wire, wire end, terminal, cable, text, reference, empty page,
   mixed selection; page row from the Pages tab) and opens its wheel centred on the cursor, with a
   chip naming the object. First tables, from commands that exist (empty where none does yet):

   | Kind | Top | Top right | Right (go to) | Bottom right (row) | Bottom (delete) | Left side (copy, turn, change) |
   | --- | --- | --- | --- | --- | --- | --- |
   | Device | Edit tag | Properties | Go to contacts (`contacts`, `refgoto`; selects through `Trial::goto_symbols`) | Show row (Lists ▸ Devices) | Delete device (`devicedelete`) | Duplicate, Rotate, Mirror |
   | Wire | Fix number (`wirenumber`) | Add to cable (`cableadd`) | Show net (`net`) | Show row (Wires, when it exists) | Delete (`ecaddelete`) | Set potential (`potential`), Join (`joinwires`), Wire settings |
   | Wire end | Connect (`connect`) | Disconnect (`disconnect`) | Show net | — | Delete | Junction (`junction`) |
   | Terminal | Edit label (`termlabel`) | Jumper (`jumper`) | Go to strip plan (`stripplan`) | Show row (Lists ▸ Terminals) | Delete | Accessory (`accessory`), Make strip position (`toposition`) |
   | Cable | Cable view (`cableview`) | Continue (`cablecontinue`) | Go to reference (`cablegoto`) | Show row (cable report) | Delete | Join (`cablefan`), Core labels (`cablecore`) |
   | Text | Edit | Show or hide (`showtext`) | — | — | Delete (free text only) | Move text (`movetext`) |
   | Reference | Go to target (`refgoto`) | — | Go to (same) | — | — | — |
   | Empty page | Paste | Page properties (`pageprops`) | — | Show page in Pages | — | New page (`newpage`) |
   | Mixed selection | Move | Copy | — | — | Delete | Rotate, Mirror |

   The designer of the worker's step may refine positions within these rules; the test of step 1
   holds whatever they become. A greyed slot says why and offers the next step (spec §5).
4. **Intent line** (output only): replaces the command line at rest **and** while prompting. It
   shows the running tool's prompt (`prompt_text`), its keywords as clickable chips, what is being
   typed (`CmdLine::buffer`, still typed into CADCraft's prompt as today), and the electrical
   intent where `interact::intent` knows it: Wire ("joins A1 to wire 24V-05"), Place symbol
   ("places -K02; A1 joins 24V-05; A2 stays open"), Cable slash ("adds 3 wires to -W01"); other
   tools show their prompt. Refusals at the cursor and in this line (spec §5: what, why, one or two
   buttons; "Show it" at least). `show_command_line` is true only with the developer switch.
5. **Search** (Ctrl+K): pages, tags, symbols (today's search) and actions by label and `search`
   words; Enter runs an action on the current selection or starts its tool.

## Out of scope

Symbol builder actions (Edit symbol, Make symbol: M1 1.1), part sets (Choose part: M2), Connector
maker, PLC address editor, the Ask tab (owner answer 2: MCP only), wheel customisation (spec: fixed),
the canvas markers (040), the inspector (042).

## Update map

| View | Changed by | Does |
| --- | --- | --- |
| Wheel | Opened on a right-click; closed on any run, Esc or a click outside | Reads the kind and `enabled` once on open; nothing while closed |
| Tool strip | Running command started or ended; last-used change | Re-read the group's label and icon |
| Intent line | Prompt, buffer or cursor change while a command runs | Recompute the sentence for the current hover target only, cached per (tool, target, change count) |
| Search results | Typing in the field | Filter the kept action list; pages and tags from the existing search index |

Every other op: nothing. At rest (no command, wheel closed) none of these reads the project.

## Acceptance criteria

1. **Registry.** The step 1 tests pass; MCP's `list_commands` output is unchanged by this lane
   (test compares before and after).
2. **Tools.** Each group key starts its last-used tool and steps on a second press; a flyout lists
   only tools whose command exists; every old ECAD Tool Sets button's command is reachable from an
   action (test; the `TOOLS` "one flyout" test is replaced by it).
3. **Wheel** (kittest, 1920 × 1080): right-click on `-K01`'s coil opens the device wheel with its
   chip; press-move-release and click-click both run the same action; Bottom deletes as one
   transaction; a greyed slot shows its reason; releasing outside cancels; right-click while Wire is
   running finishes the segment (as Enter) and opens no wheel; no wheel over Lists, Library or
   Publish (the hidden page does not answer: `Shell::over_shown_page`).
4. **Intent line.** With Wire running, the command line is not drawn, the prompt and keyword chips
   show in the intent line, a typed `@10,0` appears there and is applied on Enter; hovering A1 of
   `-K01` shows "joins A1 …"; a refused click shows what, why and a button.
5. **Keys.** Every key in spec §5's table that exists after this lane is pressed in a test (U2's
   Del was missed by not testing it; review of #74).
6. **Ctrl+K** finds a page, a tag, a symbol and an action, and runs the action.
7. **Performance.** Opening the wheel ≤ 1 frame; the intent line adds ≤ 0.5 ms per frame while a
   tool runs and 0 at rest (counter of project reads).

## Tester

None (no format, op or store change; actions run existing commands). The reviewer checks the
registry tests and that no command is defined twice.

## Owner check

Draw a contactor circuit with only the tools, the wheel and the intent line, without the
developer switch.
