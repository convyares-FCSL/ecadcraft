# UI spec: owner answers (Carl, 2026-11-13)

Answers to `ui-design-spec.md` §8 and the overseer's plan questions. Decided; do not re-ask.

## Plan
- Adopt the U plan, following spec §6:
  - **U0:** architecture design (docs only).
  - **U1:** tokens, primitives and fonts. This is the foundation lane; it merges into dev first.
  - **U2:** the shell.
  - **U3:** the one table component (Checks, then Devices).
  - **U4a:** canvas.
  - **U4b:** wheel, intent line, seven tool groups and one command registry.
  - **U4c:** inspector with origin badges, plus the Standards screen (roadmap 1.13).
  - **U4d:** agent banner and authors in History (with 035).
- 035 (live MCP bridge) and 036 (multi-level terminals) are built now, in parallel.
  - 035's styled banner lands in the U2 shell slot.
  - 036's terminal-manager screen lands in the U3 table.
- **Sheet font:** switch to **Barlow Semi Condensed** in U4a. The UI font is IBM Plex Sans, per the spec.
- **Text sizes:** 2.5 / 1.6 / 1.5 mm are **cap heights**, not font sizes. 031 built them as font sizes, so U4a converts them.

## Spec §8
1. **Wire and core colours in print:** the user picks **mono or colour** as a print setting in the defaults registry. Mono is the built-in default, with the colour code in the wire label.
2. **Agents:** over MCP only. A chat built into the app comes later, not now (API cost; Carl prefers MCP). Build no slot for it.
3. **Agent writes:** live, as designed. Ask-first is a setting.
4. **Page-level overrides:** only for function, location and sheet, as designed.
5. **Link colour:** connection point and terminal link share one hue, as designed.
6. **Contactor letter:** K, as a project override of IEC's Q, as designed.
7. **Keys:** one key per tool group, stepping through the group: V W P C D B T, as designed.
8. **Undo and redo:** keys only, with History for anything further back, as designed.
9. **Sizes:** cap heights, as above.
10. **Edit symbol:** pre-selects "update all", and switches to "update selected" if any placed copy would clash, as designed.
11. **Connector halves:** `-X10.S` and `-X10.P`, as a Standards setting, as designed.
12. **Plain report:** a one-line header. The article number is a field that stays off until the user adds the column, as designed.
