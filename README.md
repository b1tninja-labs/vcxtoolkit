# vcxtoolkit

The serial frame a VCX dongle speaks: four header bytes, a payload, a checksum, and `0xBB` markers.

`vcxkit` and `vxkit` are free on PyPI and crates.io, and each sits one keystroke from a live package
(`voxkit`, and several others for `vxkit`). The DLL names this layer `VCXComm` (`VCXCommSendFrame`,
`VCXCommRecvFrame`). This crate is `vcxtoolkit`.

Status: on Windows the crate opens the USB serial port and exchanges one frame. A live
`GetInfo` on COM4 returned `VCX-NANO` and firmware `1.9.4.2`. A firmware image header can be
compared with that version, and the license info already on the device can be read. The crate
retrieves a license from the license service, and it downloads the firmware image the
catalog lists for the device. That image is an update only when it is newer than the one
running. It does not write either to the device. `plan_firmware` names the
update steps and does not send them. The pass-through copies and the ISO 15765
PCI names are in `docs/reads.md` and are not sent. The open row is a brake-module
identity read. `winhive` and `iso9660` are not this project.

## What this is not

It does not speak SAE J2534. It retrieves a license and a firmware catalog, and it does not
write either to the device. `spskit` remains the SPS downloader and does not open the dongle.
The split is `docs/tool-boundary.md`.

No specification for the frame was found. The rules were read from `VCX.dll` and checked on the wire.
See `docs/sources.md`. The update sequence the manager runs, and which this crate does
not send, is drawn in `docs/protocol.md`.

## Use

```rust
let header = vcxtoolkit::Command::GetInfo.header(0);
let wire = vcxtoolkit::pack(header, &[]).unwrap();
let frame = vcxtoolkit::unpack(&wire).unwrap();
assert!(frame.payload.is_empty());
```

`read_header` and `file_is_newer` take a firmware image and the running version.
`parse_license_info` takes the `0xBEC`-byte buffer already on the device.
`license_nonce` and `license_key_half` are the eight-byte challenge and the key half:
`5^secret` and `reply^secret` modulo `2^64 - 59`.
`parse_catalog` reads the catalog. `firmware_for` is the image listed for a device, and
`newer_firmware` is that image only when it is newer.
On Windows, `exchange` writes one packed frame and returns the first reply, `fetch_license`
retrieves the server license, `fetch_firmware` downloads the listed image, and
`fetch_newer_firmware` downloads an image only when the catalog has a newer one.

The crate is not published (`publish = false`).

## Command line

The `vcxtoolkit` binary has two subcommands:

    vcxtoolkit doctor --port COM4     # open the VCX and read GetInfo (safe pre-flight)
    vcxtoolkit collect --ecu BECM     # build the module-read plan; the vehicle send is HELD

`doctor` (alias `info`) opens the USB serial port, sends the empty `GetInfo`, and
prints the device. It opens no vehicle channel. `collect` builds the ISO 15765
collection plan and the saved-read skeleton `spskit-vit1` consumes, then refuses to
transmit: the live read is the open `ROADMAP.md` row, and its request and
`VCX_PRO_Recv` bytes are not recovered. The operator runbook is
[docs/collect.md](docs/collect.md); the saved-read format is
[docs/module-read.md](docs/module-read.md).

## Develop

    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo test
    cargo doc --no-deps

Minimum supported Rust version: 1.85 (see `docs/msrv.md`).
