# Checks

ECAD checks the project as you draw: open ends, wires that only look connected, duplicate numbers
and tags, missing parts, rating problems and more. Nothing is refused for these: a check is a note
on the drawing, never a block on your work. Every check is worked out from the project; nothing is
stored.

## Where to see them

- **The bottom bar's checks counter** shows the number of errors (red) and warnings (yellow), or
  `No problems`. Click it to open the Checks tab from anywhere.
- **The Properties panel's Checks tab** (the **Checks** button on the right rail; it carries a dot
  when there are problems). Choose **This page** or **Project**. Each check is a card with what was
  found, where, its fix button when there is one, and an arrow that goes to it. On This page, the
  checks on other pages follow under **Elsewhere in the project**.
- **A device's own checks**: the status line at the top of its Device tab.
- **Show:** at the foot of the Checks tab chooses which kinds are listed (**All kinds** or **Some
  kinds**).

Selecting a check marks it on the page; going to one shows its page with the place ringed. On a
report page of several sheets the right sheet is shown.

The Release dialog shows the same checks grouped for a release; see [releases.md](releases.md).

## Severities

- **Error**: something is wrong and a release stops on it unless you accept it.
- **Warning**: probably wrong; look at it.
- **Note** (info): worth knowing; often a count of things not filled in yet.

## Fix buttons

Many cards carry a fix that does the usual thing in one undoable step, or opens the place to do it:
**Join** (wires that look connected), **Number them** (unnumbered nets), **Finalise** (opens
Finalise numbering), **Use it**, **Set location**, **Set function**, **Show in Settings**, **Go to
Standards**, **Edit block**, **Clear the page's sheet**, **Offer drawn**, **Renumber strip**,
**Set cores…**, **Set pins**, **Place mates**, **Ignore**, and fixes named after their value (such
as **Use A-0110 for all** on an article number conflict).

## Every check

The words are what the Checks tab shows. `name` is the check's name in the `checks` command's
reply. "Count" rows show one line with a number rather than one line per place.

### Wires, nets and numbers

| Shown as | Name | Severity | Means |
| --- | --- | --- | --- |
| Net joins two potentials | `joins-potentials` | Error | One net carries two potential names. |
| Two nets share a number | `shared-number` | Error | Two separate nets have the same wire number. |
| One net, two fixed numbers | `two-fixed` | Error | Two wire numbers were fixed by hand on one net. |
| Open end | `open-end` | Warning | A wire ends on nothing. |
| Loose wire | `loose-wire` | Warning | A wire is connected at neither end. |
| Line not connected at both ends: no wire yet | `no-wire-yet` | Note | A drawn line carries no wire until both ends land on connection points. |
| Looks connected | `looks-connected` | Warning | Two ends meet on the page but are not joined (fix: **Join**). |
| End drawn away from its connection point | `drawn-away` | Warning | A wire's drawn end no longer sits on its connection point. |
| Net with a single connection point | `single-point` | Warning | A net reaches only one connection point. |
| Connection point not connected | `unconnected` | Note | A device's connection point has no wire. |
| Connection point has more wires than it accepts | `over-count` | Error | More wires land than the point allows. |
| Continuation without partner | `lonely-continuation` | Note | A continuation has no other end with its name. |
| Mixed potentials in one net | `mixed-potentials` | Note | |
| Unnumbered nets | `unnumbered` | Note, count | Nets with no wire number (fix: **Number them**). |
| Continuation name not a potential | `unknown-name` | Warning | |
| Wire numbers provisional | `provisional-wires` | Warning, count | Nets without a number while provisional tags are on. |
| Colour code not defined (drawn in ink); define it in Standards | `unknown-colour` | Note | |
| Two-colour wires drawn in one colour (too many dashes on the page) | `dashes-left-out` | Warning | One row per page. |
| Wire category not found; drawn without one | `category-unknown` | Note | A wire's own wire type names none. |
| Two wire categories are the default for one potential | `category-potential-twice` | Warning | The first by ID is used. |
| Wire length not set (set an allowance in Standards) | `wire-allowance` | Note | A wire type with a part number has no allowance, so its BOM row has no length. |

### Devices, tags, locations and functions

| Shown as | Name | Severity | Means |
| --- | --- | --- | --- |
| Two cables share a tag | `duplicate-tag` | Error | Two cables, or a cable and a device, share a tag. |
| Provisional tag | `provisional-tag` | Warning | |
| Location used by none of its symbols | `location-unused` | Warning | |
| Function used by none of its symbols | `function-unused` | Warning | |
| Symbol tied to multiple locations | `symbol-tied-locations` | Error | A device whose location was set by hand has a symbol in a location box of another location. |
| Symbol tied to multiple functions | `symbol-tied-functions` | Error | The same for functions. |
| Devices without a function | `function-none` | Note, count | Shown once some function is set. |
| Function or location no longer in the list | `designation-missing` | Warning | |
| Two entries share a code | `designation-duplicate` | Warning | Only a branch merge makes these. |
| A device with two heads | `two-heads` | Warning | Only a merge makes these. |
| Location box outline crosses itself | `box-crosses` | Warning | |
| Location boxes overlap without nesting | `boxes-overlap` | Note | |
| Devices without a location | `no-location` | Note, count | |
| More contacts than the part offers | `too-many-contacts` | Warning | |
| Data differs from its symbol | `symbol-mismatch` | Warning | For example a fuse bank's ways. |
| Contact table over the title block | `table-over-title` | Warning | A table placed by hand overlaps the title block. |

### Parts and the BOM

| Shown as | Name | Severity | Means |
| --- | --- | --- | --- |
| Devices without a part number | `no-part-number` | Note, count | |
| Items without a part number | `item-no-part-number` | Note, count | |
| Fuses without an insert | `fuse-no-insert` | Note, count | Fuses that no part line serves. |
| One part number, two article numbers | `article-conflict` | Warning | Fix: **Use … for all**. |
| Part line names a fuse or position the device lacks | `part-for-missing` | Warning | |
| Part line or item value ECAD does not read | `bad-part-key` | Note | |

### Ratings

| Shown as | Name | Severity |
| --- | --- | --- |
| Device with no rating | `no-rating` | Warning |
| Rating differs from its supply | `supply-mismatch` | Warning |
| Wire or cable rated below its protective device | `rating-under` | Error |
| Conductor at or above its maximum temperature | `rating-too-hot` | Error |
| Load above the protective device's rating | `rating-load-over` | Warning |
| Cable volt drop over its limit | `voltdrop-over` | Warning |
| Panel wires with no bundling factor | `rating-bundling-unset` | Warning |
| Protected by names no device | `rating-device-missing` | Warning |
| Rated wires and cables with no protective device | `rating-no-device` | Note |
| Fed by two protective devices | `rating-two-devices` | Note |
| Device rating has no readable current | `rating-device-unread` | Note |
| Protected by a device that is not a protective kind | `rating-named-device` | Note |
| Protected wires with no rating data | `rating-no-data` | Note |
| No rating table value for this subject | `rating-no-table` | Note |
| Rating table rows not read | `rating-table-bad-row` | Note |
| Rating table values out of order | `rating-table-odd` | Note |
| Cables with no cable type (values assumed) | `rating-assumed` | Note |
| Cables with no load and no device (no volt drop) | `rating-no-current` | Note |
| Rating value ECAD does not read | `rating-bad-value` | Note |

What the rating checks calculate is in [circuits.md](circuits.md).

### Terminal strips

| Shown as | Name | Severity |
| --- | --- | --- |
| More terminals than the strip offers | `too-many-terminal-blocks` | Warning |
| Terminal drawn twice | `terminal-block-twice` | Error |
| Terminal level drawn twice | `level-twice` | Error |
| Position mixes terminal types | `position-mixed` | Warning |
| Link names a level the position lacks | `level-missing` | Warning |
| Accessory at a position the strip lacks | `accessory-orphan` | Warning |
| Strip accessories without a part number | `no-accessory-part` | Note, count |
| Link crosses a partition | `jumper-bar-across-partition` | Warning |
| Link names positions the strip lacks | `jumper-bar-orphan` | Warning |
| Link on a device that is not a strip | `jumper-bar-on-non-strip` | Warning |
| Fuse terminals that are devices of their own | `fuse-device` | Note |
| Fuse positions without a rating | `no-fuse-rating` | Note |

### Cables

| Shown as | Name | Severity |
| --- | --- | --- |
| More cores used than the cable offers | `too-many-cores` | Warning |
| Core used by two wires | `core-twice` | Warning |
| Cables without a core count | `no-core-count` | Note, count |
| Cable reference with no other end | `orphan-reference` | Warning |
| Cable core open at an end (tick Not used for a spare) | `core-open` | Error |
| Core landed at one end only | `core-one-end` | Warning |
| Core's stored place disagrees with the drawing | `core-place-stale` | Error |
| Cable not in the drawing yet | `cable-not-drawn` | Note (a warning in the release check) |
| Cable end drawn away from its plan | `cable-end-off-plan` | Warning |
| Cable end with no gland size | `no-gland` | Note |
| Cable value ECAD does not read | `cable-bad-value` | Warning |

### PLC

| Shown as | Name | Severity |
| --- | --- | --- |
| Two cards in one slot | `slot-twice` | Error |
| Card names a rack that is not in the project | `rack-missing` | Warning |
| Card in a slot its rack does not have | `slot-range` | Warning |
| PLC cards in no rack | `unracked` | Note, count |
| Channel drawn twice | `channel-twice` | Error |
| Channel beyond the card's count | `channel-over` | Warning |
| Channel drawn as another card type | `channel-type` | Warning |
| Address blocks overlap | `address-overlap` | Warning |
| Address given twice | `address-twice` | Warning |
| Address block past byte 65535 | `address-overflow` | Warning |
| Used channels with no address | `no-address` | Note |
| Wire runs through a PLC card | `wire-through-card` | Warning |
| PLC texts wider than their cells | `plc-text-wide` | Note |
| PLC tag given twice in one PLC | `plc-tag-twice` | Error |
| PLC tag an export leaves out | `plc-tag-chars` | Warning |

### Connectors

| Shown as | Name | Severity |
| --- | --- | --- |
| Connector pin drawn twice on one half | `connector-pin-twice` | Error |
| Connector halves of two families | `connector-family` | Warning |
| Connector halves of two sizes | `connector-size` | Warning |
| Connector pins without a mate | `connector-lone-pin` | Note |
| Connector pin beyond its pin count | `connector-over` | Note |

### Pages, styles and title blocks

| Shown as | Name | Severity |
| --- | --- | --- |
| Continuation sheet has another page's number | `continuation-clash` | Warning |
| Page type not found; drawn as its usual type | `page-type-unknown` | Warning |
| Folder start number not read | `folder-start-unknown` | Note |
| A folder's page numbers run into another folder's | `folder-runs-overlap` | Warning |
| The cover's symbol legend does not show every symbol | `cover-legend-cut` | Note |
| The cover's notes are past 40 notes of 400 characters; the rest are not shown | `cover-notes-cut` | Warning |
| A value is cut in the title block | `title-value-cut` | Note |
| Title block fields this build cannot fill | `title-binding-unknown` | Note |
| Page sheet differs from its frame's; the frame wins | `frame-sheet-differs` | Warning |
| Style not found; drawn without it | `style-unknown` | Note |
| Style records past the limits are ignored | `style-limits` | Note |
| Built-in defaults damaged: safe values used | `defaults-damaged` | Warning |

## Typed

`checks {}` returns the rows, 200 at a time (`"limit"` up to 2000; `"after"` with the `next` a
previous call gave); `checks {"kinds":["open-end","loose-wire"]}` only those kinds. It reads only
and works with an agent's writes off.

## Limits

- The full table of every check is no tab of the Project workspace: use the Checks tab, or
  `checks {}`.
- The Checks tab shows at most 100 cards, and 50 lines under "Elsewhere in the project"; the rest
  are counted.
