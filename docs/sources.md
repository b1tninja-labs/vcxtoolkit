# Sources

No specification for the VCX serial frame was found, in `primarysource` or elsewhere. SAE J2534 is
the pass-through a service tool would use, and it is not imported. This crate does not invent one.

The frame rules were read from `VCX.dll` (VX Manager 1.8.9) and checked on 2026-11-20 against a
dongle on COM4, a WCH CH343 (`USB\VID_1A86&PID_55D3`). The `GetInfo` request is header
`80 00 8C 00` with an empty payload. The dongle answered 72 bytes at 921600 8N1, and the checksum
byte in that answer matches the sum this crate computes. The echo frame in the DLL, header
`80 00 80 00` and payload `TEST`, matches as well.

What is deliberately not in this crate: the license session, the firmware write, and the
pass-through.
