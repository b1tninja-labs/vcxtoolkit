//! The J2534 `PASSTHRU_MSG` struct and a safe owned view of one message.
//!
//! The C layout is the ABI the DLL shares with the app; the FFI layer
//! ([`crate::ffi`]) is the only place a raw `PASSTHRU_MSG` is read or written.
//! Everything below is plain data with no pointers, so the rest of the crate
//! reasons in safe Rust.

/// The largest `Data` a `PASSTHRU_MSG` carries. J2534-1 fixes it at 4128 bytes,
/// which is `0x1020` — the same cap [`vcxtoolkit::MAX_FORWARD`] found in the
/// dongle's copy routine, so a message that fits J2534 fits the VCX Send PDU.
pub const DATA_MAX: usize = 4128;

const _: () = assert!(DATA_MAX == vcxtoolkit::MAX_FORWARD);

/// The J2534-1 `PASSTHRU_MSG`, laid out for the C ABI.
///
/// Field names and order are the J2534-1 API's. `Data` is a fixed array because
/// the app allocates the whole struct; `DataSize` says how many bytes are live.
/// This type crosses the FFI boundary only; [`Msg`] is the safe form.
#[repr(C)]
#[derive(Clone, Copy)]
#[allow(non_snake_case)]
pub struct PassThruMsg {
    /// The protocol the message belongs to (a [`crate::status::Protocol`] id).
    pub ProtocolID: u32,
    /// Receive-status bits on a read; 0 on a write.
    pub RxStatus: u32,
    /// Transmit flags on a write (e.g. `CAN_29BIT_ID`, `ISO15765_FRAME_PAD`).
    pub TxFlags: u32,
    /// Device timestamp in microseconds on a read.
    pub Timestamp: u32,
    /// Live length of `Data`.
    pub DataSize: u32,
    /// Start of the extra (non-header) data, for some protocols.
    pub ExtraDataIndex: u32,
    /// The message bytes. Only the first `DataSize` are meaningful.
    pub Data: [u8; DATA_MAX],
}

/// A safe, owned J2534 message: the header fields plus just the live `Data`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Msg {
    /// `ProtocolID`.
    pub protocol: u32,
    /// `RxStatus`.
    pub rx_status: u32,
    /// `TxFlags`.
    pub tx_flags: u32,
    /// The live message bytes (`Data[..DataSize]`).
    pub data: Vec<u8>,
}

/// Why a raw `PASSTHRU_MSG` could not be read as a safe [`Msg`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MsgError {
    /// `DataSize` is larger than [`DATA_MAX`]; the struct is malformed.
    DataSizeTooLarge,
}

impl Msg {
    /// A safe message from the fields of a `PASSTHRU_MSG`.
    ///
    /// Kept free of any pointer so it is callable from a unit test: the FFI layer
    /// pulls `data_size` and the `data` slice out of the raw struct and hands
    /// them here.
    ///
    /// # Errors
    ///
    /// Returns [`MsgError::DataSizeTooLarge`] when `data_size` exceeds the bytes
    /// a `PASSTHRU_MSG` can hold.
    pub fn from_fields(
        protocol: u32,
        rx_status: u32,
        tx_flags: u32,
        data_size: u32,
        data: &[u8],
    ) -> Result<Self, MsgError> {
        let len = usize::try_from(data_size).map_err(|_| MsgError::DataSizeTooLarge)?;
        let slice = data.get(..len).ok_or(MsgError::DataSizeTooLarge)?;
        Ok(Self {
            protocol,
            rx_status,
            tx_flags,
            data: slice.to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{DATA_MAX, Msg, MsgError, PassThruMsg};

    #[test]
    fn the_data_cap_is_the_dongle_forward_cap() {
        // If J2534's 4128 and the VCX 0x1020 ever diverged, a message that an app
        // considers valid could overflow the Send PDU. The const assert above
        // guards it; this pins the number.
        assert_eq!(DATA_MAX, 0x1020);
        assert_eq!(DATA_MAX, vcxtoolkit::MAX_FORWARD);
    }

    #[test]
    fn a_message_keeps_only_its_live_data() {
        let mut data = [0_u8; DATA_MAX];
        data[..6].copy_from_slice(&[0x00, 0x00, 0x07, 0xE0, 0x1A, 0xC1]);
        let msg = Msg::from_fields(6, 0, 0x40, 6, &data).unwrap();
        assert_eq!(msg.protocol, 6);
        assert_eq!(msg.tx_flags, 0x40);
        assert_eq!(msg.data, vec![0x00, 0x00, 0x07, 0xE0, 0x1A, 0xC1]);
    }

    #[test]
    fn a_data_size_past_the_buffer_is_refused() {
        let data = [0_u8; 8];
        assert_eq!(
            Msg::from_fields(6, 0, 0, 9, &data),
            Err(MsgError::DataSizeTooLarge)
        );
    }

    #[test]
    fn the_struct_is_c_sized() {
        // 6 u32 header fields then the fixed data array: a size the app and the
        // DLL must agree on byte-for-byte.
        assert_eq!(size_of::<PassThruMsg>(), 6 * size_of::<u32>() + DATA_MAX);
    }
}
