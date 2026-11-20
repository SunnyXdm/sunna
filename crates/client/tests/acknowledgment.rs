// An invalid host response cannot commit a pin or release the queued probe.
use sunna_client::{auth::Trust, reach};
use sunna_proto::messages::{ControlMessage, Proof};
use sunna_transport::{auth, ControlChannel, Server};

#[tokio::test]
async fn invalid_ack_leaves_the_record_unpinned_and_sends_no_operation() {
    let dir = tempfile::tempdir().unwrap();
    sunna_client::set_state_dir(dir.path().to_owned());
    let server = Server::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let addr = server.local_addr().unwrap();
    let key = "12".repeat(16);
    let (host, viewer) = tokio::join!(
        async {
            for variant in 0..5 {
                let conn = server.accept().await.unwrap().unwrap();
                let mut control = ControlChannel::accept(&conn).await.unwrap();
                let ControlMessage::AuthProof { nonce, proof } = control.recv().await.unwrap()
                else {
                    panic!("expected proof")
                };
                let (host, mut tag) = auth::answer(&conn, &key, &nonce, &proof.0).unwrap();
                let mut host = host.to_vec();
                match variant {
                    0 => {
                        host.pop();
                    }
                    1 => host.push(0),
                    2 => {
                        tag.pop();
                    }
                    3 => tag.push(0),
                    _ => tag = proof.0,
                }
                control
                    .send(&ControlMessage::AuthProofAck {
                        nonce: host,
                        proof: Proof(tag),
                    })
                    .await
                    .unwrap();
                assert!(
                    tokio::time::timeout(std::time::Duration::from_secs(5), control.recv())
                        .await
                        .unwrap()
                        .is_err()
                );
            }
        },
        async {
            let record = Trust {
                record: Some("fixture".into()),
                ..Default::default()
            };
            for _ in 0..5 {
                let result = reach::check_with(&addr.to_string(), &key, &record).await;
                assert_eq!(result.state, "identity-changed");
                assert!(record.pin().unwrap().is_none());
            }
        }
    );
    let _ = (host, viewer);
}
