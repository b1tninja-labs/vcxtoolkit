//! The held collection, through the public API a downstream user sees.

#[test]
fn the_gate_is_closed_with_the_shipped_defaults() {
    // The recovered-layout flag is false, so even an owner opt-in is held: the
    // gate returns LayoutUnrecovered from the shipped constant.
    assert_eq!(
        vcxtoolkit::collect::vehicle_read_gate(vcxtoolkit::collect::READ_LAYOUT_RECOVERED, true),
        Err(vcxtoolkit::collect::GateRefusal::LayoutUnrecovered),
    );
}

#[test]
fn collect_builds_the_plan_but_refuses_to_transmit() {
    let args: Vec<String> = ["collect", "--ecu", "BECM"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    let mut out = Vec::new();
    let code = vcxtoolkit::cli::run(&args, &mut out);
    let text = String::from_utf8(out).unwrap();
    assert_eq!(code, vcxtoolkit::cli::EXIT_HELD);
    assert!(text.contains("nothing sent"), "{text}");
    assert!(text.contains("HELD:"), "{text}");
    assert!(text.contains("spskit-vit1"), "{text}");
}

#[test]
fn a_saved_read_serializes_in_the_spskit_clone_format() {
    let mut block = vcxtoolkit::modread::ModuleRead::new("BECM", 0x7E4, 0x7EC, false).unwrap();
    block.push(vcxtoolkit::modread::IdentifierRead::lid(
        0xC1,
        vec![0x00, 0x00, 0x00, 0x2A],
    ));
    let text = vcxtoolkit::modread::clone_profile(&[block]).unwrap();
    assert!(text.contains("\nlid C1 0000002A\n"), "{text}");
    assert!(text.contains("ecu BECM "), "{text}");
}
