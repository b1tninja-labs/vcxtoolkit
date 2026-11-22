//! The `vcxtoolkit` command line: a safe pre-flight and a held collection.
//!
//! Two subcommands:
//!
//! - `doctor` (alias `info`) opens the VCX over USB, sends the empty `GetInfo`,
//!   and prints the device it found. It opens no vehicle channel and sends nothing
//!   to a car. This is the safe pre-flight; `GetInfo` is confirmed on hardware.
//! - `collect` would open an ISO 15765 channel and read the identity a VIT1 clone
//!   needs, but the vehicle send is held and gated ([`crate::collect`]). It refuses
//!   to transmit and prints the plan and the next step instead.
//!
//! The argument parsing, the device formatting, and the collect gating are pure
//! and unit-tested. Only `doctor`'s live `GetInfo` touches hardware, and only on
//! Windows.

use std::fmt::Write as _;
use std::io::Write;

use crate::collect::{
    CollectPlan, DEFAULT_IDENTITY_LIDS, READ_LAYOUT_RECOVERED, vehicle_read_gate,
};
use crate::modread::clone_profile;
use crate::update::{FLASH_SEQUENCE_RECOVERED, flash_gate, hold, plan_firmware};
use crate::{DeviceInfo, ImageError, LicenseInfo};

/// The default COM port when `--port` is not given. COM4 is the dongle in the
/// `#[ignore]`d hardware tests.
pub const DEFAULT_PORT: u32 = 4;

/// Exit code: the command succeeded.
pub const EXIT_OK: i32 = 0;
/// Exit code: the arguments were not understood.
pub const EXIT_USAGE: i32 = 1;
/// Exit code: the VCX was not found, or this build cannot open it.
pub const EXIT_NO_DEVICE: i32 = 2;
/// Exit code: the vehicle read is held and was refused.
pub const EXIT_HELD: i32 = 3;

/// A parsed command line.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Cli {
    /// Print usage.
    Help,
    /// The safe pre-flight: open the VCX and read `GetInfo`.
    Doctor {
        /// The COM port to open.
        port: u32,
    },
    /// The held collection: build the plan, refuse the vehicle send.
    Collect {
        /// The COM port the read would use.
        port: u32,
        /// The controller the saved read is filed under.
        ecu: String,
        /// The `$1A` identifiers the read would request.
        lids: Vec<u8>,
        /// The owner's opt-in to a live read of their own vehicle.
        owner_opt_in: bool,
    },
    /// Read the on-device license and print its products and dates.
    License {
        /// The COM port to open.
        port: u32,
    },
    /// Check the installed firmware against the catalog (read only).
    Firmware {
        /// The COM port to open.
        port: u32,
        /// The catalog product id to check; without it only the installed
        /// version is shown (the catalog is per-PID).
        pid: Option<u32>,
    },
    /// Build the firmware-update plan for an image and refuse the device flash.
    Flash {
        /// The COM port whose installed version the plan compares against.
        port: u32,
        /// Path to the firmware image file.
        image: String,
        /// The owner's opt-in to a flash of their own dongle.
        owner_opt_in: bool,
    },
}

/// Why the command line was not understood.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CliError {
    /// No subcommand was given.
    NoSubcommand,
    /// The subcommand is not one this tool knows.
    UnknownSubcommand(String),
    /// A flag that needs a value was given none.
    MissingValue(String),
    /// A flag's value was not valid.
    BadValue {
        /// The flag.
        flag: String,
        /// The value that was rejected.
        value: String,
    },
    /// A required flag was not given.
    MissingFlag(String),
    /// A flag is not one this subcommand takes.
    UnknownFlag(String),
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSubcommand => f.write_str("no subcommand; try `vcxtoolkit doctor`"),
            Self::UnknownSubcommand(name) => write!(f, "unknown subcommand `{name}`"),
            Self::MissingValue(flag) => write!(f, "`{flag}` needs a value"),
            Self::BadValue { flag, value } => write!(f, "`{flag}` does not take `{value}`"),
            Self::MissingFlag(flag) => write!(f, "`{flag}` is required"),
            Self::UnknownFlag(flag) => write!(f, "unknown flag `{flag}`"),
        }
    }
}

impl std::error::Error for CliError {}

/// Parse a COM port from `COM4`, `com4`, or `4`.
#[must_use]
pub fn parse_port(value: &str) -> Option<u32> {
    let digits = value
        .strip_prefix("COM")
        .or_else(|| value.strip_prefix("com"))
        .unwrap_or(value);
    digits.parse().ok()
}

/// Parse a `$1A` identifier from one or two hex digits, for example `C1`.
fn parse_lid(value: &str) -> Option<u8> {
    u8::from_str_radix(value.trim_start_matches("0x"), 16).ok()
}

/// Parse the argument list (without the program name).
///
/// # Errors
///
/// Returns a [`CliError`] naming the first problem.
pub fn parse(args: &[String]) -> Result<Cli, CliError> {
    let mut iter = args.iter();
    let Some(subcommand) = iter.next() else {
        return Err(CliError::NoSubcommand);
    };
    match subcommand.as_str() {
        "help" | "--help" | "-h" => Ok(Cli::Help),
        "doctor" | "info" => parse_doctor(iter),
        "collect" => parse_collect(iter),
        "license" => parse_license(iter),
        "firmware" => parse_firmware(iter),
        "flash" => parse_flash(iter),
        other => Err(CliError::UnknownSubcommand(other.to_owned())),
    }
}

fn parse_license<'a>(mut iter: impl Iterator<Item = &'a String>) -> Result<Cli, CliError> {
    let mut port = DEFAULT_PORT;
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--port" => port = take_port(&mut iter)?,
            other => return Err(CliError::UnknownFlag(other.to_owned())),
        }
    }
    Ok(Cli::License { port })
}

fn parse_firmware<'a>(mut iter: impl Iterator<Item = &'a String>) -> Result<Cli, CliError> {
    let mut port = DEFAULT_PORT;
    let mut pid: Option<u32> = None;
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--port" => port = take_port(&mut iter)?,
            "--pid" => {
                let value = take_value(&mut iter, "--pid")?;
                let parsed = value.parse().map_err(|_| CliError::BadValue {
                    flag: "--pid".to_owned(),
                    value: value.clone(),
                })?;
                pid = Some(parsed);
            }
            other => return Err(CliError::UnknownFlag(other.to_owned())),
        }
    }
    Ok(Cli::Firmware { port, pid })
}

fn parse_flash<'a>(mut iter: impl Iterator<Item = &'a String>) -> Result<Cli, CliError> {
    let mut port = DEFAULT_PORT;
    let mut image: Option<String> = None;
    let mut owner_opt_in = false;
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--port" => port = take_port(&mut iter)?,
            "--image" => image = Some(take_value(&mut iter, "--image")?),
            // The owner's opt-in to a flash of their own dongle. It is one of two
            // gates; the recovered-sequence gate still holds the write.
            "--owner-flash" => owner_opt_in = true,
            other => return Err(CliError::UnknownFlag(other.to_owned())),
        }
    }
    let image = image.ok_or_else(|| CliError::MissingFlag("--image".to_owned()))?;
    Ok(Cli::Flash {
        port,
        image,
        owner_opt_in,
    })
}

fn parse_doctor<'a>(mut iter: impl Iterator<Item = &'a String>) -> Result<Cli, CliError> {
    let mut port = DEFAULT_PORT;
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--port" => port = take_port(&mut iter)?,
            other => return Err(CliError::UnknownFlag(other.to_owned())),
        }
    }
    Ok(Cli::Doctor { port })
}

fn parse_collect<'a>(mut iter: impl Iterator<Item = &'a String>) -> Result<Cli, CliError> {
    let mut port = DEFAULT_PORT;
    let mut ecu: Option<String> = None;
    let mut lids: Vec<u8> = Vec::new();
    let mut owner_opt_in = false;
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--port" => port = take_port(&mut iter)?,
            "--ecu" => ecu = Some(take_value(&mut iter, "--ecu")?),
            "--lid" => {
                let value = take_value(&mut iter, "--lid")?;
                let lid = parse_lid(&value).ok_or_else(|| CliError::BadValue {
                    flag: "--lid".to_owned(),
                    value: value.clone(),
                })?;
                lids.push(lid);
            }
            // The owner's opt-in to a live read of their own vehicle. It is one of
            // two gates; the recovered-layout gate still holds the send.
            "--owner-unhold" => owner_opt_in = true,
            other => return Err(CliError::UnknownFlag(other.to_owned())),
        }
    }
    let ecu = ecu.ok_or_else(|| CliError::MissingFlag("--ecu".to_owned()))?;
    if lids.is_empty() {
        lids = DEFAULT_IDENTITY_LIDS.to_vec();
    }
    Ok(Cli::Collect {
        port,
        ecu,
        lids,
        owner_opt_in,
    })
}

fn take_value<'a>(
    iter: &mut impl Iterator<Item = &'a String>,
    flag: &str,
) -> Result<String, CliError> {
    iter.next()
        .cloned()
        .ok_or_else(|| CliError::MissingValue(flag.to_owned()))
}

fn take_port<'a>(iter: &mut impl Iterator<Item = &'a String>) -> Result<u32, CliError> {
    let value = take_value(iter, "--port")?;
    parse_port(&value).ok_or(CliError::BadValue {
        flag: "--port".to_owned(),
        value,
    })
}

/// The firmware version as `1.9.4.2`.
#[must_use]
pub fn fw_version(version: [u8; 4]) -> String {
    let mut out = String::new();
    for (index, byte) in version.iter().enumerate() {
        if index != 0 {
            out.push('.');
        }
        let _ = write!(out, "{byte}");
    }
    out
}

/// The `doctor` verdict for a device found on `port`.
#[must_use]
pub fn describe_device(info: &DeviceInfo, port: u32) -> String {
    let serial = if info.serial.is_empty() {
        "absent (check byte did not match)".to_owned()
    } else {
        format!("present ({})", info.serial)
    };
    let mut out = String::new();
    let _ = writeln!(out, "VCX connected on COM{port}");
    let _ = writeln!(out, "  model:    {}", info.hw_name);
    let _ = writeln!(out, "  firmware: {}", fw_version(info.fw_version));
    let _ = writeln!(out, "  fw date:  {}", info.fw_date);
    let _ = write!(out, "  serial:   {serial}");
    out
}

/// The on-device license, formatted. This is a read; nothing is written back.
#[must_use]
pub fn describe_license(info: &LicenseInfo) -> String {
    let mut serial = String::new();
    for byte in info.serial {
        let _ = write!(serial, "{byte:02X}");
    }
    let mut lines = vec![
        "device license (read, not written):".to_owned(),
        format!("  serial:  {serial}"),
        format!("  issued:  {}", info.issued),
        format!("  updated: {}", info.updated),
    ];
    if info.products.is_empty() {
        lines.push("  licensed products: none".to_owned());
    } else {
        lines.push("  licensed products:".to_owned());
        for product in &info.products {
            lines.push(format!("    {} (valid {})", product.name, product.date));
        }
    }
    lines.join("\n")
}

/// The firmware update-check verdict: installed version, and the newer one if any.
#[must_use]
pub fn describe_update_check(installed: [u8; 4], newer: Option<[u8; 4]>) -> String {
    match newer {
        Some(version) => format!(
            "installed firmware {}; update available: {}",
            fw_version(installed),
            fw_version(version),
        ),
        None => format!("installed firmware {}; up to date", fw_version(installed)),
    }
}

/// Build the firmware-update plan for `image` and refuse the device flash.
///
/// Returns the formatted plan (each step marked local or HELD) and the exit code.
/// This is pure: it plans and holds, and never writes to the device. The flash is
/// held behind [`flash_gate`] — the flash wire sequence is not recovered, so even
/// an owner opt-in does not open it.
///
/// # Errors
///
/// Returns the [`ImageError`] from [`plan_firmware`] when `image` is malformed.
pub fn describe_flash_plan(
    image: &[u8],
    device_version: [u8; 4],
    owner_opt_in: bool,
) -> Result<(String, i32), ImageError> {
    let plan = plan_firmware(image, device_version, None)?;
    let mut lines = vec!["firmware update plan (nothing written to the device):".to_owned()];
    for step in &plan.steps {
        match hold(step) {
            Ok(kept) => lines.push(format!("  local: {kept:?}")),
            Err(refused) => lines.push(format!("  HELD : {refused}")),
        }
    }
    match flash_gate(FLASH_SEQUENCE_RECOVERED, owner_opt_in) {
        Ok(()) => {
            // Unreachable while FLASH_SEQUENCE_RECOVERED is false.
            lines.push("gate open: a device flash is permitted. Not implemented here.".to_owned());
        }
        Err(refusal) => {
            lines.push(format!("HELD: {refusal}"));
            lines.push(format!("next: {}", refusal.next_step()));
        }
    }
    Ok((lines.join("\n"), EXIT_HELD))
}

/// The usage text.
#[must_use]
pub fn usage() -> String {
    "vcxtoolkit — VCX-NANO dongle management and (held) module-read collection\n\
     \n\
     USAGE:\n\
     \u{20}   vcxtoolkit doctor [--port COM4]        open the VCX and read GetInfo (safe)\n\
     \u{20}   vcxtoolkit info   [--port COM4]        alias of doctor\n\
     \u{20}   vcxtoolkit license [--port COM4]       read the on-device license (read only)\n\
     \u{20}   vcxtoolkit firmware [--port COM4] [--pid N]\n\
     \u{20}                                          check installed firmware vs the catalog (read)\n\
     \u{20}   vcxtoolkit flash --image FILE [--port COM4] [--owner-flash]\n\
     \u{20}                                          build the update plan; the device flash is\n\
     \u{20}                                          HELD (the flash sequence is not recovered)\n\
     \u{20}   vcxtoolkit collect --ecu NAME [--port COM4] [--lid C1 ...] [--owner-unhold]\n\
     \u{20}                                          build the collection plan; the vehicle\n\
     \u{20}                                          send is HELD and refuses to transmit\n\
     \n\
     doctor/license/firmware are device reads; none writes the dongle or touches a\n\
     vehicle. flash is a device write that stays HELD: the firmware-flash wire\n\
     sequence is not recovered, so it refuses to flash rather than risk bricking the\n\
     dongle. collect never transmits: the live vehicle read is the open ROADMAP row.\n\
     See docs/collect.md.\n"
        .to_owned()
}

/// Run the command line, writing to `out`, and return the process exit code.
pub fn run<W: Write>(args: &[String], out: &mut W) -> i32 {
    let cli = match parse(args) {
        Ok(cli) => cli,
        Err(error) => {
            let _ = writeln!(out, "error: {error}");
            let _ = write!(out, "{}", usage());
            return EXIT_USAGE;
        }
    };
    match cli {
        Cli::Help => {
            let _ = write!(out, "{}", usage());
            EXIT_OK
        }
        Cli::Doctor { port } => run_doctor(port, out),
        Cli::Collect {
            port,
            ecu,
            lids,
            owner_opt_in,
        } => run_collect(port, &ecu, &lids, owner_opt_in, out),
        Cli::License { port } => run_license(port, out),
        Cli::Firmware { port, pid } => run_firmware(port, pid, out),
        Cli::Flash {
            port,
            image,
            owner_opt_in,
        } => run_flash(port, &image, owner_opt_in, out),
    }
}

fn run_license<W: Write>(port: u32, out: &mut W) -> i32 {
    match probe_license(port) {
        Ok(info) => {
            let _ = writeln!(out, "{}", describe_license(&info));
            EXIT_OK
        }
        Err(reason) => {
            let _ = writeln!(out, "license read failed on COM{port}: {reason}");
            EXIT_NO_DEVICE
        }
    }
}

fn run_firmware<W: Write>(port: u32, pid: Option<u32>, out: &mut W) -> i32 {
    let info = match probe(port) {
        Ok(info) => info,
        Err(reason) => {
            let _ = writeln!(out, "VCX not found on COM{port}: {reason}");
            return EXIT_NO_DEVICE;
        }
    };
    let Some(pid) = pid else {
        let _ = writeln!(
            out,
            "installed firmware {}; pass --pid <n> to check the catalog",
            fw_version(info.fw_version),
        );
        return EXIT_OK;
    };
    match newer_version(pid, info.fw_version) {
        Ok(newer) => {
            let _ = writeln!(out, "{}", describe_update_check(info.fw_version, newer));
            EXIT_OK
        }
        Err(reason) => {
            let _ = writeln!(out, "catalog check failed: {reason}");
            EXIT_NO_DEVICE
        }
    }
}

fn run_flash<W: Write>(port: u32, image_path: &str, owner_opt_in: bool, out: &mut W) -> i32 {
    let image = match std::fs::read(image_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            let _ = writeln!(out, "cannot read image `{image_path}`: {error}");
            return EXIT_USAGE;
        }
    };
    // The device version is only the plan's Version step; a plan displays without
    // a device (zeros) so the flash CLI works offline. Nothing is written either way.
    let device_version = probe(port).map_or([0, 0, 0, 0], |info| info.fw_version);
    match describe_flash_plan(&image, device_version, owner_opt_in) {
        Ok((text, code)) => {
            let _ = writeln!(out, "{text}");
            code
        }
        Err(error) => {
            let _ = writeln!(out, "not a firmware image: {error}");
            EXIT_USAGE
        }
    }
}

fn run_doctor<W: Write>(port: u32, out: &mut W) -> i32 {
    match probe(port) {
        Ok(info) => {
            let _ = writeln!(out, "{}", describe_device(&info, port));
            EXIT_OK
        }
        Err(reason) => {
            let _ = writeln!(out, "VCX not found on COM{port}: {reason}");
            EXIT_NO_DEVICE
        }
    }
}

fn run_collect<W: Write>(
    port: u32,
    ecu: &str,
    lids: &[u8],
    owner_opt_in: bool,
    out: &mut W,
) -> i32 {
    let plan = match CollectPlan::iso15765(ecu, lids) {
        Ok(plan) => plan,
        Err(error) => {
            let _ = writeln!(out, "error: {error}");
            return EXIT_USAGE;
        }
    };
    let _ = writeln!(
        out,
        "collection plan for {ecu} on COM{port} (nothing sent):"
    );
    let _ = writeln!(
        out,
        "  channel:  ISO 15765, VCX_PRO_Open dword {:#06x} (recovered)",
        plan.open_code,
    );
    let idents: Vec<String> = plan.lids.iter().map(|lid| format!("{lid:02X}")).collect();
    let _ = writeln!(out, "  would read $1A ids: {}", idents.join(" "));
    let _ = writeln!(
        out,
        "  request + VCX_PRO_Recv bytes: NOT recovered (the [open] ROADMAP row)",
    );
    if let Ok(skeleton) = plan.skeleton() {
        if let Ok(profile) = clone_profile(std::slice::from_ref(&skeleton)) {
            let _ = writeln!(out, "  saved-read it would fill (spskit-vit1 --read):");
            for line in profile.lines() {
                let _ = writeln!(out, "    {line}");
            }
        }
    }
    match vehicle_read_gate(READ_LAYOUT_RECOVERED, owner_opt_in) {
        Ok(()) => {
            // Unreachable while READ_LAYOUT_RECOVERED is false. If this is ever
            // reached, the layout was recovered and the owner opted in.
            let _ = writeln!(
                out,
                "gate open: a live read is permitted. Not implemented here."
            );
            EXIT_HELD
        }
        Err(refusal) => {
            let _ = writeln!(out, "HELD: {refusal}");
            let _ = writeln!(out, "next: {}", refusal.next_step());
            let _ = writeln!(
                out,
                "then: spskit-vit1 --read <saved-read> --code <K17|...> --vin <VIN>",
            );
            EXIT_HELD
        }
    }
}

/// Open the VCX on `port` and read `GetInfo`.
///
/// This is the only path in the CLI that touches hardware. It opens the USB
/// serial port, writes the empty `GetInfo`, and parses the reply. It opens no
/// vehicle channel.
#[cfg(windows)]
fn probe(port: u32) -> Result<DeviceInfo, String> {
    use std::time::Duration;
    let com = crate::open_usb(port, 0).map_err(|error| error.to_string())?;
    let frame = crate::exchange(
        &com,
        crate::Command::GetInfo.header(0),
        &[],
        Duration::from_secs(2),
    )
    .map_err(|error| error.to_string())?;
    crate::device_info(&frame).map_err(|error| error.to_string())
}

/// Without Windows, the USB open is not compiled, so the pre-flight cannot run.
#[cfg(not(windows))]
fn probe(_port: u32) -> Result<DeviceInfo, String> {
    Err("this build cannot open the VCX; the USB serial open is Windows only".to_owned())
}

/// Open the VCX on `port` and read the on-device license.
///
/// Like [`probe`], this touches only the dongle: the three `GetInfo`/challenge/
/// `GetLicense` reads ([`crate::read_license`]). It writes nothing back.
#[cfg(windows)]
fn probe_license(port: u32) -> Result<LicenseInfo, String> {
    let com = crate::open_usb(port, 0).map_err(|error| error.to_string())?;
    crate::read_license(&com, std::time::Duration::from_secs(3)).map_err(|error| error.to_string())
}

#[cfg(not(windows))]
fn probe_license(_port: u32) -> Result<LicenseInfo, String> {
    Err("this build cannot open the VCX; the USB serial open is Windows only".to_owned())
}

/// The catalog's newer firmware version for `pid` over `installed`, if any.
///
/// Fetches the catalog and the newer image over `WinHTTP`
/// ([`crate::fetch_newer_firmware`]) and reads its header version. A read only.
#[cfg(windows)]
fn newer_version(pid: u32, installed: [u8; 4]) -> Result<Option<[u8; 4]>, String> {
    match crate::fetch_newer_firmware(pid, installed).map_err(|error| error.to_string())? {
        Some(bytes) => Ok(Some(
            crate::read_header(&bytes)
                .map_err(|error| error.to_string())?
                .version,
        )),
        None => Ok(None),
    }
}

#[cfg(not(windows))]
fn newer_version(_pid: u32, _installed: [u8; 4]) -> Result<Option<[u8; 4]>, String> {
    Err("this build cannot fetch the catalog; WinHTTP is Windows only".to_owned())
}

#[cfg(test)]
mod tests {
    use super::{
        Cli, CliError, DEFAULT_PORT, EXIT_HELD, EXIT_OK, EXIT_USAGE, describe_device,
        describe_flash_plan, describe_license, describe_update_check, fw_version, parse,
        parse_port, run,
    };
    use crate::DeviceInfo;
    use crate::info::FirmwareDate;
    use crate::license_info::{LicenseDate, Product};

    fn args(raw: &[&str]) -> Vec<String> {
        raw.iter().map(|s| (*s).to_owned()).collect()
    }

    fn run_to_string(raw: &[&str]) -> (i32, String) {
        let mut buf = Vec::new();
        let code = run(&args(raw), &mut buf);
        (code, String::from_utf8(buf).unwrap())
    }

    fn nano() -> DeviceInfo {
        DeviceInfo {
            serial: "NCAGM26507".to_owned(),
            hw_name: "VCX-NANO".to_owned(),
            fw_version: [1, 9, 4, 2],
            fw_date: FirmwareDate {
                year: 2023,
                month: 3,
                day: 31,
            },
            active_flag: 1,
            do_net_flag: 0,
        }
    }

    #[test]
    fn a_port_is_parsed_from_com_or_a_bare_number() {
        assert_eq!(parse_port("COM4"), Some(4));
        assert_eq!(parse_port("com10"), Some(10));
        assert_eq!(parse_port("4"), Some(4));
        assert_eq!(parse_port("tty"), None);
    }

    #[test]
    fn doctor_defaults_to_com4() {
        assert_eq!(
            parse(&args(&["doctor"])),
            Ok(Cli::Doctor { port: DEFAULT_PORT })
        );
        assert_eq!(
            parse(&args(&["info"])),
            Ok(Cli::Doctor { port: DEFAULT_PORT })
        );
        assert_eq!(
            parse(&args(&["doctor", "--port", "COM7"])),
            Ok(Cli::Doctor { port: 7 })
        );
    }

    #[test]
    fn a_bad_port_is_a_usage_error() {
        assert_eq!(
            parse(&args(&["doctor", "--port", "nope"])),
            Err(CliError::BadValue {
                flag: "--port".to_owned(),
                value: "nope".to_owned(),
            }),
        );
        assert_eq!(
            parse(&args(&["doctor", "--port"])),
            Err(CliError::MissingValue("--port".to_owned())),
        );
    }

    #[test]
    fn collect_needs_an_ecu_and_defaults_the_identifiers() {
        assert_eq!(
            parse(&args(&["collect"])),
            Err(CliError::MissingFlag("--ecu".to_owned())),
        );
        let cli = parse(&args(&["collect", "--ecu", "BECM"])).unwrap();
        match cli {
            Cli::Collect {
                port,
                ecu,
                lids,
                owner_opt_in,
            } => {
                assert_eq!(port, DEFAULT_PORT);
                assert_eq!(ecu, "BECM");
                assert_eq!(lids, crate::collect::DEFAULT_IDENTITY_LIDS.to_vec());
                assert!(!owner_opt_in);
            }
            other => panic!("not a collect: {other:?}"),
        }
    }

    #[test]
    fn collect_takes_explicit_identifiers_and_the_owner_flag() {
        let cli = parse(&args(&[
            "collect",
            "--ecu",
            "ABS",
            "--lid",
            "C1",
            "--lid",
            "CC",
            "--owner-unhold",
        ]))
        .unwrap();
        assert_eq!(
            cli,
            Cli::Collect {
                port: DEFAULT_PORT,
                ecu: "ABS".to_owned(),
                lids: vec![0xC1, 0xCC],
                owner_opt_in: true,
            },
        );
    }

    #[test]
    fn an_unknown_subcommand_is_refused() {
        assert_eq!(
            parse(&args(&["program"])),
            Err(CliError::UnknownSubcommand("program".to_owned())),
        );
        assert_eq!(parse(&[]), Err(CliError::NoSubcommand));
    }

    #[test]
    fn fw_version_is_dotted() {
        assert_eq!(fw_version([1, 9, 4, 2]), "1.9.4.2");
    }

    #[test]
    fn the_doctor_verdict_names_the_model_firmware_and_serial() {
        let text = describe_device(&nano(), 4);
        assert!(text.contains("VCX connected on COM4"), "{text}");
        assert!(text.contains("model:    VCX-NANO"), "{text}");
        assert!(text.contains("firmware: 1.9.4.2"), "{text}");
        assert!(text.contains("serial:   present (NCAGM26507)"), "{text}");
    }

    #[test]
    fn an_absent_serial_is_reported() {
        let mut info = nano();
        info.serial.clear();
        assert!(describe_device(&info, 4).contains("serial:   absent"));
    }

    #[test]
    fn collect_refuses_to_transmit_and_names_the_next_step() {
        let (code, text) = run_to_string(&["collect", "--ecu", "BECM"]);
        assert_eq!(code, EXIT_HELD);
        assert!(text.contains("HELD:"), "{text}");
        assert!(text.contains("VCX_PRO_Recv"), "{text}");
        assert!(text.contains("spskit-vit1"), "{text}");
        // The plan is shown but nothing is sent.
        assert!(text.contains("nothing sent"), "{text}");
        assert!(text.contains("0x8001"), "{text}");
    }

    #[test]
    fn the_owner_flag_alone_does_not_open_the_gate() {
        // The recovered-layout gate still holds the send.
        let (code, text) = run_to_string(&["collect", "--ecu", "BECM", "--owner-unhold"]);
        assert_eq!(code, EXIT_HELD);
        assert!(text.contains("HELD:"), "{text}");
        assert!(text.contains("not recovered"), "{text}");
    }

    #[test]
    fn help_prints_usage() {
        let (code, text) = run_to_string(&["help"]);
        assert_eq!(code, EXIT_OK);
        assert!(text.contains("USAGE:"), "{text}");
    }

    #[test]
    fn a_usage_error_prints_usage_and_exits_one() {
        let (code, text) = run_to_string(&["frobnicate"]);
        assert_eq!(code, EXIT_USAGE);
        assert!(text.contains("unknown subcommand"), "{text}");
        assert!(text.contains("USAGE:"), "{text}");
    }

    #[test]
    fn the_device_subcommands_parse_their_flags() {
        assert_eq!(
            parse(&args(&["license"])),
            Ok(Cli::License { port: DEFAULT_PORT })
        );
        assert_eq!(
            parse(&args(&["firmware", "--pid", "837"])),
            Ok(Cli::Firmware {
                port: DEFAULT_PORT,
                pid: Some(837),
            })
        );
        assert_eq!(
            parse(&args(&["firmware"])),
            Ok(Cli::Firmware {
                port: DEFAULT_PORT,
                pid: None,
            })
        );
        // flash needs an --image.
        assert_eq!(
            parse(&args(&["flash"])),
            Err(CliError::MissingFlag("--image".to_owned()))
        );
        assert_eq!(
            parse(&args(&["flash", "--image", "fw.bin", "--owner-flash"])),
            Ok(Cli::Flash {
                port: DEFAULT_PORT,
                image: "fw.bin".to_owned(),
                owner_opt_in: true,
            })
        );
        assert_eq!(
            parse(&args(&["firmware", "--pid", "notanumber"])),
            Err(CliError::BadValue {
                flag: "--pid".to_owned(),
                value: "notanumber".to_owned(),
            })
        );
    }

    fn license() -> crate::LicenseInfo {
        crate::LicenseInfo {
            serial: [0x4E, 0x43, 0x41, 0x47, 0x4D, 0x67, 0x8B, 0x28],
            issued: LicenseDate {
                year: 2023,
                month: 9,
                day: 22,
            },
            updated: LicenseDate {
                year: 2026,
                month: 11,
                day: 20,
            },
            period_seconds: 0x004F_1A00,
            products: vec![
                Product {
                    name: "PASSTHRU".to_owned(),
                    date: LicenseDate {
                        year: 2030,
                        month: 12,
                        day: 31,
                    },
                },
                Product {
                    name: "GMGDS".to_owned(),
                    date: LicenseDate {
                        year: 2030,
                        month: 12,
                        day: 31,
                    },
                },
            ],
        }
    }

    #[test]
    fn describe_license_lists_the_products_and_dates() {
        let text = describe_license(&license());
        assert!(text.contains("serial:  4E4341474D678B28"), "{text}");
        assert!(text.contains("issued:  2023-09-22"), "{text}");
        assert!(text.contains("PASSTHRU (valid 2030-12-31)"), "{text}");
        assert!(text.contains("GMGDS (valid 2030-12-31)"), "{text}");
    }

    #[test]
    fn describe_update_check_reports_newer_or_up_to_date() {
        assert_eq!(
            describe_update_check([1, 9, 4, 2], Some([1, 9, 6, 0])),
            "installed firmware 1.9.4.2; update available: 1.9.6.0"
        );
        assert_eq!(
            describe_update_check([1, 9, 4, 2], None),
            "installed firmware 1.9.4.2; up to date"
        );
    }

    // A minimal valid firmware image with `pages` page records (as update.rs tests).
    fn firmware_image(pages: u32) -> Vec<u8> {
        let mut bytes = vec![0_u8; 0x24];
        bytes[0..4].copy_from_slice(&[0xA5; 4]);
        bytes[4..12].copy_from_slice(b"VCX-At40");
        bytes[0x0C..0x10].copy_from_slice(&0x0109_0100_u32.to_le_bytes());
        bytes[0x20..0x24].copy_from_slice(&pages.to_le_bytes());
        for _ in 0..pages {
            bytes.extend_from_slice(&4_u32.to_le_bytes());
            bytes.extend_from_slice(&0_u32.to_le_bytes());
            bytes.extend_from_slice(&[1, 2, 3, 4]);
        }
        bytes
    }

    #[test]
    fn the_flash_plan_is_held_and_never_writes() {
        // Even with the owner opt-in, the unrecovered flash sequence holds the write.
        let (text, code) = describe_flash_plan(&firmware_image(1), [1, 9, 4, 2], true).unwrap();
        assert_eq!(code, EXIT_HELD);
        assert!(text.contains("nothing written to the device"), "{text}");
        assert!(text.contains("HELD:"), "{text}");
        assert!(text.contains("not recovered"), "{text}");
        // The recovered local steps and the refused wire steps both appear.
        assert!(text.contains("local:"), "{text}");
        assert!(
            text.contains("WrFlash") || text.contains("not sent"),
            "{text}"
        );
    }

    #[test]
    fn a_broken_firmware_image_is_a_usage_error() {
        let (code, text) = run_to_string(&["flash", "--image", "definitely-not-a-file.bin"]);
        assert_eq!(code, EXIT_USAGE);
        assert!(text.contains("cannot read image"), "{text}");
    }
}
