# Crate layout

`Cargo.toml`, `src/lib.rs`, `tests/`, and `winport/`.

- Library code lives under `src/`; keep `lib.rs` to module declarations and re-exports.
- `winport/` is the package `vcxcomm-open`. It is the Win32 boundary: this crate forbids `unsafe`,
  and `CreateFile`, `SetCommState`, `SetCommTimeouts`, `SetCommMask`, and `SetupComm` are FFI.
  It is not a second product and it is not published.
- Integration tests live in `tests/` and use only the public API. Unit tests sit beside the code in
  `#[cfg(test)]` modules.
- Lints are configured in the manifest (`[lints]`), not with `#![...]` attributes scattered in files.
