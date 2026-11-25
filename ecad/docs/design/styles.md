# Styles: one house style, referred to by ID

A project names one house style, and every styled thing on its pages refers to a named style by ID,
so changing a style changes every drawing that uses it, live. This note says how that is modelled and
why. How to use the Standards screen, text styles and wire types is in
[`../user/styles.md`](../user/styles.md); the defaults registry this builds on is in
[`defaults.md`](defaults.md).

## The words

- A **standard** is a named, shareable house style: values for registry entries at the standard
  level, plus the standard's own style records. A project holds at most one. With none, the project
  uses **IEC (built in)**, the built-in values.
- A **catalogue** is one kind of style: text styles, location box styles and frames (the look's,
  in `ecad-drawing-style`), and wire types, page styles, cable types and rating tables (the rules',
  in `ecad-electrical`). A catalogue declares its fields and ships built-in styles.
- A **style** is one row of a catalogue: an ID, a name and a value per field. It is **built in** (our
  own data) or **own**, added by a standard or a project.
- A **style record** is what a standard or a project stores about a style: its name and the fields
  it sets. For a built-in style the record holds only the fields it overrides.
- A **style choice** is a registry entry whose value is a style ID (`text.<kind>.style`,
  `location.style`, `print.frame.style`, `wire.category`).

## Two questions, answered the same way

Every styled thing asks two questions:

1. **Which style?** It asks a style choice entry, resolved like any registry entry: object → page →
   project → standard → built-in, using only the levels that entry allows.
2. **What does that style say?** It asks a field of that style: the object's own override (only for
   a field that names one), then the project's record, the standard's record, the built-in style,
   and finally the field's own built-in value.

The page level exists only for choices, never for fields: a page picks another style rather than
changing a style for itself. An object overrides a field only where the catalogue names an override
(a wire's own colours and size). This keeps "where does this value come from?" answerable in one
line, which the Standards screen and the inspector show as a source badge.

A stored value that does not parse, or is out of range, counts as absent and falls through to the
next level. Resolving never fails, so a hostile file cannot give a value outside a field's range.

The mechanism (catalogues, fields, resolution) is `ecad-model`'s (`registry::styles`); each crate
declares the catalogues it knows. The look's catalogues belong to `ecad-drawing-style` because they
say how things look; wire types, page styles, cable types and rating tables belong to
`ecad-electrical` because they carry electrical meaning (a wire type's size and rating, a cable
type's cores).

## Style IDs are stable; names are labels

- A built-in style has a word ID, unique in its catalogue (`tag`, `box`, `pe`, `dc24`). A built-in
  word is never renamed or reused.
- An own style has `#` followed by 32 hex digits, allocated like every other ID, and keeps that ID in
  the standard and in every project that copies it.

Renaming changes only the name. Deleting an own style that is in use needs a replacement and moves
every explicit user to it in the same transaction, so an object never points at nothing. A reference
that still points at nothing (a hostile or merged file) falls through to the next level and the
Checks list reports it. A dangling reference is not an invariant breach, so a merge that deletes a
style cannot make a document invalid.

## A project takes its standard by copy

A project holds a copy of its standard, so the project file stays self-contained and opens the same
anywhere. Copying keeps the standard's ID, so a later library can compare a project's copy with the
library's. The standard's header, its registry values and its style records are written with their
own ops (`SetStandard`, `SetStyle`, and `SetProperty` on the standard), and removing the standard
records its values and styles first in the same transaction, so one undo brings it all back.

Style records are typed per catalogue (a wire type's record is a `WireType`), so a misspelt field or
a value of the wrong form fails to read instead of being silently ignored. Bounds on the number of
styles and fields are guards against hostile files, kept as constants beside the code they protect;
records past them are kept, ignored and reported by Checks.

## Who uses a style

Explicit users (objects whose own property names the style) are found through the document's
style-choice index; implicit users (objects that take the page's, project's, standard's or built-in
choice) are worked out from the resolved choice. The count is asked for one style at a time, on
demand, never per frame. A style change redraws the pages of its explicit users and, when the
project's choice resolves to it, every page; nothing rescans the project on an ordinary commit.

## Wire types

A **wire type** is a library entry a wire refers to: the potential it is the default for, its size,
colours and insulation, its BOM part and allowance, its datasheet rating and its drawn line. The
built-in types (power L, L1–L3, neutral, protective earth, 230 V AC L and N, 24 V DC, 0 V DC, signal
and analogue) are our own data in
`ecad-electrical/defaults/wire_types.toml`.

A wire's type is its own `wire_type`, else the type whose potential is its net's potential, else the
registry's default choice, else none. Its colours and size are its own when it has them, else the
type's. A cable core takes no type colour: its core is its colour.

**Only one direction is stored**: a type names the potential it is the default for. The potentials
still drive wire numbering (name, prefix, numbered). Two types naming one potential is refused when
written; in a hostile or merged file the first by ID wins and the Checks list warns. Why one
direction: storing the link on both sides would let them disagree.

Choosing a type for a wire on a net with no potential also gives the net that type's potential, in
the same transaction. It never changes a net that already has another potential: the command says
the net keeps its potential. Changing a net's potential changes the type of every wire on it that has
none of its own, because the default follows the potential.

## Rating tables have no built-in entries

The rating table catalogue is the one catalogue that ships **no built-in styles**: ECAD holds the row
structure, the parser and the lookups, never the values of a standard's tables. A table is entered or
imported by the person (`ratingtable`), from a source they are licensed to use, and may live in the
project or in the standard, so a company's standard can carry one. The project chooses one with
`rating.table` (built-in: none). See [`../user/circuits.md`](../user/circuits.md).

## History and merge

In a merge, a change to a style, the standard or a standard-level value counts as a change to the
project as a whole, handled with the project's own properties and settings (see
[`history.md`](history.md)).

## Commands

`standard` (make, rename, copy from another project, back to IEC), `setstyle`, `deletestyle`,
`promote` (move a project-only style, or the project's own value of a default, into the standard), `setdefault` and `defaults` (registry values
at a level), `styles` (read a catalogue) and `textstyle`. All are generic commands, so the Standards
screen, scripts and agents use the same ones.
