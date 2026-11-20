fn main() {
    println!("cargo:rerun-if-env-changed=SUNNA_TEST_PUBLIC_KEY");
    println!("cargo:rerun-if-env-changed=SUNNA_PUBLIC_RELEASE");
    if std::env::var_os("SUNNA_PUBLIC_RELEASE").is_some() {
        assert!(std::env::var_os("CARGO_FEATURE_TEST_KEY").is_none(), "release packaging forbids test-key");
    }
    if std::env::var_os("CARGO_FEATURE_TEST_KEY").is_some() {
        let path = std::env::var("SUNNA_TEST_PUBLIC_KEY")
            .expect("test-key requires SUNNA_TEST_PUBLIC_KEY (raw public key path)");
        println!("cargo:rerun-if-changed={path}");
        let key = std::fs::read(path).expect("read test public key");
        assert_eq!(key.len(), 32);
        std::fs::write(
            std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("test.pub"),
            key,
        )
        .unwrap();
    }
}
