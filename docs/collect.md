# Collecting a module read (operator runbook)

This is the safe preparation for collecting a VIT1 clone from a vehicle through the
VCX-NANO. The pre-flight (`doctor`) is confirmed on hardware. The collection
(`collect`) is **held**: it builds the plan and refuses to transmit, because the
live vehicle read is the single `[open]` row in [../ROADMAP.md](../ROADMAP.md).

> This crate does not program a vehicle, write a license or firmware, or speak
> SAE J2534. See [tool-boundary.md](tool-boundary.md).

## 1. Connect

1. Plug the VCX-NANO into the vehicle's OBD-II port.
2. Plug the VCX-NANO's USB into the laptop. On Windows it enumerates as a CH343
   serial port, typically `COM4`.

## 2. Pre-flight — `doctor` (safe)

```
vcxtoolkit doctor --port COM4
```

`doctor` (alias `info`) opens the USB serial port, sends the empty `GetInfo`, and
prints the device:

```
VCX connected on COM4
  model:    VCX-NANO
  firmware: 1.9.4.2
  fw date:  2023-03-31
  serial:   present (NCAGM26507)
```

`GetInfo` is a dongle command. `doctor` opens **no** vehicle channel and sends
nothing to the car. A "VCX not found" line and a non-zero exit mean the port did not
open or did not answer; check the cable and the COM number. `--port` takes `COM4`,
`com4`, or a bare `4`; the default is `COM4`.

## 3. Collect — `collect` (held)

```
vcxtoolkit collect --ecu BECM --port COM4
```

`collect` builds the ISO 15765 collection plan: the channel open is recovered
(`VCX_PRO_Open` dword `0x8001`), and it lists the `$1A` identity identifiers a VIT1
clone reads (`--lid` to override; default `C1`..`CA`, `CB`, `CC`). It then prints the
saved-read skeleton it would fill and **refuses to transmit**:

```
HELD: vehicle read held: the $1A request content is not recovered
next: the VCX_PRO_Recv reply envelope and the VCX_PRO_Send framing are recovered, but the Send PDU request content ...
then: spskit-vit1 --read <saved-read> --code <K17|...> --vin <VIN>
```

### Why it is held

The refusal is two independent gates (`src/collect.rs`):

1. **Recovered-layout gate** — `READ_LAYOUT_RECOVERED` is `false`. The frame
   envelope is recovered both ways — the `VCX_PRO_Send` framing and the
   `VCX_PRO_Recv` reply read layout (from `VCXPT32!ConvertVCXMsg`) — but the
   vehicle-facing request *content* (service `$1A`, the identifiers, the module CAN
   id) is not in `VCX.dll`, `ManagerCenter.exe`, or `VCXPT32.dll`
   ([reads.md](reads.md), "The open row"). This crate does not invent it.
2. **Owner opt-in gate** — `--owner-unhold`. Even once the layout is recovered, a
   live read of a vehicle is the owner's decision.

Both must hold before a byte reaches the car. `--owner-unhold` alone does not open
the gate while the layout is unrecovered — and it is unrecovered.

## 4. The handoff to spskit

Once a read is collected (after the hold is lifted), it is written in the format
`spskit-vit1` reads ([module-read.md](module-read.md)). The VIT1 transform is
`spskit`'s, offline, and this crate does not duplicate it:

```
spskit-vit1 --read <saved-read> --code <K17|K20|...> --vin <VIN> --out body.json
```

The saved read goes wherever you direct it; `spskit-vit1 --read` consumes that file.

## Next step to enable a real read

1. Recover the Send PDU request *content* for the identity read — service `$1A`,
   the identifiers, and the module CAN id (the `[open]` ROADMAP row). The frame
   envelope is already recovered both ways (`frame_identity_request`,
   `read_identity_reply`); what is missing is the vehicle-facing content, which is
   not in the VXDIAG transport binaries. Do not guess it.
2. Once a real request is in hand, set the recovered-layout flag; the parse is
   already wired behind the gate (`collect::read_identity_reply`).
3. The owner opts in (`--owner-unhold`) for a read of their own vehicle.

Until step 1 is genuinely done, `collect` stays held.
