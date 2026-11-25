# Connectors

A connector is one device (`-X10`) with up to two halves that mate: a socket half and a plug half. Pin 3
of the socket half (`-X10.S:3`) and pin 3 of the plug half (`-X10.P:3`) are one net, on any pages. Each
pin is a connection point. A bulkhead is a one-piece connector through a panel wall: one connection point
per pin, reached by wires from both sides, with no halves.

Words used here are in the [glossary](../reference/glossary.md).

## Place a connector

1. In the tool strip, open the **Device** group and choose **Connector** (or type `connector` in the
   command window). The connector maker opens beside the drawing.
2. Answer its questions, or press Enter for the offered answer:
   - **Connector**: Enter for a new one, or type an existing connector's tag to add a half to it.
   - **Halves**: socket only, plug only, or plug and socket.
   - **Type**: Plain, M12, D-sub, RJ45, USB-A or USB-B.
   - **Pins**: how many (1 to 64). **Pin names**: numbers, letters, or your own names (`L, N, PE`).
   - **Draw**: **Row** (all pins in a row, placed with one click) or **Pins** (pin by pin across wires).
3. With **Pins**, draw a line across the wires: the intent line names the wires it cuts before you click.
   One pin pair lands on each wire. Each wire is split, and both parts keep one wire number, since they
   are one net through the mate. Enter ends the tool.

To draw the second half on another page, run **Connector** again there and type the first half's tag at
**Connector**: **Halves** then offers the missing half. A half that already exists is refused:
`-X10 already has a socket half. Place a plug half, or Enter for a new connector.`

Single pins and pin pairs can also be placed from the symbol picker (`socket4`, `plug4`, `pinpair`,
`pins`, `pinp`, and others); give **Device** to join an existing connector.

## Read and change it

- Hovering a pin shows its name with its half: `-X10.S:1`.
- Each half's tag shows where the other half is drawn, as a reference in the project's style (see
  [References](references.md)).
- The inspector shows the pin count and, for each half, its description, maker and part number, and its
  own location: **Location (socket)** and **Location (plug)**. An unset half shows the device's location
  faint.
- From the datasheet, a connector takes **Rated current per contact**, **Rated voltage**, **IP rating
  (mated)** and **Largest cross-section**, one field at a time. They show in the inspector's Device tab
  and in a BOM rating column; a connector draws no rating line.
- **Rename pin** (search with Ctrl+K, or `pinlabel`) renames a pin on both halves, or moves one drawn
  pin to its half's next free pin.
- The BOM has one row per half, each at its own location, with its own description (`D-sub socket,
  9-way`, `D-sub plug, 9-way`).

Settings ▸ Standards ▸ **Connectors** holds the separator between the tag and the half (`.`), the names
of the socket and plug halves (`S`, `P`), the description of a new connector and of each family's
halves, and the answers the maker offers first. Changing a half name changes every half tag, the hover,
the wire report and the BOM at once.

## Checks

| Check | Means |
| --- | --- |
| Connector pin drawn twice on one half | Two symbols draw the same pin of one half |
| Connector halves of two families | The halves are of different types (D-sub and M12) |
| Connector halves of two sizes | The halves have different pin counts |
| Connector pins without a mate | A pin is drawn on one half only |
| Connector pin beyond its pin count | A pin number is past the connector's pin count |

The family and size rows can be ignored per connector (`setprop` key `ignore.connector-family` or
`ignore.connector-size`).

## Command window

```
connector {"halves": "both", "family": "m12", "pins": 4, "names": "numbers", "layout": "pins",
           "line": [[x, y], [x, y]]}
connector {"device": "-X10", "halves": "plug", "layout": "row", "at": [x, y]}
place {"symbol": "pinpair", "device": "<connector id>", "pin": "3", "at": [x, y]}
pinlabel {"device": "<connector id>", "pin": "3", "to": "PE"}
setprop {"component": "<connector id>", "key": "pins", "value": "9"}
setprop {"component": "<connector id>", "key": "half.s.maker", "value": "Example Ltd"}
setprop {"component": "<connector id>", "key": "rating.current", "value": "16 A"}
```

`connector` also takes `rotation`, `swap` (a row's halves swapped), `wire` (the one wire a pin lands on)
and `page`. A half's field is `description`, `maker` or `part_number`; any other word is refused. Each
call is one transaction.

## Limits

- The maker offers more types (M8, heavy-duty, coded M12 variants, USB-C and others), hoods and
  housings, and saving a type; these are greyed: not in the connector library yet.
- Pin descriptions are not set in the maker; names are.
- There is no connector plan report and no cordsets.
