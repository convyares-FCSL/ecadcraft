# References

How ECAD shows, on one page, where something continues on another, and why it is built on one index. How to
read and follow references is in [../user/references.md](../user/references.md); cable references from the
user's side are in [../user/cables.md](../user/cables.md).

## One word, one mechanism

**References** is the one word for every cross-reference: a coil's contact list and a contact's coil place, a
cable's other end, a continuation arrow's other ends. They are all the same thing: a text on one drawn item
that shows another drawn item's place. So they share one structure, the **reference index**, and only a short
rule table differs between kinds.

Why: before they shared one, each kind had its own derivation, its own update code and its own bugs. One index
means one way to keep texts current, one Go to Reference (`refgoto`, and `cablegoto` for cables) and one read
command (`references`).

## Nothing is stored but the drawing

A reference's text is never stored. What is stored is only the drawing: a cable's reference end (a point,
drawn as a representation of the cable), a device's symbols, a continuation arrow and its name. Everything a
reference **says** (the other end's page and zone, whether it has a partner) is derived when the page is drawn.

Why: references are by identity, never by tag or page name. Renaming, renumbering, reordering or moving pages,
or moving a symbol, changes the texts with no step of its own, and a reference can never be stale or
contradict the drawing. Editing a reference text is refused, with the way out ("move the reference point",
"move or rename the continuation", "move the coil or the contact").

## The index

- A **group** holds the drawn items that may point at each other: one cable's reference ends, one device's
  symbols, or all the continuation arrows of one name.
- A **member** is a drawn item in a group, with a class: a cable reference **end**, a device's **head** (its
  exclusive symbol: coil, MCB, motor…), a **child** (a symbol with contact poles), or other.
- An **edge** says "this source shows that target's place". Edges come from the group's rule, never from the
  file.

| Group | Edges | What a source shows |
| --- | --- | --- |
| a cable | every end to every other end of the cable | the other end's place; `?` when there is none |
| a device | every child to the head; the head to every child | a child: the head's place under it. The head: its contact mirror table |
| a continuation name | every arrow to every other arrow of the name | the other arrows' places |

A device without a head has no references. If a damaged file has two heads, the lower ID is the head.

A **place** is the page's number and the zone of the frame grid the anchor falls in, written in the project's
reference style. Zones come from each end's own page frame, so what a reference says is what that frame shows.

## Device references and the contact mirror table

Under a contact, the text shows the head's place. Under a head, the **contact mirror table** lists one row
per pole: NO or NC, power or auxiliary, its labels and its place. What a part offers comes from its typed
contacts (`3P+1NO+1NC`). Rows the part offers that nobody drew are **spare**; poles drawn beyond the offer are
**over**, and the Checks tab reports them (*More contacts than the part offers*): a warning, never a refusal.
Fewer drawn than offered is normal.

Where the table goes is set on the device (`mirrormove`, or its Properties), else by the project's default,
else automatically below the head. It can be switched off while the texts under contacts stay.

Why derived: the table is the electrician's answer to "where are this relay's contacts used?". It must follow
every move and renumber at once, and it must show spare contacts, which only the part data knows.

## Cable references

A cable that leaves a page and continues on another has a **reference end** on each page. Nothing pairs them
in the file: they are two drawings of one cable, and the index pairs them. An end whose cable has no other
end is an **orphan** and shows `?`; it is the normal state between placing the first reference
(`cablereference`) and continuing the cable on the other page (`cablecontinue`), so the message says so and
the Checks tab lists it until it is resolved. Wire ends at a reference that continue on another page show no
open-end mark: the wire continues and the reference says where.

`cablecontinue`'s preview and its commit use one planning function, so what is previewed is what is
committed.

## Continuation references

A continuation arrow is a drawing aid that joins its net to every arrow with the same name (see
[nets.md](nets.md)). Its reference text lists the other arrows' places in book order, then column, then row, up
to a set number and then `+n`. The text sits past the arrow's tip and reads along the wire, never upside
down, so many arrows side by side do not overlap. An arrow with no partner shows nothing, and the Checks tab
lists it as information: a single named arrow is legitimate.

A **caption** (`contcaption`) is display text on one arrow. On a page, captioned arrows with the same caption
and direction, close together, form a **run**, drawn under one bracket with the caption. A caption joins
nothing and is not compared across pages: each end has its own.

## Why it scales

- The index is built whole on open, reload and branch switch. After that each commit remakes only the groups
  its ops name: at most a few members for a cable, a handful for a device.
- Per commit, the references view finds the pages whose texts, tables or caption runs differ before and after,
  and only those pages are redrawn. A change to the reference style, the sheet, the length unit or the table
  defaults redraws every page that has a reference.
- A group's sorted list of places is worked out once per drawing, so a group of many arrows costs the sort of
  that group, never a pass over all pages per arrow.
- Verify mode checks each reused result against a fresh derivation, and the index against a build.

Background: [archive/designs/](../archive/designs/) holds the original designs for cable references, device
references and continuation references.
