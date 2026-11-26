# Ratings from parts

Status: Draft, not started; waits for a parts library with ratings (the parts catalogue, see the
[roadmap](../roadmap.md))

## A part sets a device's rating

When a part number's library entry carries a rating, it fills the device's typed rating fields; a value typed
on the device is an override, and a Checks row reports an override that differs from the part. Nothing is
built until a parts library with ratings exists: no stub and no override handling now. Until then the typed
rating fields are stored plainly.

## Catalogue-part ratings

Terminal block types (largest and smallest cross-section, rated current, rated voltage, pitch, connection
screw, spring or push-in) and PLC channel groups (a digital output's output current, an analog channel's
resolution in bits). These are ratings of a catalogue part, not of the device: one set per strip or per
module would store wrong data for a strip or module that mixes them. The other fields of terminal strips'
neighbours, PLC stations and modules (supply, backplane current supplied and drawn, power, power loss) are
built.
