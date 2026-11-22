# The saved module read

`spskit-vit1 --read` turns a saved module read into a VIT1 `getProgrammingData`
body. This crate writes that saved read in the format `spskit` already consumes, so
a read collected through the VCX feeds the same offline transform a
`voltkit profile --clone` read does. The format and its parser are in
`spskit/handoff.py` (`parse_clone_profile`); the writer is `src/modread.rs`.

Nothing in `modread` reads a vehicle. It serializes reads that are already in hand.
The collection that would fill them is held and gated: see [collect.md](collect.md).

## The lines spskit reads

`spskit`'s parser (`parse_clone_profile`) is line-oriented and tolerant. It reads:

- `ecu <name> ...` — begins a block. Only the first token after `ecu` matters to
  the parser; the rest of the line is ignored.
- `lid <ident> <hex>` — a service `$1A` read. `<ident>` is one byte; `<hex>` is the
  data the module returned **after** the service byte and the identifier.
- `did <ident> <hex>` — a service `$22` read, the same shape with a two-byte
  identifier.
- Lines starting with `#`, blank lines, and the `s3 <ms>` timing line are skipped.

`spskit` groups each `lid`/`did` line under the `ecu` block above it, uppercases the
identifier, and reads `<hex>` with `bytes.fromhex`. A read with no data is a valid
line with an empty hex field.

## What this crate writes

`clone_profile(&[ModuleRead])` (`src/modread.rs`) writes a documented **superset**
of that format. The superset is on the `ecu` line only, so `spskit` reads it
unchanged:

```
# Captured module bytes. A did or lid line is the data after the service and the identifier.
# Written by vcxtoolkit. This file feeds spskit-vit1 --read; it is not a vehicle write.
s3 5000
ecu BECM req=7E4 resp=7EC functional=no pad=00 latency=0
lid C1 0000002A
lid CC 56494E...
did 4532 0001
```

The `req=`, `resp=`, `functional=`, `pad=`, and `latency=` fields on the `ecu` line
are for the operator; `spskit` keeps only the name (`BECM`). The identity and part
identifiers a given controller answers are its request method — `spskit`'s
saved-read rules (`spskit/vit1.py`, `_RULES`) list them per controller code. The
common `$1A` identity slice is `C1`..`CA` (ten part numbers) plus `CB`/`CC`
(hardware), which is `collect::DEFAULT_IDENTITY_LIDS`.

## The handoff

Once a saved read is written, the offline transform is `spskit`'s, not this crate's.
This crate does **not** reimplement the VIT1 logic:

```
vcxtoolkit collect ...        # writes <saved-read> (currently held; see collect.md)
spskit-vit1 --read <saved-read> --code <K17|K20|...> --vin <VIN> [--out body.json]
```

If the format ever needs a change on the `spskit` side, that is a message to the
`spskit` owner, not a second copy of the transform here. The split is
[tool-boundary.md](tool-boundary.md): this crate owns the dongle, `spskit` owns the
GM wrapper and the VIT1 body.
