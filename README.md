# vcxtoolkit

The serial frame a VCX dongle speaks: four header bytes, a payload, a checksum, and `0xBB` markers.

`vcxkit` and `vxkit` are free on PyPI and crates.io, and each sits one keystroke from a live package
(`voxkit`, and several others for `vxkit`). The DLL names this layer `VCXComm` (`VCXCommSendFrame`,
`VCXCommRecvFrame`). The crate was registered as `vcxcomm` and is now `vcxtoolkit`.

Status: on Windows the crate opens the USB serial port and exchanges one frame. A live
`GetInfo` on COM4 returned `VCX-NANO` and firmware `1.9.4.2`. A firmware image header can be
compared with that version, and the license info already on the device can be read. The crate
retrieves a license from the license service, and it downloads the firmware image the
catalog lists for the device. That image is an update only when it is newer than the one
running. It does not write either to the device. A vehicle
read is not.

## What this is not

It does not speak SAE J2534. It retrieves a license and a firmware catalog, and it does not
write either to the device. `spskit` remains the SPS downloader.

No specification for the frame was found. The rules were read from `VCX.dll` and checked on the wire.
See `docs/sources.md`.

## Use

```rust
let header = vcxtoolkit::Command::GetInfo.header(0);
let wire = vcxtoolkit::pack(header, &[]).unwrap();
let frame = vcxtoolkit::unpack(&wire).unwrap();
assert!(frame.payload.is_empty());
```

`read_header` and `file_is_newer` take a firmware image and the running version.
`parse_license_info` takes the `0xBEC`-byte buffer already on the device.
`parse_catalog` reads the catalog. `firmware_for` is the image listed for a device, and
`newer_firmware` is that image only when it is newer.
On Windows, `exchange` writes one packed frame and returns the first reply, `fetch_license`
retrieves the server license, `fetch_firmware` downloads the listed image, and
`fetch_newer_firmware` downloads an image only when the catalog has a newer one.

The crate is not published (`publish = false`).

## Develop

    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo test
    cargo doc --no-deps

Minimum supported Rust version: 1.85 (see `docs/msrv.md`).
