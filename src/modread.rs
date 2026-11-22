//! The saved "module read" text this crate would write for a collected identity.
//!
//! The format is the one `voltkit profile --clone` already emits and `spskit`'s
//! `spskit-vit1` CLI already reads: an `ecu` block, then one `lid`/`did` line per
//! answered identifier. A `lid` line is a service `$1A` read; a `did` line is a
//! service `$22` read. The hex on each line is the data the module returned after
//! the service byte and the identifier, which is what `spskit.handoff` slices into
//! the VIT1 control-module block. Nothing here reads a vehicle: this module only
//! serializes reads already in hand. The collection that would fill them is held
//! and gated in [`crate::collect`].

use std::fmt::Write as _;

/// The service byte of a GMLAN data-by-identifier read, logged as a `lid` line.
pub const SERVICE_READ_DATA_BY_IDENTIFIER: u8 = 0x1A;
/// The service byte of a UDS read-data-by-identifier read, logged as a `did` line.
pub const SERVICE_READ_DATA_BY_DID: u8 = 0x22;

/// The `s3` session-timing line `voltkit` writes at the top of a clone profile.
pub const DEFAULT_S3_MS: u32 = 5000;

/// Why a saved read could not be serialized.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModuleReadError {
    /// The identifier does not fit the width the service's line uses.
    IdentifierTooWide,
    /// The service is neither `$1A` (`lid`) nor `$22` (`did`).
    UnknownService(u8),
    /// The ECU name is empty, or carries whitespace a line would split on.
    BadEcuName,
}

impl std::fmt::Display for ModuleReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IdentifierTooWide => f.write_str("a lid identifier does not fit one byte"),
            Self::UnknownService(service) => {
                write!(f, "service {service:#04x} is not a lid or a did read")
            }
            Self::BadEcuName => f.write_str("an ecu name is empty or carries whitespace"),
        }
    }
}

impl std::error::Error for ModuleReadError {}

/// One answered identifier: its service, its identifier, and the returned data.
///
/// `data` is the payload after the service byte and the identifier, exactly the
/// bytes `spskit.handoff` reads back as the `lid`/`did` value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdentifierRead {
    /// [`SERVICE_READ_DATA_BY_IDENTIFIER`] or [`SERVICE_READ_DATA_BY_DID`].
    pub service: u8,
    /// The identifier. A `lid` identifier is one byte; a `did` identifier is two.
    pub identifier: u16,
    /// The data the module returned after the service byte and the identifier.
    pub data: Vec<u8>,
}

impl IdentifierRead {
    /// A service `$1A` `lid` read.
    #[must_use]
    pub fn lid(identifier: u8, data: Vec<u8>) -> Self {
        Self {
            service: SERVICE_READ_DATA_BY_IDENTIFIER,
            identifier: u16::from(identifier),
            data,
        }
    }

    /// A service `$22` `did` read.
    #[must_use]
    pub fn did(identifier: u16, data: Vec<u8>) -> Self {
        Self {
            service: SERVICE_READ_DATA_BY_DID,
            identifier,
            data,
        }
    }

    /// The clone-profile line for this read, for example `lid C1 0000002A`.
    ///
    /// # Errors
    ///
    /// Returns [`ModuleReadError::UnknownService`] when the service is not a
    /// `lid` or `did` read, and [`ModuleReadError::IdentifierTooWide`] when a
    /// `lid` identifier does not fit one byte.
    pub fn line(&self) -> Result<String, ModuleReadError> {
        let hex = hex_upper(&self.data);
        match self.service {
            SERVICE_READ_DATA_BY_IDENTIFIER => {
                let ident = u8::try_from(self.identifier)
                    .map_err(|_| ModuleReadError::IdentifierTooWide)?;
                Ok(format!("lid {ident:02X} {hex}"))
            }
            SERVICE_READ_DATA_BY_DID => Ok(format!("did {:04X} {hex}", self.identifier)),
            other => Err(ModuleReadError::UnknownService(other)),
        }
    }
}

/// One `ecu` block of a saved module read.
///
/// The header fields print on the `ecu` line as a documented superset of what
/// `spskit`'s parser needs: that parser reads only the `ecu` name and the
/// `lid`/`did` lines, so the request/response headers and the functional flag ride
/// along for the operator without changing what `spskit-vit1` consumes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleRead {
    /// The controller name, for example `BECM`.
    pub ecu: String,
    /// The request header (the `$7E4`-style CAN id), printed on the `ecu` line.
    pub request_header: u16,
    /// The response header, printed on the `ecu` line.
    pub response_header: u16,
    /// Whether the request was functional, printed as `functional=yes`/`no`.
    pub functional: bool,
    /// The answered identifiers, in the order they were read.
    pub reads: Vec<IdentifierRead>,
}

impl ModuleRead {
    /// A block with the given name and headers and no reads yet.
    ///
    /// # Errors
    ///
    /// Returns [`ModuleReadError::BadEcuName`] when `ecu` is empty or carries
    /// whitespace, which would split the `ecu` line.
    pub fn new(
        ecu: impl Into<String>,
        request_header: u16,
        response_header: u16,
        functional: bool,
    ) -> Result<Self, ModuleReadError> {
        let ecu = ecu.into();
        if ecu.is_empty() || ecu.chars().any(char::is_whitespace) {
            return Err(ModuleReadError::BadEcuName);
        }
        Ok(Self {
            ecu,
            request_header,
            response_header,
            functional,
            reads: Vec::new(),
        })
    }

    /// Append one answered identifier.
    pub fn push(&mut self, read: IdentifierRead) {
        self.reads.push(read);
    }

    /// The `ecu` line, for example `ecu BECM req=7E4 resp=7EC functional=no pad=00 latency=0`.
    #[must_use]
    pub fn ecu_line(&self) -> String {
        let functional = if self.functional { "yes" } else { "no" };
        format!(
            "ecu {} req={:03X} resp={:03X} functional={functional} pad=00 latency=0",
            self.ecu, self.request_header, self.response_header
        )
    }
}

/// Serialize one or more module reads as a clone profile `spskit-vit1` can read.
///
/// The output opens with the comment banner and the `s3` line `voltkit` writes,
/// then one `ecu` block per read. An empty `reads` list on a block is allowed: the
/// block header still prints, and `spskit` reports the missing identifier itself.
///
/// # Errors
///
/// Returns the first [`ModuleReadError`] a line raises.
pub fn clone_profile(reads: &[ModuleRead]) -> Result<String, ModuleReadError> {
    let mut out = String::from(
        "# Captured module bytes. A did or lid line is the data after the service and the identifier.\n\
         # Written by vcxtoolkit. This file feeds spskit-vit1 --read; it is not a vehicle write.\n",
    );
    let _ = writeln!(out, "s3 {DEFAULT_S3_MS}");
    for read in reads {
        let _ = writeln!(out, "{}", read.ecu_line());
        for identifier in &read.reads {
            let _ = writeln!(out, "{}", identifier.line()?);
        }
    }
    Ok(out)
}

fn hex_upper(data: &[u8]) -> String {
    let mut hex = String::with_capacity(data.len() * 2);
    for byte in data {
        let _ = write!(hex, "{byte:02X}");
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::{IdentifierRead, ModuleRead, ModuleReadError, clone_profile, hex_upper};

    #[test]
    fn a_lid_line_is_the_identifier_byte_and_the_hex_data() {
        let read = IdentifierRead::lid(0xC1, vec![0x00, 0x00, 0x00, 0x2A]);
        assert_eq!(read.line().unwrap(), "lid C1 0000002A");
    }

    #[test]
    fn a_did_line_pads_the_identifier_to_four_hex_digits() {
        let read = IdentifierRead::did(0x4532, vec![0x00, 0x01]);
        assert_eq!(read.line().unwrap(), "did 4532 0001");
    }

    #[test]
    fn an_empty_read_is_an_empty_hex_field() {
        // spskit's parser reads an empty hex token as empty bytes, so an answered
        // identifier with no data is a well-formed line.
        assert_eq!(
            IdentifierRead::lid(0xCC, Vec::new()).line().unwrap(),
            "lid CC "
        );
    }

    #[test]
    fn a_lid_identifier_over_one_byte_is_refused() {
        let read = IdentifierRead {
            service: super::SERVICE_READ_DATA_BY_IDENTIFIER,
            identifier: 0x0100,
            data: Vec::new(),
        };
        assert_eq!(read.line(), Err(ModuleReadError::IdentifierTooWide));
    }

    #[test]
    fn an_unknown_service_is_named() {
        let read = IdentifierRead {
            service: 0x27,
            identifier: 1,
            data: Vec::new(),
        };
        assert_eq!(read.line(), Err(ModuleReadError::UnknownService(0x27)));
    }

    #[test]
    fn an_ecu_name_with_a_space_is_refused() {
        assert_eq!(
            ModuleRead::new("BE CM", 0x7E4, 0x7EC, false),
            Err(ModuleReadError::BadEcuName),
        );
        assert_eq!(
            ModuleRead::new("", 0x7E4, 0x7EC, false),
            Err(ModuleReadError::BadEcuName),
        );
    }

    #[test]
    fn the_ecu_line_carries_the_headers_and_the_functional_flag() {
        let block = ModuleRead::new("BECM", 0x7E4, 0x7EC, false).unwrap();
        assert_eq!(
            block.ecu_line(),
            "ecu BECM req=7E4 resp=7EC functional=no pad=00 latency=0",
        );
    }

    #[test]
    fn a_profile_is_the_banner_the_s3_line_and_one_ecu_block() {
        let mut block = ModuleRead::new("BECM", 0x7E4, 0x7EC, true).unwrap();
        block.push(IdentifierRead::lid(0xC1, vec![0x00, 0x00, 0x00, 0x2A]));
        block.push(IdentifierRead::did(0x4532, vec![0x00, 0x01]));
        let text = clone_profile(&[block]).unwrap();
        assert!(text.contains("s3 5000\n"), "{text}");
        assert!(
            text.contains("ecu BECM req=7E4 resp=7EC functional=yes pad=00 latency=0\n"),
            "{text}",
        );
        assert!(text.contains("\nlid C1 0000002A\n"), "{text}");
        assert!(text.contains("\ndid 4532 0001\n"), "{text}");
        // The block header is the second non-comment line, after the s3 line, so
        // spskit groups the reads under it.
        let lines: Vec<&str> = text.lines().filter(|line| !line.starts_with('#')).collect();
        assert_eq!(lines[0], "s3 5000");
        assert!(lines[1].starts_with("ecu BECM "), "{lines:?}");
    }

    #[test]
    fn hex_upper_is_two_uppercase_digits_per_byte() {
        assert_eq!(hex_upper(&[0x0a, 0xff, 0x00]), "0AFF00");
        assert_eq!(hex_upper(&[]), "");
    }
}
