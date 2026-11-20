# vcxtoolkit

The link to a VCX dongle, registered 2026-11-20. Untiered and unsequenced. It sits beside `spskit`,
which downloads SPS modules and does not talk to the device.

`vcxkit` and `vxkit` were the first names and were refused: both are free, and each is one keystroke
from a live package (`voxkit` for `vcxkit`; `gxkit`, `rxkit`, `v2kit`, `vkit`, `vlkit`, `vokit`,
`voxkit` and `wxkit` for `vxkit`). This crate is `vcxtoolkit`.

## What this slice does

- Packs and unpacks one frame, including the checksum and the `0xDD` escape.
- Checks that against the echo frame in the DLL and a `GetInfo` answer captured from a dongle.
- On Windows, opens the USB serial port: overlapped read and write, 8N1, DTR on, and 921600
  when the profile baud is 0.
- Names the command headers the DLL writes, and reads a `GetInfo` reply into the six
  fields that function logs.
- On Windows, writes one packed frame and reads the first reply.
- Reads a firmware image header and says whether that file is newer than the running version.
- Reads the license info already stored on the device.
- Retrieves a license from the license service.
- Reads the firmware catalog and downloads the image it lists for a device. That image
  is an update only when it is newer than the running version.
- Describes the firmware-update steps and builds the DFU payloads whose bytes the DLL
  writes. It does not send them. The pass-through reads are planned in `docs/reads.md`.

## What it does not do

- It does not speak SAE J2534. That document is not in `primarysource`, so this crate does not grow a
  pass-through. `PassThruSupport.04.04`, if a real Techline Connect must see a device, is this crate's
  registry record. `spskit` owns the GM wrapper key `SOFTWARE\GM\J2534Wrapper` and does not open the
  dongle. `j2534bridge.dll` is not assigned to either package. The split is `docs/tool-boundary.md`.
- It does not write a license or a firmware image to the device. A file that is not newer
  is not an update. The on-device license parser does not write the buffer back.
  `docs/protocol.md` draws the manager's update sequence. The crate does not send it.
- It does not read a vehicle module. A brake-module identity read is a later slice, after the port
  and the command set, and only with the car connected.
