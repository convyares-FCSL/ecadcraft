# Brief 043: Agent banner and authors in History (U4d)

Status: Approved (Carl, 2026-11-13).
Branch: `feature/ui-agent`, from `origin/dev` (no wait: 035 and U2 have merged).
Designer: done (`docs/design/ui-architecture.md` §11, §12 O2; the look and rules are spec §1
principle 8, §2 "Agent banner", §4 `agent` colour and "Agent's last write", screen f; design 035
§7 for the data contract) · Size: M, in three steps
Roadmap: M1 **1.0** follow-up (the live bridge's banner and authorship, styled) and the History
part of **1.5**. Lane **U4d**; runs beside 040, 041 and 042.
Inputs: spec §2, screen f ("Every change has an author and an undo"); owner answers 2 (MCP only)
and 3 (agents write live; ask-first is a later setting); design 035 §5.2 (author per call), §7
(banner contract), §8 (Undo session).

## Goal

While an agent works, the designer sees who it is, what it last did and can pause it or undo its
whole session in one click; History shows the author of every change and groups an agent's
session; the agent's last write is outlined on the page.

## What exists today (`dev` after #74)

- 035's `AgentBanner` (`agent/banner.rs`): the data contract (agent, state Connected / Writing /
  Held / Paused / ReadOnly / Finished / Error, writes, last change, steps, last refusal, others),
  `BannerAction`, element names `agent.banner`, `.state`, `.last`, `.undo`, `.pause`,
  `.disconnect`, `.dismiss`, and an unstyled `show_line`. U2 draws `show_line` in the shell's
  32 px banner slot (`shell/mod.rs` `shell_ui`), and the top bar's agent pill reads the same state
  (`shell/top_bar.rs` `agent_pill`).
- Undo session (`agent/undo_session.rs`): plans and confirms undoing a live session's owned steps
  (035 §8).
- Every commit records its author (`Author::User` or `Author::Agent(name)`; 035 §5.2). History
  shows it as text (`history::author_text`: "you" or the agent's name) in its rows
  (`history_panel.rs`).
- Agent access (the setting) moves to Settings in 042; this lane does not touch it.

## §0 Files touched

● owned by this lane; ○ shared, with the hunk named.

| File | Change |
| --- | --- |
| ● `ecad/apps/ecad-trial/src/agent/banner.rs` | `show` styled with `ecad-ui` primitives (the contract, `BannerAction` and element names unchanged; `show_line` kept as the plain fallback the tests of 035 read, or replaced with the names kept) |
| ● `ecad/apps/ecad-trial/src/agent/last_write.rs` (new) | The agent's last-write outline and name chip, painted over the shown page |
| ○ `ecad/apps/ecad-trial/src/agent/mod.rs` (one `pub mod last_write;` line; nothing else) | |
| ○ `ecad/apps/ecad-trial/src/shell/mod.rs` (`shell_ui`: the banner block calls `agent::banner::show`; `main_view`: one call to `agent::last_write::paint` after 040's `canvas::paint` line, or after `ui_embedded` if 040 is not in `dev`) | |
| ○ `ecad/apps/ecad-trial/src/shell/top_bar.rs` (`agent_pill` only) | The pill in the `agent` colour with the agent's name; click toggles the banner |
| ● `ecad/apps/ecad-trial/src/history_panel.rs` | Author chips, session groups, Undo session from History |
| ● `ecad/crates/ecad-cadcraft/src/history.rs` (read helpers only: `sessions(log) -> Vec<SessionGroup>`) | Grouping of consecutive transactions by one agent author; no change to the ledger |
| ● `ecad/crates/ecad-cadcraft/tests/all/lanes/lane_043.rs`, `ecad-trial/tests/all/lanes/lane_043.rs` | Tests; one `mod lane_043;` line in each `lanes/mod.rs` |
| ● `ecad/docs/quick-start/043-ui-agent-banner.md` | How to try it |

Not touched: `ecad-mcp` and the bridge's protocol, gate and policy (035), Agent access and
Settings (042), the canvas painter layer (040; this lane paints its one outline itself), tools and
wheel (041), the ledger and the `.ecad` format.

## Scope, in order

1. **Banner** (spec §2: 32 px under the top bar, only while an agent is connected): `agent` colour
   bar and name chip, "connected since", the last write as a go-to (opens History at it, 035's
   `.last`), and Pause writes, Undo session (with its step count), Disconnect as `ecad-ui` buttons;
   Held, Paused, ReadOnly, Finished and Error as 035 §7.2 defines, in the token colours (`agent`,
   `warning`, `text_2`, `error`). The agent pill in the top bar mirrors the state.
2. **History authors** (screen f): every row carries an author chip ("You" for a person, the
   agent's name in the `agent` colour for an agent). Consecutive transactions by one agent form a
   **session group** that collapses to one row ("Claude · 14 changes · 20:31–20:44"). While the
   session is live, the group has Undo session (the same plan and confirmation as the banner's,
   035 §8); once the bridge session has ended, the group's transactions undo one by one as any
   others (035's rule for owned steps). Selecting a change marks what it touched on the page and
   shows its details and the command as sent on the right (the existing details, restyled).
3. **Last write on the page**: the objects of the agent's last accepted transaction on the shown
   page get a 0.6 mm dashed outline in `agent_write` with a name chip (spec §4), for as long as
   the banner shows that write as its last; never printed, never an entity. Hidden when Lists,
   Library or Publish hides the page (`Shell::over_shown_page`).

## Out of scope

Ask-first mode and its proposal review (owner answer 3: later, with list import's proposal state),
a chat or Ask tab (owner answer 2), Agent access settings (042), more than one writing agent
(035), a person's name or initials in History (owner question Q1).

## Update map

| View | Changed by | Does |
| --- | --- | --- |
| Banner and pill | 035 §7.5's events only (hello, accepted or refused call, flash end, hold, pause, disconnect, bye, idle, access off, Undo session; `steps` on undo, redo, branch switch) | Re-read the banner value |
| History rows and session groups | A new head; undo, redo, branch switch (as the History tab's map today) | Append or re-group only the tail that changed |
| Last-write outline | Banner's last change; page switch; `changes_since` containing the shown page | Recompute the outline for the shown page, cached per (transaction, page) |

A person's ordinary edit changes nothing in the banner (035 §7.5). Nothing here reads the project
per frame while those inputs are unchanged.

## Acceptance criteria

1. **Contract kept.** 035's tests pass unchanged; the element names find the styled controls; a
   test drives Connected → Writing → Held → Paused → Finished and checks each state's text and
   controls.
2. **Banner** (kittest with 035's test bridge): appears on `ecad.hello` in the 32 px slot (the main
   view is 32 px shorter), disappears after Dismiss; Pause refuses the next agent write with
   035's message; Undo session undoes exactly the session's steps as one confirmed action.
3. **History**: on a log of 3 person edits, 14 agent transactions and 2 person edits, the agent's
   are one group with its count and time span; expanding shows 14 rows with the agent chip; person
   rows show "You"; Undo session from the group is offered only while the session is live.
4. **Last write**: after an agent places `-K02` on the shown page, `-K02` has the dashed outline and
   chip; after the person's next edit the outline stays until the agent's next write or the banner
   is dismissed; it is absent from the plotted page (export test) and over Lists.
5. **Live, cheap**: 300 frames with an agent connected and idle do no project read from the banner,
   the pill or the outline (counter).
6. **Lessons from U2/U3**: every key and button the brief names is exercised in a test; a go-to
   from the banner's last write keeps its view (the stored page view does not overwrite it; review
   of #74).

## Tester

None (no format, op or store change; the bridge's behaviour is 035's, unchanged).

## Owner questions

1. **Q1. A person's name in History.** The ledger records a person as `Author::User`, with no name,
   so History can show "You" but not the spec's initials ("CV"). Storing a person's name or
   initials per transaction is a format change (a field on the author) and puts a personal detail
   in the file. **Recommend:** show "You" now; decide a named author together with multi-user work
   later, as its own FP item.
   **Answer (Carl, 2026-11-13):** History shows **"You"** for the person, not initials. No name
   is stored: no format change and no personal detail in the file.

## Owner check

Connect Claude over MCP, let it draw a circuit, watch the banner and the outline, then undo the
session from History.
