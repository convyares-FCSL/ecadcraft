# Device data (0.2.3 close-out, D4)

## A potential's supply

A row of the potentials table can say what feeds it: the nominal voltage, AC or DC, the phases
(AC) and the frequency (AC, optional). On three phases the nominal is line-to-line (`400 V`), on
one phase line-to-neutral (`230 V`). Nothing is read from the potential's name: a row with no
supply is not known. There is no table editor in the app yet; set it with `wiresettings`:

```
wiresettings {"potentials": [{"name": "L1", "prefix": "L1-", "numbered": true,
  "supply": {"current": "ac", "nominal": 400, "phases": "three", "frequency": 50}}, ...]}
```

`wiresettings {}` shows each row's `supply`. A voltage or frequency that is not above zero is
refused, naming the potential.

## A motor's rated voltage and frequency

A motor's nameplate now also holds its rated voltage and rated frequency: `7.5 kW 400 V 15.2 A
50 Hz` reads as **7.5 kW, 400 V, 15.2 A, 50 Hz**. A DC voltage (`24 VDC`) is refused.

- The Device tab of one motor shows **Rated voltage**. Empty, on a net whose potential has a
  supply, it offers **Use 400 V** (hover: `From L1: 400 V AC 3~ 50 Hz`): a three-phase motor is
  offered the line-to-line voltage, a single-phase motor the line-to-neutral one (`230 V` on a
  400 V system). Nothing is filled in until the link is clicked. `setprop rated_voltage` sets it
  (`400 V` or `400`; empty clears it).
- A rated voltage more than 5 % away from what the motor sees on its supply, or a rated frequency
  other than the supply's (when both are known), is the Checks warning **supply-mismatch**:
  `-M01 is rated 230 V; its supply gives 400 V line-to-line, L1 (400 V AC 3~ 50 Hz)`. When the
  supplied potentials on the motor's nets disagree, nothing is compared.

To try it: give L1 a supply as above, place `motor3`, draw a wire from one of its points and set
its potential to L1, select the motor (the Device tab offers **Use 400 V**), then type `230` as
its Rated voltage (Checks shows **supply-mismatch**). Nobody has tried it by hand yet.

Tests: `devices::supply` in ecad-engine and ecad-electrical, `connectivity::supply` in ecad-model,
`supply_contracts` in ecad-mcp (automated, not tried by hand).
