# Crate layout

`Cargo.toml` (now a `[workspace]` root), `src/lib.rs`, `tests/`, `winport/`, and `j2534/`.

- Library code lives under `src/`; keep `lib.rs` to module declarations and re-exports.
  `command` names the headers, the `VCX_DEV_Config` copy lengths, and the
  device-command payloads, including the `SynExInfo` exchange. `frame` packs one frame and splits the send and receive export buffers. `info` reads the logged `GetInfo`
  fields. `exchange` writes one frame and reads the reply over the `SerialPort`
  seam, so a fake serial drives it in tests; `device` composes that seam into the
  channel-open plus Send/Recv exchange a vehicle read would use, still held behind
  the `collect` gate. `image` reads a firmware header
  and the packed page records.
  `license_info` reads the on-device license buffer and derives the key half. `catalog` reads the firmware catalog.
  `remote` retrieves a license and a catalog firmware image. `serial` opens the USB port.
  `comm` reads the profile `VCX_GetCommIf` copies.
  `dfu` builds the DFU payloads the DLL writes, reads the 8 serial bytes at
  offset `0x14` of a `GetInfo` reply, and rewrites a page in memory.
  `update` describes the firmware-update steps and refuses the ones that would be sent.
  `forward` builds the `VCX_PRO_Open` dword and the `VCX_PRO_Send` payload the pass-through copies.
  `pci` names the ISO 15765 PCI fields and the service id at the start of the application data. The pass-through does not build them.
- `winport/` is the package `vcxtoolkit-open`. It is the Win32 boundary: this crate forbids `unsafe`,
  and `CreateFile`, `SetCommState`, `SetCommTimeouts`, `SetCommMask`, `SetupComm`, and `WinHTTP` are FFI.
  It is not a second product and it is not published. It is a path dependency, not a
  workspace member (`exclude`d), so the gates leave its own formatting untouched.
- `j2534/` is the workspace member `vcxtoolkit-j2534`: our SAE J2534-1 pass-through DLL
  (`crate-type = ["cdylib", "rlib"]`). It forbids `unsafe` everywhere but its `ffi`
  module (the `extern "system" PassThru*` exports), the same `unsafe_code = "deny"` +
  `#[allow]` boundary `winport` uses, and delegates every wire decision to the safe
  `vcxtoolkit` codecs. It writes no registry and transmits no vehicle frame. `docs/j2534.md`
  is the design. The root `default-members` keeps it under the same four gates as the library.
- Integration tests live in `tests/` and use only the public API. Unit tests sit beside the code in
  `#[cfg(test)]` modules.
- Lints are configured in the manifest (`[lints]`), not with `#![...]` attributes scattered in files.
