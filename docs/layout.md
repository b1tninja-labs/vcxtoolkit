# Crate layout

A single crate: `Cargo.toml`, `src/lib.rs`, `tests/`.

- Library code lives under `src/`; keep `lib.rs` to module declarations and re-exports.
- Integration tests live in `tests/` and use only the public API. Unit tests sit beside the code in
  `#[cfg(test)]` modules.
- Lints are configured in the manifest (`[lints]`), not with `#![...]` attributes scattered in files.
