# Instructions for coding agents

`vcxcomm` is a Rust crate (edition 2024, MSRV 1.85). Read `docs/layout.md` before adding modules.

## Commands

Run these from this directory; all four must pass before a change is done.

    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo test
    cargo doc --no-deps

## Invariants

- `unsafe` is forbidden by the lint table in `Cargo.toml`; do not relax it to make something compile.
- Never run `cargo fmt` or `cargo clippy --fix` outside this directory.
- Do not raise `rust-version` without updating `clippy.toml`, `docs/msrv.md` and CI together.
- Do not add a dependency without saying why in the commit message.
- Write scratch output outside the repository; `git add -A` will commit it.
- A test that cannot fail is not a test: break the code once, watch it fail, revert.
- On Windows this crate opens the USB serial port and retrieves a license and a firmware catalog. It does not speak SAE J2534, and it does not write a license or a firmware image to the device.
- Commit messages carry no AI attribution trailer.
