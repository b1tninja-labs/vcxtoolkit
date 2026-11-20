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

## The USB open

`VCXCommOpen` for link type 0 opens `\\.\COM{port}`. The `CreateFile` arguments are
`GENERIC_READ | GENERIC_WRITE`, share mode 0, `OPEN_EXISTING`, and `FILE_FLAG_OVERLAPPED`.
A failure is tried three times, with 200 ms between tries.

The DCB keeps the flag dword `GetCommState` returned. `fOutxDsrFlow` and `fDsrSensitivity`
are cleared, and `fDtrControl` is set to enabled. `fRtsControl` is left as the driver
reported it. Byte size, parity, and stop bits are 8, 0, and 0. A profile baud of 0 is
921600.

`SetCommTimeouts` sets `ReadIntervalTimeout` to `0xFFFFFFFF` and the other four fields to 0,
so a read returns whatever is already buffered, including nothing. `SetCommMask` is `0x85`
(`EV_RXCHAR | EV_TXEMPTY | EV_ERR`). `SetupComm` asks for queues of `0x1000` bytes. This open
does not write a frame, and it does not start the transmit thread.

On 2026-11-20 `open_usb(4, 0)` on COM4 (`USB\VID_1A86&PID_55D3\56C2094099`) returned on the
first try. The driver reported 921600, 8N1, DTR enabled, that timeout pattern, and event mask
`0x85`. The read returned 0 bytes.
