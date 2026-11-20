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

## Command headers

The third header byte is the command. `VCX_PRO_Send` is the exception: its header is
`80`, a caller byte, `00`, and another caller byte. `VCX_PRO_GetComParam` and
`VCX_PRO_Clear` do not write a header. `VCX_DEV_Config` returns `0x91` and logs
`VCX_ERR_INVALID_CMD` for a third byte it does not send. `VCX_DEV_Reset` and
`VCX_DEV_ResetEx` both write `0x8D`. The names and bytes are the `Command` enum.

## GetInfo fields

`VCX_DEV_GetInfo` copies the reply payload into the caller buffer, skipping the first
payload byte. It then logs six fields from that buffer. `DevInfo.Serial` is the `%s`
built from the eight bytes at `+0x10`: byte 7 must equal the xor of the other seven
with `0x82`, and the text is the first five bytes plus `%05d` of the big-endian word
at bytes 5 and 6. A failed check logs an empty string. `DevInfo.HwName` is the C
string at `+0x1C`. `DevInfo.FwVersion` is `%d.%d.%d.%d` of bytes `+0x37`, `+0x36`,
`+0x35`, `+0x34`. `DevInfo.FwDate` is `%04d-%02d-%02d` of (`+0x3B` plus 1970),
`+0x3A`, and `+0x39`. `DevInfo.ActiveFlag` is `+0x3C` and `DevInfo.DoNetFlag` is
`+0x3D`.

The DLL accepts a payload of `0x3D` bytes and then still reads `+0x3C` and `+0x3D`.
Those two offsets are in the copied bytes only when the payload is at least `0x3F`
bytes, which is what this crate requires. The 72-byte COM4 reply of 2026-11-20 is
that long. Its logged hardware name is `VCX-NANO`, the version is `1.9.4.2`, and
the date is `2023-03-31`.
