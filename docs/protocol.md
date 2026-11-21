# Protocol

What this crate sends, and the update sequence it does not send.
The pass-through reads are planned in [reads.md](reads.md).

The frame and `GetInfo` were checked on COM4 on 2026-11-20. The update sequence was
read from `ManagerCenter.exe` (image base `0x400000`) and from `vcx-reconstructed.dll`
(image base `0x7B2F0000`). No step of that sequence was sent to the dongle, and no
license secret is recorded here. The command bytes inside ordinals 20, 21, and 22
were not recovered. Those exports jump into a virtualized section, and this note
does not invent a frame for them.

## What this crate does

```mermaid
flowchart TD
  open["open USB serial, 8N1, DTR on, 921600"] --> frame["pack one frame and exchange it"]
  frame --> info["GetInfo: name, version, date, logged id"]
  info --> held["read the 0xBEC license info already on the device"]
  info --> fetch["GET the license for that id"]
  info --> catalog["read the firmware catalog"]
  catalog --> file["download the image listed for the PID"]
  file --> newer{"version dword greater than the device?"}
  newer -->|yes| keep["keep the bytes"]
  newer -->|no| same["the file is not an update"]
  keep --> stop["do not write the license or the image"]
  same --> stop
  held --> stop
  fetch --> stop
```

A retrieved license is the server body. It is not the `0xBEC` buffer ordinal 21
returns, and it is not written back. `newer_firmware` is stricter than the manager:
the manager arms an update when the catalog version string differs at all, and the
DLL still enters the boot mode when the device version dword is greater.

## The wire frame

`VCXCommSendFrame` (`0x7B2F9F20`) fills an internal buffer, then escapes it.

| Offset | Contents |
|---|---|
| 0 | `0x80` |
| 1 | `0x00` for the device commands, or a caller byte for `VCX_PRO_Send` |
| 2 | command |
| 3 | `0x00`, or the caller channel |
| 4 | payload length, a little-endian `u32`, refused above `0x1026` |
| 8 | checksum |
| 9 | payload |

The checksum starts at header byte 2. When byte 0 is `0x80` it adds `byte 3 - 0x80 + byte 1`.
Payload bytes are then added, two per step, and a leftover odd byte is added on its own.
For a header `80 00 cmd 00` that is `(cmd - 0x80 + sum of the payload) mod 256`. The empty
`GetInfo` request is `80 00 8C 00` and the checksum byte is `0x0C`.

```mermaid
flowchart LR
  hdr["four header bytes"] --> sum["checksum"]
  pay["payload"] --> sum
  sum --> esc["0xDD then the complement, for BB, DD, and EE"]
  esc --> wire["0xBB, the bytes, 0xBB"]
```

The escape reads the four header bytes, then the payload, then the checksum, and wraps
that stream in `0xBB`. A live `GetInfo` answer on 2026-11-20 matched this sum.

`VCX_SendFrame` (`0x7B2F54C0`) takes one caller buffer. Its length counts the
four header bytes and the payload, and a length above `0x1026` returns `0x94`.
A length of 0 returns `0x94`. A length of 1, 2, or 3 subtracts 4, the inner
check fails, and the export returns `0x82`. `VCX_RecvFrame` (`0x7B2F55E0`)
writes the header and the payload back, and stores that combined length.
`0x7B2F4860` splits a queued body into the header, the payload, and the last
byte. The stored payload length is the body length minus 5. The checksum byte
is kept and is not compared. An empty ring returns 0. `VCX_RecvFrame` tries
the ring at `device+0x24` and then the ring at `device+0x28`. A miss sleeps 1
millisecond. The deadline read after that sleep is virtualized. A missed
deadline returns `0x83`.

The device sender at `0x0801C17E` refuses a payload length of 0 or one past
`0x1026`. The checksum it stores at struct offset `0x102E` is the low 8 bits
of a byte sum. When header byte 0 is `0x80` the sum covers all four header
bytes and the payload, which is the same byte the crate's checksum
already returns. The wire frame is then the `0xBB` frame. When header byte 0
is not `0x80`, only header byte 2 and the payload are summed, and the wire
frame opens with `0xBB`, escapes `0xAA`, `0xBB`, and `0xEE` as `AA 0A`,
`AA 0B`, and `AA 0E`, and closes with `0xEE`.

`0x0801C294` reads one of those frames back. It picks the `0xDD` unstuffer
when a saved byte is `0x80`, and `0x0801BECA` otherwise. That unstuffer
requires at least 2 stuffed bytes. An `0xAA` takes the next byte as
`next | (next << 4)`. A plain result shorter than 2, or longer than `0x1002`,
is refused. Header byte 0 is stored as 0, header byte 2 is the first plain
byte, the last plain byte is the checksum, and the stored length is the plain
length minus 2.

The same wait, at `0x7B2F4C90`, is what the command functions call after a send.
It compares queued header byte 2 with the command byte of the request. A match
returns 1. A queued `0x86` with a payload of at least 5 bytes stores the same
pin backup `GetPinVol` stores, and the wait continues. A `0x86` payload shorter
than 5 bytes stores nothing. Any other command is logged and the wait
continues. The timeout added is the dword at `device+0x20`. A miss returns 0,
and the caller returns `0x83`. `0x7B2F4B60` copies one ring chunk unchanged,
writes `0xBB` before it and after it, and stores the chunk length plus 2.

## License update, as the manager orders it

`DeviceInfoWnd::UpdateDeviceLicense` (`0x0042BA50`) opens with `VCX_Open` when byte
`device+0x1A91` is clear. A return of `0x7F` sets byte `+0x1A92`. `_ReadHWInfo`
(`0x00441F90`) then calls `VCX_DEV_GetInfo` into 64 bytes, `VCX_DEV_SynExInfo` into
128 bytes, `VCX_GetCommIf`, and ordinal 21 into `0xBEC` bytes at `device+0x128`.
A zero return from ordinal 21 sets byte `+0x1A94`. The logged id is taken from the
`GetInfo` serial only after byte 7 matches the xor of the other seven with `0x82`.

```mermaid
flowchart TD
  gate["ordinal 20, twice"] -->|AL is 0| login["out-code 0x1F46"]
  gate -->|AL is 1| auth["GET /auth"]
  auth -->|no authCode| denied["session failure"]
  auth --> active["GET /active"]
  active -->|status 0x323| erase["fill 0x1100 with 0xFF, ordinal 22"]
  active -->|inactive| flag["ordinal 48, then ordinal 47"]
  flag -->|ordinal 48 fails| e8001["out-code 0x1F41"]
  flag -->|ordinal 47 fails| e8002["out-code 0x1F42"]
  active -->|active| download["GET /devices/id/license"]
  download -->|status 0x323| erase
  download -->|hex length is 0| bad["out-code 0xFA4"]
  download --> write["ordinal 22, up to three tries"]
  write --> done["return 1"]
```

Ordinal 20 is `DAT_004B99B4`. `FUN_00442EA0` clears `0x44` bytes at `device+0xD14`
and stores a 16-byte `SYSTEMTIME` at `+0x00` and a 16-byte GUID at `+0x10`.
`FUN_00442FA0` is SHA-256 of those 32 bytes. The first call passes the block with
the dword at `+0x40` set to 0. On a zero return the manager compares eight dwords,
32 bytes at `device+0xD34`, with that digest. The immediate `0x1C` is the counter
the loop starts from. It steps down by 4 and stops once the counter is no longer
greater than 3, so the compare is 32 bytes. A match stores 1 at `+0x40`, writes the
digest back over those 32 bytes, and calls ordinal 20 again. The DLL export does not
show this sequence. Its body is the jump at `0x7B2F6A30`.

Ordinal 22 is `DAT_004B9928`, called with the handle and a `0x1100`-byte buffer.
The write loop in `UpdateLicense` (`0x00442280`) logs `try write license, time: {n}/3`
with the counter at 1, then 2, then 3. A zero return logs `write license okay`.
After the third failure the counter is 4, the branch at `0x00442D8B` joins
`0x00442DED`, the out-code is cleared, and the function returns 1. A failed write
is reported as success. This crate does not copy that loop.

Status `0x323` on the get-state path, the set-state path, or the download path calls
`EraseLicense` (`0x004421C0`). That fills `0x1100` bytes with `0xFF`, runs the same
ordinal-20 gate, and calls ordinal 22. This crate does not send that buffer.

Ordinals 20, 21, and 22 do not call `VCXCommSendFrame` by a direct call. Their
command bytes, chunk sizes, and per-frame layouts were not recovered. The log names
`VCX_DEV_ReqLicense`, `VCX_DEV_GetLicense`, and `VCX_DEV_SetLicense` have no xref.

## Firmware page records

`VCXHexLoad` (`0x7B2FAEC0`) reads `0x24` bytes, then one packed record per
`Head.PageNum`:

| Offset in the record | Contents |
|---|---|
| 0 | payload length, little-endian `u32` |
| 4 | a second dword, stored and not passed to `WrFlash` |
| 8 | `length` payload bytes |

The in-memory copy uses a stride of `0x410`. The allocation is `0x104024`, which is
the header plus 1024 slots. `PageNum` is the count that is actually walked.
`0x400` is only the capacity.

`read_pages` walks the packed records. A length above `0x1026` is refused. The
VCIA 1.9.4.2 catalog image is 191770 bytes, `Head.PageNum` is 187, and the packed
records consume the file exactly. Their lengths run from 8 to 1030. The second
dword is not the index `0..186`.

## Ordinal 0x27

`GetProcAddress` ordinal `0x27` is export 39 at `0x7B2F9C20`. It is not the named
`VCX_DFU_*` exports, and it is not Ghidra's `Ordinal_27` (that one is
`VCX_DEV_SetLed`). The export tail-calls `VCX_DFU_AutoUpdate` (`0x7B2F97B0`).
`UpdateVCIFirmware` calls it at `0x00435FC1` with the handle, the ANSI path of
`\VCX.bin`, 0, and a progress dword. The 0 does not survive: the function overwrites
that argument with the poll count 10. A zero return is the manager's "firmware has
upgrade" branch, which then calls `UpdateLicense`.

The manager decides to call it by a UTF-16 compare of the catalog `VERSION` with
the running version string (`0x00435566`). Any difference sets byte `window+0x504`.
An equal string skips ordinal `0x27`.

```mermaid
flowchart TD
  load["VCXHexLoad"] --> mode["GetMode 0xF1"]
  mode --> boot["SetMode 0xA5, then poll GetMode"]
  boot --> name["DFU GetInfo 0xF2"]
  name -->|names differ and the device name starts with VCX| leave["SetMode 0, return 0xFF"]
  name --> chip["ErChip 0xF3"]
  chip --> flash["each page: rewrite the payload, WrFlash 0xF5"]
  flash --> serial["WrSerial 0xF7, 8 bytes"]
  serial --> done["SetMode 0, return 0"]
```

`GetMode` runs first. When its reply byte is already `0xA5`, the function jumps
to the DFU `GetInfo` call and skips the device `GetInfo`, the version compare,
and `SetMode 0xA5`. Otherwise it calls `VCX_DEV_GetInfo` (`0x7B2F8440`). The
device dword in the compare is at offset `0x34` of the buffer that call fills,
which is `DevInfo.FwVersion`. The file dword is at image+`0x0C` (`0x7B2F9905`).
When the device dword is greater it logs `Device FwVersion is New!` and falls
through. The log is not a refusal. `ErChip` and the page loop still run.
`Head.Key` at image+`0x10` is logged and is not an argument of any DFU command.

Each DFU command uses the internal buffer above, with channel `0x00`.

| Command | Byte | Payload |
|---|---|---|
| `SetMode` | `0xF0` | one byte, `0xA5` to enter and `0` to leave |
| `GetMode` | `0xF1` | empty. The reply length must be at least 2 |
| `GetInfo` | `0xF2` | empty. The reply length must be at least `0x21` |
| `ErChip` | `0xF3` | empty. Called once, before the page loop |
| `WrFlash` | `0xF5` | the buffer ordinal 37 is given. No page index is inserted |
| `WrSerial` | `0xF7` | 8 bytes, or the function returns `0x94` |

There is no sender for `0xF4` or `0xF6`. `VCX_DEV_Reset` is not called from this
function. Shared failure returns are `0x80` for a null device, `0x94` for a null
parameter, `0x82` for a send failure, and `0x83` for a receive failure. Success of
the whole update is `0`, at `0x7B2F9C08`. A refused name or a missed boot mode is
`0xFF`.

The 8-byte name check reads offset `0x20` of the buffer DFU `GetInfo` fills and
compares it with `Head.Name`. When the 8 bytes differ and the device bytes begin
`V`, `C`, `X`, the function calls `SetMode` 0 and returns `0xFF`.

Before each `WrFlash`, `0x7B2FAB90` rewrites the page payload at record+8. The
byte length is the record's first dword. The second dword is not an argument.
The key pointer is the DFU `GetInfo` buffer; the round uses its first 16 bytes,
not `Head.Key`. The instruction is `sub ebx, 0x61C88647`. Modulo `2^32` that
adds `0x9E3779B9`, and the mix is added into each little-endian word.
`rewrite_page` is that transform. It runs only when the caller supplies the key.
This crate does not ask the dongle for the buffer.

The `WrFlash` call that follows pushes the dword at the start of the loaded
image, not the page length. Every image `read_header` accepts begins
`A5 A5 A5 A5`, so that dword is `0xA5A5A5A5`. `VCXCommSendFrame` refuses a
payload above `0x1026`. `write_flash` builds the buffer ordinal 37 copies, with
no page index, and refuses a length above `0x1026`. It does not build the
`0xA5A5A5A5` call. `plan_firmware` records that length and `hold` refuses it.

`WrSerial` is called with length 8. At the resting frame, `[esp+0x20]` is the
third argument. The manager passes 0, so the other pointer is used:
`lea eax, [esp+0x3C]`. `GetInfo` filled the buffer at `[esp+0x28]`, and
`0x3C - 0x28` is `0x14`. The copy skips the reply's first payload byte, so
those 8 bytes are `payload[0x15..0x1D]`. `dfu_serial` reads them. A reply
shorter than `0x21` bytes is the same path as a failed receive: the log is
`VCX_ERR_COMM_RX` and the return is `0x83`, with no copy. `GetInfo` is
reached only after the boot poll has seen `0xA5`. Outside that mode the
device answers `80 00 F2 00` with the one byte `0x91`. A non-zero third
argument is the pointer instead of that buffer. The store of 10 is the
poll counter, at a different slot. After `WrSerial` returns, the zero-argument
path loads 10 into `ESI` and polls `GetMode` until the reply byte is 0. This
crate does not send `SetMode` and does not send `WrSerial`.

The boot-mode poll, after `SetMode 0xA5`, uses the same 10. The store is
`[esp+0x20] = 10` while the three log arguments are still pushed, and
`add esp, 0xC` makes that slot `[esp+0x14]`. Each pass calls `Sleep` and then
`GetMode`. The counter is decremented after the pass, and the pass repeats
while the value before the decrement was not 0, so the body runs 11 times
unless the reply byte is already `0xA5`. `poll_passes` is that count. The
leave poll has the same shape. `Sleep` is `[0x7B2FE0B4]`; its argument is
`0x1F4`. The dumped kernel32 base plus the `Sleep` export offset is
`0x751BD770`, the value in that slot.

`[0x7B2FE13C]` is `MessageBoxA`. The name refusal pushes a null owner, the text
`Device Firmware Wrong!`, the caption `Error`, and `0x1010`, then returns
`0xFF`. This crate does not show that dialog and does not send these commands.

`0x0802DD94` logs the dword already at destination offset `0x10`, then stores
the source dword at offset `0x18` and the source byte at offset `0x0C` as a
halfword at offset 2. It sets the kind byte to 3. The call after those stores
lands inside another routine.

`0x08018A6A` reads the kind byte of a queued record. Kind 2 continues only
when bit 2 of the byte at `r6 + 0x0C` is set. It sets flag `4`, stores a
length of the extra byte plus 4, and writes kind 3. Kinds 1 and 5 store a
length of the extra byte plus the count dword plus 4, and write kind 0.
`0x08018A8E` turns the id dword into the stored id: bits 24..28 become bits
0..4, and the other three bytes swap places. Bit 31 of the original dword
sets flag `0x100`. A non-zero extra byte sets flag `0x400` and the following
byte is kept. The stored length is the low 16 bits.

The DoIP row is a different worker (`0x004363D0`). It sends `\openwrt.bin` through
`ZWSFirmwareUpdate`, not through ordinal `0x27`. A Nano window leaves that byte clear,
so that pass does not transfer.
