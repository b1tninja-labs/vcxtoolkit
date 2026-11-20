# vcxcomm

The link to a VCX dongle, registered 2026-11-20. Untiered and unsequenced. It sits beside `spskit`,
which downloads SPS modules and does not talk to the device.

`vcxkit` and `vxkit` were the first names and were refused: both are free, and each is one keystroke
from a live package (`voxkit` for `vcxkit`; `gxkit`, `rxkit`, `v2kit`, `vkit`, `vlkit`, `vokit`,
`voxkit` and `wxkit` for `vxkit`). `vcxcomm` is free on PyPI and crates.io with no live one-keystroke
neighbour (snapshot of 936,266 names, and crates.io, both 2026-11-20). It is the name the DLL uses
for this layer.

## What this slice does

- Packs and unpacks one frame, including the checksum and the `0xDD` escape.
- Checks that against the echo frame in the DLL and a `GetInfo` answer captured from a dongle.

## What it does not do

- It does not open a serial port.
- It does not speak SAE J2534. That document is not in `primarysource`, so `spskit` and `obd-bridge`
  do not grow a pass-through, and neither does this crate.
- It does not call a license server or write firmware.
- It does not read a vehicle module. A brake-module identity read is a later slice, after the port
  and the command set, and only with the car connected.
