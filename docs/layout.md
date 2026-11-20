# Crate layout

`Cargo.toml`, `src/lib.rs`, `tests/`, and `winport/`.

- Library code lives under `src/`; keep `lib.rs` to module declarations and re-exports.
  `command` names the headers. `frame` packs one frame. `info` reads the logged `GetInfo`
  fields. `exchange` writes one frame and reads the reply. `image` reads a firmware header.
  `license_info` reads the on-device license buffer. `catalog` reads the firmware catalog.
  `remote` retrieves a license and a catalog firmware image. `serial` opens the USB port.
- `winport/` is the package `vcxtoolkit-open`. It is the Win32 boundary: this crate forbids `unsafe`,
  and `CreateFile`, `SetCommState`, `SetCommTimeouts`, `SetCommMask`, `SetupComm`, and `WinHTTP` are FFI.
  It is not a second product and it is not published.
- Integration tests live in `tests/` and use only the public API. Unit tests sit beside the code in
  `#[cfg(test)]` modules.
- Lints are configured in the manifest (`[lints]`), not with `#![...]` attributes scattered in files.
