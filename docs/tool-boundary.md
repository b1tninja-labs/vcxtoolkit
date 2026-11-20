# Who talks to the tool

`vcxtoolkit` and `spskit` (`D:\code\spskit`) split the native stack. This crate does not post an SPS request. The same split is in `spskit`'s `docs/tool-boundary.md`.

## This crate

`vcxtoolkit` owns the dongle: the serial frame, `GetInfo`, the license, and the firmware catalog. A later vehicle read is `docs/reads.md`, the `VCX_PRO_*` commands, not the fifteen `PassThru*` exports. It does not load a GM wrapper, write `HKLM\SOFTWARE\GM\J2534Wrapper`, or build a programming-data body.

If real Techline Connect must see a device, this crate is the one that would write `HKLM\SOFTWARE\PassThruSupport.04.04\<product> - J2534` (and the `Wow6432Node` twin) with `FunctionLibrary` pointing at a pass-through DLL, `APIVersion` `04.04`. The existing PASSTHRU installer already does that, including a second key tail `Bosch - MDI` for the same DLL. This crate does not write that key yet. SAE J2534 is not imported, so the fifteen `PassThru*` exports stay unbuilt.

`VCX_SetEventCallback` is export 5 of `VCX.dll`. It is a device-SDK hook for this side, not a registration call into Techline.

## spskit

`spskit` builds the SPS request from a saved module read. It does not open a COM port. If a native client must look like `spsvcs`, that package is the one that would speak the GM wrapper: the init string `%04.04%J2534,<tool name>`, the key `HKLM\SOFTWARE\GM\J2534Wrapper`, and the exports `GetDefaultDeviceInfo`, `ClearDefaultDeviceInfo`, `GetConnectedDeviceInfo`, and `GetDeviceStatus`. `j2534bridge.dll`, which would turn those exports into `PassThruOpen`, is not assigned to either package.
