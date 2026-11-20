# vcxcomm

The serial frame a VCX dongle speaks: four header bytes, a payload, a checksum, and `0xBB` markers.

`vcxkit` and `vxkit` are free on PyPI and crates.io, and each sits one keystroke from a live package
(`voxkit`, and several others for `vxkit`). This name is the one the DLL uses for the same layer
(`VCXCommSendFrame`, `VCXCommRecvFrame`).

Status: on Windows the crate opens the USB serial port and exchanges one frame. A live
`GetInfo` on COM4 returned `VCX-NANO` and firmware `1.9.4.2`. A firmware image header can be
compared with that version, and the license info already on the device can be read. The crate
does not write firmware and does not call a license server. A vehicle read is not.

## What this is not

It does not speak SAE J2534. It does not call a license server, and it does not write firmware.
`spskit` remains the SPS downloader.

No specification for the frame was found. The rules were read from `VCX.dll` and checked on the wire.
See `docs/sources.md`.

## Use

```rust
let header = vcxcomm::Command::GetInfo.header(0);
let wire = vcxcomm::pack(header, &[]).unwrap();
let frame = vcxcomm::unpack(&wire).unwrap();
assert!(frame.payload.is_empty());
```

`read_header` and `file_is_newer` take a firmware image and the running version.
`parse_license_info` takes the `0xBEC`-byte buffer already on the device. On Windows,
`exchange` writes one packed frame and returns the first reply.

The crate is not published (`publish = false`).

## Develop

    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo test
    cargo doc --no-deps

Minimum supported Rust version: 1.85 (see `docs/msrv.md`).
