# vcxcomm

The frame is the part that has been checked on a real dongle. The rows below it are not started.

| Status | Obligation | Anchor | Test |
|---|---|---|---|
| [ok] | Pack and unpack one frame, with the checksum and the DD escape for BB, DD and EE | No specification was found. Read from VCX.dll and checked against a live GetInfo answer on 2026-11-20. See docs/sources.md. | `src/frame.rs` `echo_frame_round_trips`, `get_info_request_is_the_empty_command`, `live_get_info_reply_checks`, `marker_bytes_in_the_payload_are_stuffed`. Making `checksum` return 0 failed the first three (they pin the wire bytes 0x40, 0x0C and 0x58). The round-trip of a stuffed payload still passed, because pack and unpack share the sum. |
| [open] | Open the serial port the way VCXCommOpen does: overlapped, 8N1, DTR on, 921600 for USB | docs/sources.md | |
| [open] | Name the command headers and parse a GetInfo reply into the fields the DLL logs | docs/sources.md | |
| [open] | Read a brake-module identity from a connected car, and do not send a security seed or a write | none until the car is connected | |
