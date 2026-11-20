# vcxcomm

The serial frame a VCX dongle speaks: four header bytes, a payload, a checksum, and `0xBB` markers.

`vcxkit` and `vxkit` are free on PyPI and crates.io, and each sits one keystroke from a live package
(`voxkit`, and several others for `vxkit`). This name is the one the DLL uses for the same layer
(`VCXCommSendFrame`, `VCXCommRecvFrame`).

Status: the frame codec is implemented and checked against a live `GetInfo` answer. The serial port,
the command set, and a vehicle read are not.

## What this is not

It does not open the port. It does not speak SAE J2534. It does not call a license server, and it
does not write firmware. `spskit` remains the SPS downloader.

No specification for the frame was found. The rules were read from `VCX.dll` and checked on the wire.
See `docs/sources.md`.

## Use

```rust
let wire = vcxcomm::pack([0x80, 0x00, 0x8C, 0x00], &[]).unwrap();
let frame = vcxcomm::unpack(&wire).unwrap();
assert!(frame.payload.is_empty());
```

The crate is not published (`publish = false`).

## Develop

    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo test
    cargo doc --no-deps

Minimum supported Rust version: 1.85 (see `docs/msrv.md`).
