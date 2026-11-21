# Sources

No specification for the VCX serial frame was found, in `primarysource` or elsewhere. SAE J2534 is
the pass-through a service tool would use, and it is not imported. This crate does not invent one.

The frame rules were read from `VCX.dll` (VX Manager 1.8.9) and checked on 2026-11-20 against a
dongle on COM4, a WCH CH343 (`USB\VID_1A86&PID_55D3`). The `GetInfo` request is header
`80 00 8C 00` with an empty payload. The dongle answered 72 bytes at 921600 8N1, and the checksum
byte in that answer matches the sum this crate computes. The echo frame in the DLL, header
`80 00 80 00` and payload `TEST`, matches as well.

What is deliberately not in this crate: the license write, the firmware write, and the
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
`VCX_ERR_INVALID_CMD` for a third byte it does not send, including selector
`0x91` itself. The nine selectors it does send copy a fixed length from the
caller buffer and, for a get, copy that many reply bytes back. The reply
must be at least one byte longer than the copy. `config_copy` is those two
lengths: `SetFrame` 2 and 0, `SetCom` 8 and 0, `GetCom` 0 and 8, `SetBt`
`0x28` and 0, `GetBt` 0 and `0x28`, `SetLan` `0x0C` and 0, `GetLan` 0 and
`0x0C`, `SetWlan` `0x2C` and 0, `GetWlan` 0 and `0x2C`. `VCX_DEV_Reset` and `VCX_DEV_ResetEx`
both write `0x8D`. `VCX_DEV_Reset` sends that header with an empty payload.
`VCX_DEV_ResetEx`
sends the same header and one caller byte. `SetPinVol` sends the pin byte
and a big-endian dword. `GetPinVol` (logged as `VCX_DEV_GetDlcVol`) sends
one pin byte. A reply of at least 5 bytes stores payload bytes 1 through 4
as a big-endian dword, and byte 5, when present, selects `PinVolBackups`.
`GetTime` sends nothing and stores the same kind of dword from a reply of
at least 5 bytes. On 2026-11-20 that dword was `0x1FAE5582`, and pin `0x10`
stored `500` at backup index `0x10`. `SetLed` copies 4 caller bytes and `SetBeep` copies 2, in
order. `GetTest` requires `0x107` reply bytes and copies the `0x106` bytes
after the first. `SynExInfo` sends the first `0x80` bytes of a buffer whose
first dword is `0x16161616`, and logs `Set ExInfo`. Any other buffer is
logged `Get ExInfo` and the payload is four zero bytes. A reply applied to
that other buffer fills `0x80` bytes with zero, and when the reply is
exactly `0x81` bytes the bytes after the status replace the fill. On
2026-11-20 the device answered that get with status `0` and `0x80` zero
bytes. A buffer
that already has the mark is not overwritten. `SetExInfo` copies `0x80`
bytes into the frame and then calls `0xB230`, which is virtualized; the
length it returns replaces the `0x80`, so the bytes on the wire were not
recovered. The names and bytes are the `Command` enum.

## Comm profile

`VCX_GetCommIf` at `0x7B2F5250` writes a little-endian type dword and then
`0x114` bytes. A null device returns `0x80` and a null buffer returns
`0x94`. The opener at `0x7B2FA680` saved those `0x114` bytes from its
profile argument. Types 0, 1, and 2 read a dword at `+0`, a dword at `+4`,
and a dword at `+0x10C`. A zero baud becomes `0xE1000` for type 0 and
`0x1C200` for types 1 and 2. A zero delay becomes `0xC8` for types 0 and 1
and `0x5DC` for type 2. A new type 0, 1, or 2 stores byte size 8 and parity
0 in the working line. The type 1 helper names `Serial` and
`SerialSTDiscover`. The type 2 helper names `Bt` and `SerialBTDiscover`.
Types 3 through 8 read a dword at `+0` and `0x40` bytes at `+8`. A requested
type of `0xFF` probes first. `0x7B2F44D0` checks `192.168.8.1`, then
`192.168.171.1`, then `192.168.8.1` again, then `192.168.4.1`. The first
three store mark `0x1AA4` and the fourth stores `0x17`. A positive check
keeps link type 4. `0x7B2F4690` stores `192.168.9.1` and `0x1AA4`, and a
positive check keeps link type 8. The remaining probe is the type 0 opener,
which keeps link type 0.

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

## One exchange

`exchange` packs the caller's header and payload, writes those bytes with overlapped
`WriteFile`, and reads until the first buffer that [`unpack`](../src/frame.rs) accepts.
`ERROR_IO_PENDING` is waited out. The port timeouts are left as the open set them, so an
empty read returns at once and the wait is a 10 ms pause until the caller's deadline.
On 2026-11-20 this wrote the empty `GetInfo` to COM4 and the reply was the same
`VCX-NANO` / `1.9.4.2` block.

## Firmware image header

`VCXHexLoad` logs `Head.Format`, `Head.Name`, `Head.Version`, `Head.Key`, and
`Head.PageNum`. Both shipped files, `vcx.bin` and `vcxa.bin`, start `A5 A5 A5 A5`.
The version dword at offset `0x0C` is `0x01090100`, which is 1.9.1.0. The names are
`VCX-Plus` and `VCX-At40`. `Head.PageNum` is 181 and 186. `file_is_newer` compares that
dword with the running version. 1.9.1.0 is not newer than the dongle's 1.9.4.2, so this
crate does not write an image.

## License info on the device

The on-device buffer is `0xBEC` bytes. The serial is the raw 8 bytes at the start.
`issued` at `+0x14` and `updated` at `+0x18` are little-endian Unix timestamps, shown
as the UTC calendar day. The period dword at `+0x1C` is `0x004F1A00`. From `+0x64`,
each product row is 16 bytes: an 8-byte name, a Unix timestamp, and a zero dword.
The four named rows are `PASSTHRU`, `PDU`, `GMGDS`, and `GMT2WIN`, each dated
2030-12-31. The read frame is header `80 00 83 00` with an empty payload. The
reply body is `0xBFC` ciphertext bytes, and `decrypt_license` applies the
decipher at `0x7B2FB130`. Before that frame, ordinal 21 sends header
`80 00 A0 00` and eight bytes. The reply is status `0x00`, eight bytes, and
the low byte of the sum of the frame. The key's second half is written by the
virtualized handlers at RVA `0x17E885` and `0x211B47` once that reply is on
the VM stack. The clear function at `0x7B2F8680` stores eight bytes at the
same slot and then calls `0x7B2FB460`, which jumps into section `.V0`; ordinal
21 does not take that path. The 16 bytes after the `0xBEC` buffer begin
`67 A5 67 A5`. This crate does not write the buffer back.

Two clear helpers sit just after that decipher. `0x7B2FB340` multiplies two
64-bit values modulo `2^64 - 59`. `0x7B2FB400` raises a 64-bit base to a
64-bit exponent in that same modulus, by squaring on the way down and
multiplying the base back in when the exponent bit is set. Ordinal 21 calls
it twice, with the secret shifted right by one. The first base is 5. The
second base is the eight-byte `0xA0` reply, little-endian. After each result
it squares, and when the secret is odd it multiplies the original base back
in. The nonce is `5^secret` and the eight key bytes are `reply^secret`, both
modulo `2^64 - 59`. `license_nonce` and `license_key_half` are those two
values. Two live calls, one even secret and one odd secret, matched.

## License retrieve

The license service base is `LicenseServerURL`, `https://api.vxdiag.net/v1`. Auth is
`GET {base}/auth` with the query the session builder assembles: the app id piece, the
timestamp piece, the decimal Unix time, and `sign`. The signature is SHA-256 of that
material, written as standard base64, with `+` replaced by `%2B`. The license request
is `GET {base}/devices/{id}/license` with header `Authorization: authcode`. The id is
the logged serial. The body field `result.license` is hex. On 2026-11-20 that hex
decoded to 4076 bytes. The manager passes a `0x1100`-byte buffer to ordinal 22.
The export is virtualized, so the command byte and the chunking were not recovered.
This crate downloads the bytes. It does not send ordinal 20 or ordinal 22, and it
does not send the `0xFF` erase. The manager's order, the 32-byte compare, and the
retry that returns success after three failures are drawn in [protocol.md](protocol.md).

## Firmware catalog

The catalog is `FirmwareURL`, `https://www.vxdiag.net/managecenter/fws.xml`. Each
`ITEM` has `PID`, `VERSION`, `SIZE`, `MD5`, and `URL`. `firmware_for` is the newest
item whose PID list contains the device. A file is an update only when that version
dword is greater than the running version. On 2026-11-20 the row for PIDs 836, 837,
843, and 844 was VCIA 1.9.4.2, the same version this dongle is running. That image
was downloaded anyway: 191770 bytes, header name `VCX-At40`, version 1.9.4.2. The
VCI row at 1.9.6.0 is the other PID set. This crate does not call ordinal `0x27`.

## Page records and the update this crate does not send

`VCXHexLoad` reads `Head.PageNum` packed records after the `0x24`-byte header.
Each record is a little-endian length, a second dword, and that many payload bytes.
`read_pages` returns those records. The VCIA 1.9.4.2 catalog image has 187 records
and the packed records end at the last byte of the file. Ordinal `0x27` is export 39.
It loads the image, enters boot mode `0xA5`, erases, and writes each page. A device
version dword that is already greater is logged and does not stop that sequence.
The frames and the branches are in [protocol.md](protocol.md). `plan_firmware`
names the steps and `hold` refuses a step that would be sent. `rewrite_page` is
`FUN_7b2fab90`, checked against an emulation of that function. This crate does
not send the update.

`VCXPT32.dll` `FUN_10005df0` copies the caller's data to PDU offset 6, and
`FUN_10005a20` chooses the `VCX_PRO_Open` dword. ISO 15765 is `0x8001`.
`FUN_10006070` copies a reply PDU back out: flags, length, then the data, with
the trailing-byte drops in [reads.md](reads.md). `FUN_10005840` builds the
`SET_CONFIG` records, `FUN_10007780` fills an `AddTimer` record, and
`FUN_10005df0` rewrites the bytes at PDU offset 6 and sets PDU bit 0 when it
logs `CAN_MIXED_FORMAT`. `READ_VBATT` replaces a 14000 seed with the first
dword from `VCX_DEV_GetPinVol`. `FIVE_BAUD_INIT` sends one byte with flags 8.
`FAST_INIT` sets flag `0x4` on the UART copy. `VCX_PRO_Recv` jumps into
section `.V0`.
The caller's data bytes are not in that DLL. `VCX_PRO_Recv` is still virtualized.
SAE J2534 is not imported.
