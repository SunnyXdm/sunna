use super::*;
use ring::signature::KeyPair;
fn fixture() -> (tempfile::TempDir, Updater) {
    let dir = tempfile::tempdir_in(std::env::var_os("HOME").unwrap()).unwrap();
    let u = Updater {
        root: dir.path().join("host"),
        component: "host".into(),
    };
    fs::create_dir_all(u.root.join("versions/1.0.0")).unwrap();
    fs::create_dir_all(u.root.join("versions/1.1.0")).unwrap();
    for v in ["1.0.0", "1.1.0"] {
        let bin = u.binary(&u.root.join("versions").join(v));
        fs::create_dir_all(bin.parent().unwrap()).unwrap();
        fs::write(&bin, format!("#!/bin/sh\necho sunnad {v}\n")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(bin, fs::Permissions::from_mode(0o700)).unwrap();
        }
    }
    (dir, u)
}
#[test]
fn signature_covers_exact_bytes_and_the_pinned_key() {
    let rng = ring::rand::SystemRandom::new();
    let pkcs8 = signature::Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
    let key = signature::Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
    let bytes = br#"{"version":"1.0.0","date":"today","notes":"","minimum_protocol":8,"files":[]}"#;
    let sig = key.sign(bytes);
    assert!(verify_with_key(bytes, sig.as_ref(), key.public_key().as_ref()).is_ok());
    assert!(verify_manifest(bytes, sig.as_ref()).is_err());
    let mut changed = bytes.to_vec();
    changed.push(b' ');
    assert!(verify_with_key(&changed, sig.as_ref(), key.public_key().as_ref()).is_err());
}
#[test]
fn crash_journal_recovery_and_two_failed_starts_roll_back() {
    let (_dir, u) = fixture();
    let a = State {
        current: Some("1.0.0".into()),
        pending: Some("1.1.0".into()),
        healthy: true,
        ..Default::default()
    };
    u.save(&a).unwrap();
    u.flip("1.0.0").unwrap();
    u.activate().unwrap();
    assert!(!u.starting().unwrap());
    assert!(!u.starting().unwrap());
    assert!(u.starting().unwrap());
    let state = u.state().unwrap();
    assert_eq!(state.current.as_deref(), Some("1.0.0"));
    assert_eq!(state.failed, ["1.1.0"]);
    assert_eq!(
        fs::read_link(u.root.join("current")).unwrap(),
        Path::new("versions/1.0.0")
    );
    let b = State {
        current: Some("1.1.0".into()),
        previous: Some("1.0.0".into()),
        ..Default::default()
    };
    fs::write(
        u.root.join("transaction.json"),
        serde_json::to_vec(&b).unwrap(),
    )
    .unwrap();
    u.recover().unwrap();
    assert_eq!(u.state().unwrap().current, b.current);
    assert!(!u.root.join("transaction.json").exists());
}
#[test]
fn a_session_and_an_update_cannot_own_the_switch_at_the_same_time() {
    let (_dir, u) = fixture();
    let session = u.lock("session.lock", true).unwrap();
    assert!(u.lock("session.lock", false).is_err());
    drop(session);
    let update = u.lock("session.lock", false).unwrap();
    assert!(u.lock("session.lock", true).is_err());
    drop(update);
    assert!(u.lock("session.lock", true).is_ok());
}
#[test]
fn archives_cannot_make_links_outside_the_version() {
    let (dir, u) = fixture();
    let file = dir.path().join("evil.tar.gz");
    let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(
        File::create(&file).unwrap(),
        flate2::Compression::default(),
    ));
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Symlink);
    header.set_size(0);
    header.set_mode(0o777);
    tar.append_link(&mut header, "outside", "/tmp").unwrap();
    tar.into_inner().unwrap().finish().unwrap();
    assert!(unpack(
        File::open(&file).unwrap(),
        &file,
        &u.root.join("versions/1.1.0")
    )
    .is_err());
}
fn http(body: Vec<u8>, length: usize, range: bool) -> (String, std::thread::JoinHandle<()>) {
    let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/artifact", socket.local_addr().unwrap());
    let thread = std::thread::spawn(move || {
        let (mut stream, _) = socket.accept().unwrap();
        let mut request = [0; 4096];
        let n = stream.read(&mut request).unwrap();
        let request = String::from_utf8_lossy(&request[..n]);
        if range {
            assert!(request.to_ascii_lowercase().contains("range: bytes=3-"));
        }
        let status = if range {
            "206 Partial Content\r\nContent-Range: bytes 3-5/6"
        } else {
            "200 OK"
        };
        write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        stream.write_all(&body).unwrap();
    });
    (url, thread)
}
#[test]
fn truncated_and_bad_hash_downloads_fail_and_a_partial_download_resumes() {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("archive");
    let expected = dir.path().join("expected");
    fs::write(&expected, b"abcdef").unwrap();
    let (url, thread) = http(b"abc".to_vec(), 6, false);
    let mut file = Artifact {
        component: "host".into(),
        target: "test".into(),
        url,
        size: 6,
        sha256: sha256(&expected).unwrap(),
    };
    assert!(download(&file, &dest).is_err());
    thread.join().unwrap();
    assert!(!dest.exists());
    let (url, thread) = http(b"def".to_vec(), 3, true);
    file.url = url;
    download(&file, &dest).unwrap();
    thread.join().unwrap();
    assert_eq!(fs::read(&dest).unwrap(), b"abcdef");
    fs::remove_file(&dest).unwrap();
    let (url, thread) = http(b"ghijkl".to_vec(), 6, false);
    file.url = url;
    assert!(download(&file, &dest)
        .unwrap_err()
        .to_string()
        .contains("SHA-256"));
    thread.join().unwrap();
    assert!(!dest.exists());
    assert!(!dest.with_extension("part").exists());
}

#[test]
fn failed_state_commit_keeps_running_pointer() {
    let (_dir, u) = fixture();
    u.flip("1.0.0").unwrap();
    u.save(&State {
        current: Some("1.0.0".into()),
        pending: Some("1.1.0".into()),
        ..Default::default()
    })
    .unwrap();
    // Reject state replacement while the journal and current can still be replaced.
    fs::create_dir(u.root.join("state.new")).unwrap();
    assert!(u.activate().is_err());
    assert_eq!(
        fs::read_link(u.root.join("current")).unwrap(),
        Path::new("versions/1.0.0")
    );
}

#[test]
fn rollback_request_is_consumed_with_result() {
    let (_dir, u) = fixture();
    u.save(&State {
        current: Some("1.1.0".into()),
        previous: Some("1.0.0".into()),
        ..Default::default()
    })
    .unwrap();
    u.request_rollback().unwrap();
    u.rollback().unwrap();
    assert!(!u.root.join("rollback-requested").exists());
}

#[cfg(unix)]
#[test]
fn shared_install_root_refused_before_lock_creation() {
    use std::os::unix::fs::PermissionsExt;
    let (_dir, u) = fixture();
    fs::set_permissions(&u.root, fs::Permissions::from_mode(0o777)).unwrap();
    assert!(u.lock("update.lock", false).is_err());
}
#[test]
fn public_http_metadata_refused_before_network_access() {
    let error = limited("http://192.0.2.1:0/feed", 10).unwrap_err();
    assert!(error.to_string().contains("HTTPS"));
}
#[test]
fn future_timestamp_is_due() {
    assert!(check_due(now() + 100));
}

#[cfg(unix)]
#[test]
fn partial_file_must_be_regular() {
    let (dir, _) = fixture();
    let archive = dir.path().join("artifact.zip");
    let other = dir.path().join("untouched");
    fs::write(&other, b"original").unwrap();
    std::os::unix::fs::symlink(&other, archive.with_extension("part")).unwrap();
    let artifact = Artifact {
        component: "host".into(),
        target: target(),
        url: "http://127.0.0.1:0/test".into(),
        size: 8,
        sha256: String::new(),
    };
    let error = download(&artifact, &archive).unwrap_err();
    assert_eq!(
        error
            .downcast_ref::<std::io::Error>()
            .unwrap()
            .raw_os_error(),
        Some(libc::ELOOP)
    );
    assert_eq!(fs::read(other).unwrap(), b"original");
}

#[test]
fn redirect_policy_is_applied_before_following_location() {
    let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/manifest", socket.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut stream, _) = socket.accept().unwrap();
        let mut buf = [0; 1024];
        stream.read(&mut buf).unwrap();
        stream.write_all(b"HTTP/1.1 302 Found\r\nLocation: http://192.0.2.1/release\r\nContent-Length: 0\r\n\r\n").unwrap();
    });
    assert!(limited(&url, MAX_MANIFEST)
        .unwrap_err()
        .to_string()
        .contains("HTTPS"));
    server.join().unwrap();
}

#[test]
fn rollback_preserves_high_water_mark() {
    let (_dir, u) = fixture();
    let latest = "1.3.0";
    u.save(&State {
        current: Some(latest.into()),
        previous: Some("1.0.0".into()),
        ..Default::default()
    })
    .unwrap();
    atomic(&u.root.join("highest"), latest.as_bytes()).unwrap();
    u.rollback().unwrap();
    assert!(version("1.2.0").unwrap() < u.floor(&u.state().unwrap()).unwrap());
}
#[test]
fn archive_entry_limit_includes_empty_directories() {
    let (dir, _) = fixture();
    let archive = dir.path().join("dirs.tar.gz");
    let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(
        File::create(&archive).unwrap(),
        flate2::Compression::fast(),
    ));
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Directory);
    header.set_size(0);
    header.set_mode(0o700);
    for _ in 0..16385 {
        tar.append_data(&mut header, ".", std::io::empty()).unwrap();
    }
    tar.into_inner().unwrap().finish().unwrap();
    let out = dir.path().join("stage");
    fs::create_dir(&out).unwrap();
    assert!(unpack(File::open(&archive).unwrap(), &archive, &out)
        .unwrap_err()
        .to_string()
        .contains("too many"));
}

#[test]
fn remote_request_checks_peer_floor_and_stale_results() {
    let (_dir, u) = fixture();
    u.save(&State {
        current: Some("1.0.0".into()),
        minimum_protocol: 9,
        ..Default::default()
    })
    .unwrap();
    assert!(u
        .request_from(8)
        .unwrap_err()
        .to_string()
        .contains("update the app first"));
    u.request_from(9).unwrap();
    let id = u.state().unwrap().request_id;
    u.request_from(10).unwrap();
    u.report_result(id, Some("old error".into())).unwrap();
    assert_eq!(u.state().unwrap().peer_protocol, Some(10));
}

#[test]
fn corrupt_state_after_explicit_rollback_preserves_operator_selection() {
    let (_dir, u) = fixture();
    u.save(&State {
        current: Some("1.1.0".into()),
        previous: Some("1.0.0".into()),
        healthy: true,
        ..State::default()
    })
    .unwrap();
    u.flip("1.1.0").unwrap();
    atomic(&u.root.join("known-good"), b"1.1.0").unwrap();
    u.rollback().unwrap();
    fs::write(u.root.join("state.json"), b"{").unwrap();
    u.recover().unwrap();
    assert_eq!(
        u.floor(&u.state().unwrap()).unwrap(),
        version("1.1.0").unwrap()
    );
    assert_eq!(u.state().unwrap().current.as_deref(), Some("1.0.0"));
}
