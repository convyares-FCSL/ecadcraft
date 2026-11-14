# PLC tag table fixtures (brief 056)

Synthetic generic CSV tag tables for fixture P56 (design 056 §13): written for this project, no
data from any PLC tool or customer.

- `P56-tags.csv`: comma-separated, UTF-8, the generic header `name,address,type,comment`.
  `%Q2.2` is `Valve_In` (the project says `Valve_Inlet`), `%Q3.0` is missing, `%Q3.4` is
  `Spare_Out4` (a free channel), `%I8.0` is `Door_Sw` (no card there), and 300 memory tags.
- `P56-tags-b.csv`: the same table as a spreadsheet saves it in some locales: `;`-separated, with a
  UTF-8 BOM and capitalised headers.
