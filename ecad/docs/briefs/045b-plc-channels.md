# Brief 045b: PLC channels and addressing

Status: Draft (designer, 2026-11-13)
Branch: `feature/plc`, after 045a.
Designer: **yes** (`docs/design/045.md`, §4–§7, §9 b-rows, §10, §12) · Size: L
Roadmap: M1 item **1.3(b)**, lane D. Decided: roadmap question 6 (free-text address per channel,
automatic addressing in a chosen style, no vendor built in); design §15 Q6 refines it to live
addresses with pins and overrides.

## Goal

Draw any card's channels on the page where their circuit is, as cells of the card with pin
numbers, the channel number, an address filled in by slot and channel (or typed), a description
written under the cell (rotated by default) and `Spare` on unused channels.

## What exists today

- `di` and `do` (`Kind::Plc { letter }`) join a device with labels that are addresses (`I0.0`).
- 045a's racks, slots and card types; 034's card bodies and anchors; 031's text kinds and show
  rules (`text.tag.show.plc`); 025's head and child index.

## §0 Files touched

| File | Change |
| --- | --- |
| `ecad/crates/ecad-cadcraft/src/plc/channels.rs` (**new**) | Channel keys and pins (design §4.2–§4.3), joining (§4.4), `plcchannels` fence and rest (§4.5), `plcchannel` (§4.6), spare state (§6) |
| `ecad/crates/ecad-cadcraft/src/plc/address.rs` (**new**) | Blocks, formats, pins and overrides (§5); `plcaddress` |
| `ecad/crates/ecad-cadcraft/src/plc/mod.rs` | The b-parts of the `Plc` model and its map; `setprop_rule` for `plc.*` |
| `ecad/crates/ecad-cadcraft/src/defaults/plc.rs` (append) | Design §12's b-entries |
| `ecad/crates/ecad-cadcraft/src/symbols.rs` (lane S; append-only) | Cells `chdi`, `chdq`, `chai`, `chao`, `chrtd`, `chiol` (`pc(12)`–`pc(17)`); `Kind::Plc` → `Kind::Channel(CardType)`; anchors `ChannelNo`, `Address`, `ChannelText` |
| `ecad/crates/ecad-cadcraft/src/devices.rs` | The `Kind::Channel` arm of `join_labels` calls `plc::channels::join` |
| `ecad/crates/ecad-cadcraft/src/device_cmds.rs` | Channel prompt when placing a cell or `di`/`do` |
| `ecad/crates/ecad-cadcraft/src/text_style.rs` | `RunRule::Plc` reads `Kind::Channel` (one line) |
| `ecad/crates/ecad-cadcraft/src/xref.rs` | Cells as children of the card body; the optional channel table (`plc.xref.table`) |
| `ecad/crates/ecad-cadcraft/src/commands.rs` `run_setprop`/`run_setprop_many` | One call to `plc::setprop_rule` beside 034's |
| `ecad/crates/ecad-cadcraft/src/checks.rs` | Append: `channel-twice`, `channel-over`, `channel-type`, `address-overlap`, `address-twice`, `address-overflow`, `no-address` |
| `ecad/crates/ecad-cadcraft/src/interact/forms.rs`, `actions.rs` | Properties ▸ Channel; wheel rows Place channels, Draw remaining channels, Addresses…, Fix addresses |
| `ecad/crates/ecad-cadcraft/tests/all/lanes/lane_045/{channels,address}.rs` | Contract tests |
| `ecad/apps/ecad-mcp/tests/all/lanes/lane_045.rs` (append) | MCP contract tests |
| `ecad/crates/ecad-bench` | `slotmove`, `chfence`, `plcstyle` rows |
| `ecad/docs/quick-start/045-plc.md` | Section 2 |

Not touched: `ecad-model`, `ecad-store`, `ecad-file`, upstream, CI.

## Scope

1. Six channel cells and the compact `di`/`do`, joining a card as its next free channel or a named
   one; refusals with a way out.
2. Channel points: label = pin, key = `.<index>.<role>`; pins from the role or channel layout;
   old `di`/`do` terminals read as channels.
3. Place Channels: a fence across wires, and **Draw remaining channels** for spares.
4. `plcchannel`: description, typed address, pin relabelling.
5. Addresses live by slot and channel in `byte.bit`, `iec`, `slot` or `none`; pinned starts per
   card; `plcaddress` (style, starts, size, fix, clear).
6. Spare and reserved derived; `Spare` printed on drawn spares.
7. Texts from anchors: channel number, address, rotated description; caption; tag once per card.
8. Checks with fixes; inspector; wheel rows; update map; MCP; bench.

## Acceptance criteria

Design §13 rows F–M (b-rows), R (b-commands), S (b-rows), T (b-inputs). In short:

1. A fence across eight wires makes channels 0–7 (pins 1–8) in one transaction; rest draws 8–15 as
   `Spare`.
2. Pins follow `plc.pins` (`role`: AI `+` 1–4, `-` 5–8; `channel`: 1, 2 / 3, 4); a wrong-type,
   full or taken channel is refused with its sentence.
3. Addresses in all three styles match design §13 I; moving a card shifts later cards and the echo
   counts them; after `plcaddress fix` the same move changes nothing.
4. A typed address and description show on the cell and in `iolist`-ready rows; a duplicate gives
   `address-twice` with a working fix.
5. Registry switches (address shown, rotation, caption pattern) change only the cards that inherit
   them (counter).
6. Every b-row of design §9 has a working fix or go-to.
7. MCP contract tests: `plcchannels`, `plcchannel`, `plcaddress`, `place` of a cell with device and
   channel record `Author::Agent(<client>)` and are refused with writes off.
8. Update map and bench rows within target; verify-mode equality.
9. Hostile keys, counts and addresses of design §13 T: refusal or Checks row, never a panic.

## FP items (overseer)

FP2 (channel point keys `.<i>.<role>`, label = pin), FP3 (card properties `plc.channels`,
`plc.addr.start`, `plc.addr.size`, `plc.ch.<i>.text`, `plc.ch.<i>.address`), FP6 (recorded ops),
FP7 (`Kind::Plc` → `Kind::Channel`). No op, no record field, no `ecad-model` change.
**Approved (overseer, 2026-11-13)**; reading old `di`/`do` keys as `a × 8 + b` is not
compatibility work. Decisions in design §14.

## Tester

Yes: a terminal key format (FP2), a change to how `di`/`do` join, fence and relabel transactions.

## Needs owner check

Design §15 questions 3–13, 16. Reference screenshots `045-inputs.png`, `045-analogue.png`,
`045-checks.png`.
