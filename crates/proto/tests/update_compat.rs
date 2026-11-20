#[allow(dead_code)]
#[path = "fixtures/protocol8.rs"]
mod old;
use sunna_proto::messages as new;
#[test]
fn current_main_and_new_read_identical_handshakes() {
    let messages = [
        new::ControlMessage::Hello { version: 8, name: "viewer".into(), token: "test-only".into(), stream: Default::default(), device: "device".into() },
        new::ControlMessage::HelloAck { version: 8, name: "host".into(), width: 640, height: 360, fps: 30, codec: "raw".into(), fast_lane: false, audio: false },
        new::ControlMessage::Probe { token: "test-only".into() },
        new::ControlMessage::ProbeAck { name: "host".into(), version: 8, busy: false, token_ok: true },
        new::ControlMessage::HostInfo(Default::default()),
        new::ControlMessage::InUse { viewer: "viewer".into(), device: "device".into() },
        new::ControlMessage::Bye,
    ];
    for message in messages {
        let bytes = new::encode(&message).unwrap();
        let before = old::decode(&bytes).unwrap();
        assert_eq!(old::encode(&before).unwrap(), bytes);
        assert_eq!(new::decode(&old::encode(&before).unwrap()).unwrap(), message);
    }
}
#[test]
fn unknown_variants_are_fatal_so_extensions_must_be_probe_only() {
    let bytes = new::encode(&new::ControlMessage::UpdateInfo("{}".into())).unwrap();
    assert!(old::decode(&bytes).is_err());
    // Postcard structs are positional: serde(default) is not a negotiation mechanism.
    let truncated = vec![6]; // Probe discriminant without its token.
    assert!(old::decode(&truncated).is_err());
    assert!(new::decode(&truncated).is_err());
}
