# How ECAD stores a project

This page explains the ideas behind ECAD's project storage. The exact format is specified in
[EEP-0002](../eep/eep-0002.md); the design and its principles are in the architecture proposal
([`docs/document-architecture-proposal.md`](../../../../docs/document-architecture-proposal.md),
§3–§6 and Part E). This page links to both and repeats neither.

## The file is a log, not a picture

An `.ecad` file does not hold the drawing. It holds the **history of changes** that produced it:
a header, then one record per transaction, in order, each one appended at the end when you save.

A **transaction** is one accepted change: the end of a command, an undo or a redo. It holds a
list of **ops**, the smallest recorded changes (`AddItem`, `AttachEnd`, `SetProperty`…). Each op
carries the values it adds, removes or replaces, so it says exactly what happened without any
need to look elsewhere, and it can be inverted from its own content.

Undo does not delete anything. It appends a new transaction holding the inverse ops. The file only
ever grows, and saving is an append and a flush, so a crash can at worst lose what was not yet
saved, and an interrupted save is recognised and recovered on the next open.

Each record is framed with a checksum, and each transaction carries a hash of the one before it.
Damage to a record, a missing or reordered transaction, or a record from another project is
found when the file is opened. See EEP-0002 §3 and §8.

## Opening means replaying

There is no saved "current state" to load. Opening a file **replays** it: starting from an empty
document, every transaction is applied in order, exactly as it was recorded. Replay never
guesses: it does not snap, search, infer connections or invent IDs. Every ID was chosen when the
edit was made and is in the op.

Replay is also the check. Each op checks that the document holds what the op says it replaces,
and after each transaction the document's invariants (I1–I18: every item's page exists, every
conductor end names a real terminal, and so on) are checked for what the transaction touched.
The first transaction that fails stops the replay: the project opens read-only at the last good
state, and nothing in the file is changed. See EEP-0002 §10–§11.

Because the whole history is replayed, opening takes time in proportion to the number of
transactions. [EEP-0003](../eep/eep-0003.md) is the plan for making it follow the size of the
project instead.

## Stored state and derived state

The document that replay builds holds only two kinds of state (proposal §3.1):

- **Stored intent:** what things *mean*. Components and their terminals, conductors and which
  terminal each end is attached to, cables and their members, properties, embedded symbol
  definitions.
- **Drawing content:** what is *drawn* and where. Pages, layers, items (lines, arcs, text, symbol
  instances), and the representations that link drawing to meaning.

Everything else is **derived** and never stored: nets, cable reference texts, open ends, terminal
positions of placed symbols, the CADCraft drawing shown on screen. Derived results are computed
from the document when needed and recomputed after a change. Each one states what it depends
on, and is updated from the objects a commit touched rather than by rescanning the project.
Temporary UI state (selection, zoom, the active page) is not stored either.

This split is why the file stays small and exact: a derived value can never disagree with the
stored model, because it is not in the file to disagree.

## Pages

A **page** is a sheet of the project: a title and a position in the page order. Pages are not
containers. Every drawing item names its page, but electrical objects do not belong to a page: a
component, a conductor or a cable exists once, in the project, whatever pages show it. A cable can
be drawn on page 3 and continue on page 7 and is still one cable.

When you look at a page, ECAD **projects** it: it builds a CADCraft drawing from the items on that
page. Only the page in front is redrawn after an edit; other open pages are marked stale and
redrawn when brought to the front. The projection is derived state and is never saved.

## Representations

A **representation** links a piece of drawing to the object it depicts. It names its subject (a
component, a conductor or a cable), the page it is on, its items, and its role. There are two
forms (proposal §4):

- **Drawn:** ordinary page items that have been given a meaning, with a connection point for each
  terminal, in page coordinates. This is how existing geometry becomes a component without being
  redrawn.
- **Symbol:** one symbol instance, which places an embedded definition with an origin, a rotation,
  a mirror flag and a scale. Terminal positions are derived from the definition.

An object can have several representations (on several pages, or in several roles) or none. One
with none is valid; it is "not shown", and is reported rather than deleted. An item belongs to at
most one representation.

Symbol definitions are **embedded**: the project holds its own copy of every definition it uses,
recorded in the transaction that first placed it, with a note of the library it came from and a
content hash. A project never needs its library to open. See EEP-0002 §14.

## References

Objects refer to each other **by ID**, never by position, name or geometry. A conductor end is
attached to a terminal because the conductor records that terminal's ID; two lines touching on
the page is not a connection. An object can only be removed when nothing refers to it any more,
and the transaction that removes it also removes those references explicitly.

**Cable references** are how a cable crossing pages is shown: where the drawing of a cable stops
on one page, a reference end (a drawn representation of the cable with the role `reference`)
marks the spot, and its text names the page and position where the cable continues. That text
is derived: renumber or reorder the pages and every reference updates, because what is stored is
only the cable and where its drawings stop, never the text. A reference whose partner is missing
is an orphan and is marked in red.

## Where to go next

- The exact bytes, checks and save rules: [EEP-0002](../eep/eep-0002.md).
- What will change, and why: [EEP-0003](../eep/eep-0003.md).
- Trying it in the app, including the read-only debugger that shows a project's transactions and
  where each sits in the file: [`ecad/docs/quick-start.md`](../../quick-start.md).
