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

/// Low nibble of a flow-control PCI byte. ISO 15765-2:2016 §9.6.5.1 Table 18.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FlowStatus {
    /// `0000`. Continue to send.
    Continue,
    /// `0001`. Wait for another flow-control frame.
    Wait,
    /// `0010`. Abort. The received length does not fit.
    Overflow,
    /// `0011` through `1111`.
    Reserved(u8),
}

/// Byte 3 of a flow-control PCI. ISO 15765-2:2016 §9.6.5.4 Table 20.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Separation {
    /// `0x00` through `0x7F`, in milliseconds.
    Milliseconds(u8),
    /// `0xF1` through `0xF9`, in microseconds. `0xF1` is 100 and `0xF9` is 900.
    Microseconds(u16),
    /// `0x80` through `0xF0`, and `0xFA` through `0xFF`.
    ///
    /// §9.6.5.5 says the sender uses 127 ms for the rest of that message.
    /// The received byte is kept here.
    Reserved(u8),
}

/// The three flow-control PCI bytes. ISO 15765-2:2016 §9.6.5.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FlowControl {
    /// Low nibble of byte 1.
    ///
    /// For [`FlowStatus::Wait`] and [`FlowStatus::Overflow`], §9.6.5.1 says
    /// the block size and the separation time are ignored. Both are still
    /// returned.
    pub status: FlowStatus,
    /// Byte 2. `0` means the sender finishes without another flow-control
    /// frame. ISO 15765-2:2016 §9.6.5.3.
    pub block_size: u8,
    /// Byte 3.
    pub separation: Separation,
}

/// The three flow-control PCI bytes, when `frame` starts at the PCI byte.
///
/// That byte is the first CAN data byte for normal and normal fixed addressing
/// (ISO 15765-2:2016 §10.3.2 and §10.3.3). Extended addressing puts `N_TA` in
/// the first data byte (§10.3.4), and mixed addressing puts `N_AE` there
/// (§10.3.5). This does not skip an address byte. The pass-through copy does
/// not say which format the caller used. A kind-3 filter's flow bytes are a
/// separate copy and are not read here.
#[must_use]
pub fn flow_control(frame: &[u8]) -> Option<FlowControl> {
    let first = *frame.first()?;
    if !matches!(pci_type(first), PciType::Flow) || frame.len() < 3 {
        return None;
    }
    Some(FlowControl {
        status: flow_status(first & 0x0F),
        block_size: frame[1],
        separation: separation(frame[2]),
    })
}

fn flow_status(nibble: u8) -> FlowStatus {
    match nibble {
        0 => FlowStatus::Continue,
        1 => FlowStatus::Wait,
        2 => FlowStatus::Overflow,
        reserved => FlowStatus::Reserved(reserved),
    }
}

fn separation(byte: u8) -> Separation {
    match byte {
        0x00..=0x7F => Separation::Milliseconds(byte),
        0xF1..=0xF9 => Separation::Microseconds(u16::from(byte - 0xF0) * 100),
        reserved => Separation::Reserved(reserved),
    }
}

/// `NR_SI`. ISO 14229-1:2013 §7.3.3. It is not a service id.
pub const NEGATIVE_RESPONSE: u8 = 0x7F;

/// Service id at byte 1 of a classical single frame.
///
/// The length nibble is the number of service bytes, and byte 1 is the first
/// of them. ISO 14229-1:2013 §7.2 puts `A_PCI` at the start of `A_Data`, and
/// §7.3.2 says that byte is the service id. The slice starts at the PCI byte,
/// which is the first CAN data byte only for normal and normal fixed
/// addressing. A first, consecutive, flow, or escaped single frame has no
/// service id here.
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
    use super::{
        FlowControl, FlowStatus, NEGATIVE_RESPONSE, PciType, Separation, flow_control, pci_type,
        single_service,
    };

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

    #[test]
    fn a_flow_control_frame_is_three_bytes() {
        assert_eq!(
            flow_control(&[0x30, 0x00, 0x0A]),
            Some(FlowControl {
                status: FlowStatus::Continue,
                block_size: 0,
                separation: Separation::Milliseconds(10),
            })
        );
        assert_eq!(
            flow_control(&[0x31, 0x08, 0xF1]),
            Some(FlowControl {
                status: FlowStatus::Wait,
                block_size: 8,
                separation: Separation::Microseconds(100),
            })
        );
        assert_eq!(
            flow_control(&[0x32, 0x01, 0xF9]),
            Some(FlowControl {
                status: FlowStatus::Overflow,
                block_size: 1,
                separation: Separation::Microseconds(900),
            })
        );
        assert_eq!(
            flow_control(&[0x33, 0x01, 0x80]),
            Some(FlowControl {
                status: FlowStatus::Reserved(3),
                block_size: 1,
                separation: Separation::Reserved(0x80),
            })
        );
        assert_eq!(
            flow_control(&[0x30, 0x00, 0xFA]).unwrap().separation,
            Separation::Reserved(0xFA)
        );
        assert_eq!(flow_control(&[0x10, 0x00, 0x00]), None);
        assert_eq!(flow_control(&[0x30, 0x00]), None);
    }
}
