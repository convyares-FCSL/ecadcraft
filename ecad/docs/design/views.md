# Views: how derived state follows an edit

Much of what ECAD shows is worked out from the document, not stored: the references, the Checks
list, the report models, the connector index, which pages need drawing again, the rows of the
Devices list and the Properties palette. Each of these is a **derived view**. This note says how
every view is kept in line with the document after an edit, and why it is built that way. How to
add a view is in [../developer/architecture.md](../developer/architecture.md).

## The problem it solves

There are three ways a transaction reaches the document: a command (with merge and Restore), a
drawing commit (a move, a grip edit), and undo or redo. If each path called each view's update in
its own list, a new view would have to be added three times, and a path that forgot one would leave
that view stale after some edits but not others. A view that matched on ops directly would also
silently ignore a new op. Both mistakes are invisible until someone sees a wrong list.

## The design

**One classifier.** `Effects::of` (`ecad-model/src/document/effect.rs`) turns a transaction's ops
into `Effect`s: a page, device, wire, route, cable, property, style, aid, device link and so on,
each with what it was before and after. It is computed from the ops alone (every op carries the
whole object or its before and after), so it needs no document and never fails. It is the only
`match` on `Op` that views rely on, with no wildcard arm: a new op does not compile until it is
classified.

**One protocol.** `DerivedView` (`ecad-model/src/document/view.rs`) is what every view
implements:

- `note` is the view's **update map**. It is called once per effect on the document before the
  ops, and once on the document after them, and adds to the view's scope what that effect could
  change. It reads the view's kept state but never changes it.
- `update` brings the kept state in line for that scope only, and reports which pages changed,
  whether the kept frame is stale, whether the page list must be read again, and lines for the
  command line (an orphaned reference, for example).
- `verify` compares the kept state with a fresh build, in verify mode.
- `AREAS` names the property areas a view reads, so a property effect outside them never reaches
  its `note`.

**One pipeline.** `Views` (`ecad-electrical/src/views/`) holds every kept view. Each of the three
commit paths makes the same calls: `begin` on the document before the ops, the path's own commit,
then `finish` on the document after it. A refused commit drops what `begin` noted; no view was
touched, so nothing has to be put back. The engine then applies the `Changes` (the change log and
the redraw) in one place, `Project::apply_changes` in `ecad-engine/src/project/changes.rs`.

Because the After phase reads the committed transaction's ops, anything the transaction gained on
the way (the wire-numbering pass's own ops, an undo's inverse) reaches every view without extra
plumbing.

## Stages

Views run in stage order, so a later stage can read an earlier one's result:

1. the indexes that depend only on the document: references, connectors;
2. the report models;
3. the pages, the kept frame and the generated pages, then the rating results and the Checks list
   (whose rating rows follow the rating results and whose cover rows follow the cover);
4. the feeds for the change log: the page tree, the Devices list and the Properties palette.

The feeds keep nothing themselves; they are views because their scope is exactly what the change
log records.

## Built on first show

The report models, the rating results and the Checks list are built the first time something
shows them, and the pipeline skips them until then at no cost (`Lazy`). Their first build runs off
the UI thread on the background pool (`ecad-engine/src/warm.rs`); a result whose project moved on
while it was built is dropped and started again, and a build that fails leaves the view unbuilt for
the next try.

## One way in

Every derived view is reached through the project's `Views`, built once with the engine's inputs
(the text look and the defaults registry, `EngineInputs`) on open, reload and branch switch, and
after that changed only by the pipeline. Another document state, such as an issued revision for a
release, is read through `Views::detached`, built with the same inputs and thrown away after use.
No caller builds a view from scratch on its own, so an engine-made input reaches every view without
being threaded through callers.

## When a view keeps an update map

A view keeps an incremental update only where a full rebuild after each commit costs more than
about 5 ms on the large bench project. Elsewhere it rebuilds whole after a commit, and no
incremental code is kept for it. There are no update maps for terminal strip pages, PLC,
connectors or the book outline. Where a view keeps one, its `note` says which effects change what
it shows and what it does for each; every other effect does nothing. There are no refresh buttons:
views stay live.

A whole rebuild on an ordinary commit happens only through the scope's own "all" case, for the
effects that really reach everything: for the drawn pages, a page moved to another folder (the book
order), a change to the project's info, revisions or a default, a cable's cores changed. Each
view's module doc lists its own update map.

## Guards that keep it this way

- `#![deny(clippy::wildcard_enum_match_arm)]` on the classifier and on `Views`, and the views
  guard in `cargo xtask layers` (`dev/xtask/src/views_guards.rs`): no match over `Effect` in
  ecad-electrical ends in a wildcard, so a new effect kind needs a decision in every view.
- The same guard refuses a direct full build of a view type outside `Views` and the view's own
  module; the files allowed to build one are listed with their reasons, no counts.
- No view note routes a property key by its prefix: a key's meaning belongs in its definition.
- `Document` is `Sync` (checked at compile time), so notes and updates can run in parallel later
  without a change of design; they run in order today.

## What stays outside

Code that reads ops to make or judge a command rather than to keep derived state (a refusal, a
merge's object list, the debugger's op description) stays a plain match on ops. The tag high-water
mark and the designation uses catch up from the history on any path, so they are not views.
