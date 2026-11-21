//! The first PCI byte from ISO 15765-2:2016 Table 8, and the service id that
//! follows a classical single frame.
//!
//! The pass-through copies the caller's bytes unchanged. This module names
//! those bytes when the caller already formed them. It does not wrap a
//! service, and it does not send a frame.

/// High nibble of `N_PCI` byte 1. ISO 15765-2:2016 §9.6.1 Table 8.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PciType {
    /// `0000`. [`None`] is a low nibble of 0: the classical length is not in
    /// this byte. ISO 15765-2:2016 §9.6.2 uses that nibble as an escape when
    /// `CAN_DL` is greater than 8, and §9.6.2.2 says a classical single frame
    /// whose length is 0 is ignored. The copy has no `CAN_DL`, so this does
    /// not choose between those two.
    Single {
        /// Low nibble, when it is not 0.
        length: Option<u8>,
    },
    /// `0001`. First frame.
    First,
    /// `0010`. Consecutive frame.
    Consecutive,
    /// `0011`. Flow control.
    Flow,
    /// `0100` through `1111`. Reserved by ISO 15765-2:2016 Table 8.
    Reserved,
}

/// `N_PCI` type of the first byte. ISO 15765-2:2016 §9.6.1 Table 8.
#[must_use]
pub fn pci_type(first: u8) -> PciType {
    match first >> 4 {
        0 => {
            let nibble = first & 0x0F;
            PciType::Single {
                length: if nibble == 0 { None } else { Some(nibble) },
            }
        }
        1 => PciType::First,
        2 => PciType::Consecutive,
        3 => PciType::Flow,
        _ => PciType::Reserved,
    }
}

/// `NR_SI`. ISO 14229-1:2013 §7.3.3. It is not a service id.
pub const NEGATIVE_RESPONSE: u8 = 0x7F;

/// Service id at byte 1 of a classical single frame.
///
/// The length nibble is the number of service bytes, and byte 1 is the first
/// of them. ISO 14229-1:2013 §7.2 puts `A_PCI` at the start of `A_Data`, and
/// §7.3.2 says that byte is the service id. A first, consecutive, flow, or
/// escaped single frame has no service id here.
#[must_use]
pub fn single_service(frame: &[u8]) -> Option<u8> {
    let PciType::Single {
        length: Some(length),
    } = pci_type(*frame.first()?)
    else {
        return None;
    };
    if usize::from(length) > frame.len().saturating_sub(1) {
        return None;
    }
    frame.get(1).copied()
}

#[cfg(test)]
mod tests {
    use super::{NEGATIVE_RESPONSE, PciType, pci_type, single_service};

    #[test]
    fn the_high_nibble_is_the_pci_type() {
        assert_eq!(pci_type(0x02), PciType::Single { length: Some(2) });
        assert_eq!(pci_type(0x00), PciType::Single { length: None });
        assert_eq!(pci_type(0x10), PciType::First);
        assert_eq!(pci_type(0x21), PciType::Consecutive);
        assert_eq!(pci_type(0x30), PciType::Flow);
        assert_eq!(pci_type(0x40), PciType::Reserved);
        assert_eq!(single_service(&[0x02, 0x10, 0x01]), Some(0x10));
        assert_eq!(
            single_service(&[0x03, NEGATIVE_RESPONSE, 0x10, 0x11]),
            Some(0x7F)
        );
        assert_eq!(single_service(&[0x10, 0x10]), None);
        assert_eq!(single_service(&[0x03, 0x10]), None);
    }
}
