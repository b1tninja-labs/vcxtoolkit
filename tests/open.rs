//! Opens that need a Windows COM port.

#[cfg(windows)]
#[test]
fn a_missing_com_port_fails() {
    let error = vcxtoolkit::open_usb(255, 0).expect_err("COM255 is not the dongle");
    assert_eq!(error.call(), vcxtoolkit::Call::CreateFile);
    assert_eq!(error.io().kind(), std::io::ErrorKind::NotFound, "{error}");
}

#[cfg(windows)]
#[test]
#[ignore = "needs the CH343 dongle on COM4"]
fn com4_is_quiet_until_spoken_to() {
    let port = vcxtoolkit::open_usb(4, 0).expect("COM4");
    let state = port.state().expect("line");
    assert_eq!(state.baud, vcxtoolkit::USB_BAUD);
    assert_eq!(state.byte_size, 8);
    assert_eq!(state.parity, 0);
    assert_eq!(state.stop_bits, 0);
    assert_eq!((state.flags >> 4) & 0b11, 1, "DTR");
    assert_eq!(state.flags & (1 << 3), 0, "DSR flow");
    assert_eq!(state.flags & (1 << 6), 0, "DSR sensitivity");
    assert_eq!(state.read_interval_timeout, u32::MAX);
    assert_eq!(state.read_total_timeout_multiplier, 0);
    assert_eq!(state.read_total_timeout_constant, 0);
    assert_eq!(state.write_total_timeout_multiplier, 0);
    assert_eq!(state.write_total_timeout_constant, 0);
    assert_eq!(state.event_mask, 0x85);
    let mut buf = [0_u8; 256];
    let got = port.read(&mut buf).expect("read");
    assert_eq!(got, 0, "the dongle spoke: {:02x?}", &buf[..got]);
}
