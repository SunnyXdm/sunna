use sunna_transport::{auth, connect_peer, HostIdentity, Server};

#[test]
fn identity_is_stable_until_rotated() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("host.identity");
    let first = HostIdentity::load_or_create(&path).unwrap().pin().unwrap();
    assert_eq!(
        HostIdentity::load_or_create(&path).unwrap().pin().unwrap(),
        first
    );
    assert_ne!(HostIdentity::rotate(&path).unwrap().pin().unwrap(), first);
    std::fs::write(&path, []).unwrap();
    assert!(HostIdentity::load_or_create(&path).is_err());
}

#[tokio::test]
async fn proof_is_valid_only_on_its_connection() {
    let host = Server::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let addr = host.local_addr().unwrap();
    let pin = auth::certificate_pin(&host.cert).unwrap();
    let connect = || async {
        let (client, server) = tokio::join!(connect_peer(addr, "sunna", Some(&pin)), host.accept());
        (client.unwrap(), server.unwrap().unwrap())
    };
    let (first, first_host) = connect().await;
    let (_second, second_host) = connect().await;
    let key = "19".repeat(16);
    let (nonce, proof) = auth::client_proof(&first.connection, &key).unwrap();
    use sunna_proto::messages::{encode, ControlMessage, Proof};
    let request = ControlMessage::AuthProof {
        nonce: nonce.to_vec(),
        proof: Proof(proof.clone()),
    };
    assert!(!encode(&request)
        .unwrap()
        .windows(key.len())
        .any(|w| w == key.as_bytes()));
    assert!(format!("{request:?}").contains("[redacted]"));
    let (host_nonce, ack) = auth::answer(&first_host, &key, &nonce, &proof).unwrap();
    assert!(auth::verify_host(&first.connection, &key, &nonce, &host_nonce, &ack).is_ok());
    assert!(auth::answer(&second_host, &key, &nonce, &proof).is_err());
    assert!(auth::verify_host(&second_host, &key, &nonce, &host_nonce, &ack).is_err());
    assert!(auth::answer(&first_host, &"aa".repeat(16), &nonce, &proof).is_err());
    for size in [0, 31, 33, 256] {
        assert!(auth::answer(&first_host, &key, &vec![0; size], &proof).is_err());
    }
    assert!(auth::client_proof(&first.connection, "weak").is_err());
    for size in [0, 31, 33, 256] {
        assert!(auth::verify_host(&first.connection, &key, &nonce, &vec![0; size], &ack).is_err());
        let mut wrong_length = proof.clone();
        wrong_length.resize(size, 0);
        assert!(auth::answer(&first_host, &key, &nonce, &wrong_length).is_err());
        wrong_length = ack.clone();
        wrong_length.resize(size, 0);
        assert!(auth::verify_host(&first.connection, &key, &nonce, &host_nonce, &wrong_length).is_err());
    }
    assert!(auth::answer(&first_host, &key, &nonce, &ack).is_err());
    assert!(auth::verify_host(&first.connection, &key, &nonce, &host_nonce, &proof).is_err());
    let wrong = auth::encode_pin(&[0; 32]);
    let (result, _) = tokio::join!(connect_peer(addr, "sunna", Some(&wrong)), host.accept());
    assert!(result.is_err());
}

#[test]
fn generated_daemon_key_is_private_and_survives_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("host.key");
    let key = sunna_transport::load_or_create_key(&path).unwrap();
    assert!(key.len() == 32 && key.bytes().all(|b| b.is_ascii_hexdigit()));
    assert!(sunna_transport::load_or_create_key(&path).unwrap() == key);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(sunna_transport::load_or_create_key(&path).is_err());
    }
}

/// Something sits between the viewer and the host: it accepts the viewer's
/// connection, opens its own to the host, and passes the sign-in messages
/// along unchanged. Signing in must not work through it, and nothing it
/// handles may contain the key.
#[tokio::test]
async fn sign_in_does_not_pass_through_a_relay() {
    use sunna_proto::messages::{encode, ControlMessage, Proof};
    use sunna_transport::ControlChannel;

    let key = "5a".repeat(16);
    let host = Server::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let host_pin = auth::certificate_pin(&host.cert).unwrap();
    let relay = Server::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let relay_addr = relay.local_addr().unwrap();

    // A viewer that knows the host's identity stops in the handshake: the
    // relay can't present the host's certificate.
    let (pinned, _) = tokio::join!(
        connect_peer(relay_addr, "sunna", Some(&host_pin)),
        relay.accept()
    );
    assert!(pinned.is_err(), "a pinned viewer connected to the relay");

    // A viewer that doesn't know it yet reaches the relay, which has its own
    // connection to the real host.
    let (viewer, relay_in) = tokio::join!(connect_peer(relay_addr, "sunna", None), relay.accept());
    let (viewer, relay_in) = (viewer.unwrap(), relay_in.unwrap().unwrap());
    let (relay_out, host_in) = tokio::join!(
        connect_peer(host.local_addr().unwrap(), "sunna", None),
        host.accept()
    );
    let (relay_out, host_in) = (relay_out.unwrap(), host_in.unwrap().unwrap());

    // Viewer → relay.
    let (nonce, proof) = auth::client_proof(&viewer.connection, &key).unwrap();
    let mut from_viewer = ControlChannel::open(&viewer.connection).await.unwrap();
    from_viewer
        .send(&ControlMessage::AuthProof {
            nonce: nonce.to_vec(),
            proof: Proof(proof),
        })
        .await
        .unwrap();
    let mut at_relay = ControlChannel::accept(&relay_in).await.unwrap();
    let seen = at_relay.recv().await.unwrap();

    // What the relay holds doesn't contain the key, as text or as bytes.
    let wire = encode(&seen).unwrap();
    let raw: Vec<u8> = (0..key.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&key[i..i + 2], 16).unwrap())
        .collect();
    assert!(!wire.windows(key.len()).any(|w| w == key.as_bytes()));
    assert!(!wire.windows(raw.len()).any(|w| w == raw.as_slice()));

    // Relay → host, unchanged.
    let mut to_host = ControlChannel::open(&relay_out.connection).await.unwrap();
    to_host.send(&seen).await.unwrap();
    let mut at_host = ControlChannel::accept(&host_in).await.unwrap();
    let ControlMessage::AuthProof {
        nonce: got_nonce,
        proof: got_proof,
    } = at_host.recv().await.unwrap()
    else {
        panic!("the host received something other than the proof");
    };
    assert_eq!(got_nonce, nonce.to_vec());

    // The host refuses it: the proof was made for the viewer's connection to
    // the relay, not for the relay's connection to the host.
    assert!(
        auth::answer(&host_in, &key, &got_nonce, &got_proof.0).is_err(),
        "the host accepted a proof that was passed along by a relay"
    );
    // The same proof is fine on the connection it was made for, so the
    // refusal above is the binding at work and not a broken fixture.
    assert!(auth::answer(&relay_in, &key, &got_nonce, &got_proof.0).is_ok());

    // And the relay can't answer for the host: without the key, nothing it
    // sends back satisfies the viewer.
    assert!(auth::verify_host(&viewer.connection, &key, &nonce, &[7; 32], &[0; 32]).is_err());
    let (other_nonce, other_ack) = {
        // The best it could do is replay a real answer from another connection.
        let (n, p) = auth::client_proof(&relay_out.connection, &key).unwrap();
        auth::answer(&host_in, &key, &n, &p).unwrap()
    };
    assert!(
        auth::verify_host(&viewer.connection, &key, &nonce, &other_nonce, &other_ack).is_err(),
        "the viewer accepted the host's answer from a different connection"
    );
}

#[test]
fn public_key_fingerprint_survives_certificate_reissue() {
    // Independent expected digest: openssl x509 -pubkey | openssl pkey
    // -pubin -outform DER | openssl dgst -sha256 -binary, then base64url.
    let known = rustls::pki_types::CertificateDer::from(&include_bytes!("fixtures/public.der")[..]);
    assert_eq!(auth::certificate_pin(&known).unwrap(), "874mUdd69DdDk7TMCipQ_TDri5bSPm5sEmY9ZkkXn_4");
    let key = rcgen::KeyPair::generate().unwrap();
    let issue = |name: &str, key: &rcgen::KeyPair| {
        rcgen::CertificateParams::new(vec![name.to_string()]).unwrap().self_signed(key).unwrap()
    };
    let first = issue("original", &key);
    let renewed = issue("renewed", &key);
    assert_ne!(first.der(), renewed.der());
    assert_eq!(auth::certificate_pin(first.der()).unwrap(), auth::certificate_pin(renewed.der()).unwrap());
    let changed = issue("renewed", &rcgen::KeyPair::generate().unwrap());
    assert_ne!(auth::certificate_pin(first.der()).unwrap(), auth::certificate_pin(changed.der()).unwrap());
}

#[cfg(unix)]
#[test]
fn concurrent_first_launch_and_private_file_checks() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("host.identity");
    let barrier = std::sync::Barrier::new(8);
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..8).map(|_| scope.spawn(|| {
            barrier.wait();
            HostIdentity::load_or_create(&path).unwrap().pin().unwrap()
        })).collect();
        let pins: Vec<_> = workers.into_iter().map(|t| t.join().unwrap()).collect();
        assert!(pins.windows(2).all(|pair| pair[0] == pair[1]));
    });
    let link = dir.path().join("alias");
    symlink(&path, &link).unwrap();
    assert!(HostIdentity::load_or_create(&link).is_err());
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
    assert!(HostIdentity::load_or_create(&path).is_err());
}
