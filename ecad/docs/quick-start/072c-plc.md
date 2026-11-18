# 072 C: PLC stations, modules and channels

Lane C of brief 072 (design 072 §3). A PLC card is now stored as what it is: a **station** (a CPU or
head module card) or a **module** (every other card), each with its own fields. Nothing on screen
moves; the overview's kind words now show each module's signal.

## What changed

- `plccpu` places a station with a **CPU** head and `plcim` one with a **coupler** head (PROFINET
  until you change it). Neither is a module.
- Every other card is a module with a function: digital in/out, analog in/out (RTD is an analog
  input with the RTD signal), communication (an IO-Link master's ports are its channels), or system
  (power supply, power feed). Safety-rated, Ex i and a calibration certificate are yes/no
  attributes, not functions.
- A module's station, slot, channel count, pinned address and each channel's description, typed
  address, PLC tag and safe state are fields of the module. A new card takes the project's channel
  count for its type (`plc.di.channels` …) when you place it; changing that default later leaves
  placed cards as they are.
- The overview's kind words show the module's signal (`16 inputs, 24 V`) and, for a communication
  module, its protocol and ports (`comms, Modbus TCP`).

## Try it

1. New project. Place **PLC CPU** (`plccpu`), then **PLC digital input card** (`plcdi`) and **PLC communication
   module** (`plccm`). Both cards go into the CPU's next free slots.
2. Select the DI card. In Properties, type a channel's description; it shows on the card and in the
   I/O list as before.
3. Type at the command line:
   `setprop {"component": "<the CM card's ID>", "key": "plc.protocol", "value": "modbus_tcp"}`.
   Open the rack overview: the communication module reads `comms, Modbus TCP`.
4. Mark the DI card safety-rated:
   `setprop {"component": "<DI ID>", "key": "plc.safety_rated", "value": "yes"}`.
5. Give the CPU an address: `setprop {"component": "<CPU ID>", "key": "plc.network.address", "value": "192.168.0.10"}`.
   `300.1.1.1` is refused.
6. Open the project in the inspector (`ecad-inspector`): the device rows show `plc_rack` and
   `plc_module` with these fields, and each change is one `ModifyDevice`.

The new `setprop` keys: `plc.signal`, `plc.safety_rated`, `plc.ex_i`, `plc.calibration`,
`plc.protocol`, `plc.network`, `plc.network.address`, `plc.cpu.family`, and on a station
`plc.address.style`. `plc.rack`, `plc.slot`, `plc.channels`, `plc.addr.start`, `plc.addr.size` and
`plc.ch.<i>.text|address|tag|safe` work as before but write the fields.

Channel points (step 3b): place `di` or `do` on a DI or DQ card. The point's key is `.0.in` (or
`.0.out`), as for `chdi`; the label still shows `I0.0`. A point keyed `I1.3` in a hand-made file is
a plain point name, not channel 11.

## Checked

Automated (agent-run): `lane_072c` in ecad-model, ecad-cadcraft, ecad-mcp and ecad-inspector. Not
yet tried by the owner.
