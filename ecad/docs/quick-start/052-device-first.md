# 052: the device-first model

Brief 052, design `docs/design/052.md`. This file grows as parts a to d land.

## Part a: symbol types, and no `main` symbol

Every symbol of a device is equal. What a symbol is for its device (head, contact, pole,
terminal, channel, half, pin) comes from its definition, never from the order it was placed in.
A device's **head** is its coil, actuator, breaker or PLC card body, by type.

Nothing new appears on screen. What changes is which symbol a rule follows when it was not placed
first.

### Try it

1. **A coil placed last is the head.** Place an NO contact (Device ▸ Symbol ▸ NO) on page 1.
   Join an NC and another NO to the same device. Add a page and join a coil to that device. Each contact's reference text now points at the coil's page,
   and the contact table is drawn under the coil.
2. **Joining a second coil is refused.** Place another coil on the same device: `-K… already
   has a coil.`
3. **Numbering follows the head.** With provisional tags on (Project ▸ Numbering), place a
   contact of a new device at the left of page 1, a coil of another device to its right, and
   the first device's coil on page 2. Renumber devices: the device whose coil is on page 1
   gets the lower number.
4. **Pin-only connector halves.** Place a socket pin (`pins`) on page 1 and its plug pin
   (`pinp`) on page 2. Each pin shows its mate's place; there is no half-level reference text
   beside the first pin.

### Automated checks

- `cargo test -p ecad-cadcraft --test all lane_052`
- `cargo test -p ecad-model --test all lane_052`
