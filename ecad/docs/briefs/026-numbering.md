# Brief 026: Numbering (renumber, fixed, swap, provisional tags)

Status: Approved (owner, 2026-11-11: design §15 Q1–Q13 as recommended; overseer FP1–FP7 as
written, design §17). Designer draft 2026-11-11; owner ideas 2026-11-09 to 2026-11-11.
Branch: `feature/numbering` (worktree `C:\dev\ecad-features\components`, folder name historical),
from `dev` `af2d41eb`.
Designer: **yes** (`docs/design/026.md`) · Size: L
Roadmap: lane C of v0.1 (one real small panel drawn, printed and listed). Runs beside 024
(outputs) and 025 (cross-references). After it: the HTML inspector reading `.ecad` v2, then the
look-and-feel pass.

## Goal

One numbering mechanism for devices, cables and wires. The designer draws first and numbers
later, in drawing order, in one undoable step, without numbering "in the hundreds to leave room".
A clash between two tags always offers a way forward: swap them, make them one, or take the next
free number.

## What exists today

- Device tags (018): component property `tag`, padding `ProjectSettings.tag_padding` (2), prefix
  overrides `tag_prefixes`, next number = the prefix's high-water mark + 1 (`tag::HighWater`,
  never reused). A duplicate is refused with "make it the same device" (`setprop … merge: true`;
  Device Properties, the Devices tab and the Properties panel offer it).
- Wire numbers (022): `wire_number` and `wire_number_fixed` on every conductor of a net; the
  numbering pass; `wirenumber` with `conflict: swap | another | merge`; `renumberwires {scope:
  unnumbered | all}` in book order (page, then top to bottom, then left to right); `?` labels.
- Cables (004, 010): an optional `tag` property, set only when given. No automatic tag, no
  duplicate rule.
- Merge (020b): two devices with one tag give a tag conflict with **Renumber the branch's** /
  **Renumber main's**; "Make them one device" was deferred to here.
- Issue Revision (019 §7.2, 020 §2.5.2): `issuerevision`.
- Project properties (`SetProperty` on the project, 019 FP1) and the Checks tab (022 §8).

## Scope

1. **Renumber.** One dialog and one command for devices, cables and wires.
   - Scope: all, the selection, a prefix (or potential), a folder of pages, or the provisional
     items.
   - Order: drawing order (page in book order, then column, then row of the sheet grid).
   - Numbers: a start number and digits (padding).
   - Preview: old → new for every item before anything is written.
   - Apply: one transaction, one `u`. Tags on every page, references and contact mirrors (025),
     the Devices tab, the Properties panel and wire labels follow through their update maps.
   - 022's Renumber Wires becomes this dialog with Wires chosen; `renumberwires` stays as an
     alias.
2. **Fixed.** A per-device and per-cable flag, like 022's fixed wire number. Renumber and
   Finalise leave fixed items alone and number around them.
3. **Swap.** Giving a device a tag another device holds offers **Swap** first (the two exchange
   tags in one transaction), then **Make it the same device**, then **Use the next free**. The same
   for cables. Wires keep 022's swap / another / merge. Never a dead end.
4. **Provisional tags** (a project setting).
   - New devices get `-K?3`, cables `-W?1`, and new nets `24V-?`, drawn in a provisional colour.
   - **Finalise numbering** numbers the provisional items in drawing order, skipping fixed
     numbers, with preview, in one transaction.
   - Checks: a warning for each provisional tag.
   - Issue Revision lists them, with **Finalise numbering now** or **Issue anyway**.
5. **Make them one device** as a merge-conflict choice (020b): the branch's device joins main's.
   The same for cables.
6. **Cables get automatic tags** (`-W01`) like devices, sharing their high-water mark and
   duplicate rule.
7. **MCP:** every new or changed write goes through its command's JSON form; contract tests.
   **Quick start:** a click-only "Numbering" section.
8. **Labels:** electrical shorthand, small and spaced (design §10).

## Format and ops

New ops and `.ecad` additions only when required; the designer proposes them in design §12 and the
**overseer decides**. The design needs **no new op**. It proposes reserved property keys (fixed
flag, provisional setting, a high-water reset) and one merge choice.

## Acceptance criteria

1. Renumber devices, scope all, start 1, digits 2, on a project tagged out of order gives `-K01`…
   in drawing order per prefix. Fixed devices and out-of-scope tags are skipped and numbered
   around. It is one transaction; `u` restores every tag.
2. Each scope (selection, prefix, folder, provisional) changes only its items. The preview equals
   what Apply writes. Apply on a stale preview is refused and the preview refreshed.
3. Swap: renaming `-K05` to `-K03` (held) with Swap gives both their new tags in one
   transaction. Make it the same device and Use the next free work as before. The same for cables.
4. With provisional tags on, new devices, cables and nets are provisional and drawn so. Checks
   lists each provisional tag. Finalise numbers them in drawing order after the highest number in
   use, skipping fixed ones. Issue Revision offers Finalise numbering now and Issue anyway.
5. A merge with a duplicate tag offers Make them one device, which joins the branch's device into
   main's in the merge transaction, or is refused with the reason while the other choices remain.
6. Save and reopen keep tags, fixed flags and the setting. An old file opens unchanged.
7. Bench, large tier: renumbering all devices, all cables and all wires stays within design §11's
   targets. A swap costs about the same as a tag rename.
8. MCP contract tests for `renumber` (preview and apply), `finalise`, `setprop` tag with
   `conflict`, `tag_fixed`, `numbering`, `issuerevision` with `provisional`, and the merge choice:
   the recorded transaction, `Author::Agent(<client>)`, and refusal with writes off.
9. Reference screenshots, approved by the owner.

## Needs owner check

- Approve the design (`docs/design/026.md`) and answer its questions (§15).
- A click-only trial of the quick-start section: renumber a folder, swap two tags, turn
  provisional tags on, place, finalise, issue.
