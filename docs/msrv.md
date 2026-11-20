# MSRV policy

The minimum supported Rust version is **1.85**, declared as `rust-version` in the manifest and mirrored
in `clippy.toml` (`msrv`). It is the first release that supports edition 2024.

- CI runs the tests on stable and a job on 1.85 so the claim is tested, not asserted.
- Raising the MSRV is a minor-version change at best, announced in the changelog, and never silent.
- `rust-toolchain.toml` pins the channel `stable` for development only; it does not set the MSRV.
