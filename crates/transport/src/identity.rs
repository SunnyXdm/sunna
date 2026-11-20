//! Store the certificate and PKCS#8 together so a restart or concurrent
//! first launch always sees a complete pair.
use super::{auth, Result, TransportError};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use std::{
    fs,
    io::{self, Read, Write},
    path::Path,
};

fn read_private(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let mut opts = fs::OpenOptions::new();
    opts.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let f = opts.open(path)?;
    let meta = f.metadata()?;
    if !meta.is_file() || meta.len() > limit {
        return Err(io::Error::other("Invalid private host settings file.").into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.mode() & 0o077 != 0 || meta.uid() != unsafe { libc::geteuid() } {
            return Err(
                io::Error::other("Host credentials must be private to their owner.").into(),
            );
        }
    }
    let mut bytes = Vec::new();
    f.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(io::Error::other("Private host settings file is too large.").into());
    }
    Ok(bytes)
}

fn write_private(path: &Path, bytes: &[u8], replace: bool) -> Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| io::Error::other("Missing identity directory."))?;
    fs::create_dir_all(dir)?;
    let mut file = tempfile::NamedTempFile::new_in(dir)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    if replace {
        file.persist(path).map_err(|e| e.error)?;
    } else if let Err(e) = file.persist_noclobber(path) {
        if e.error.kind() == io::ErrorKind::AlreadyExists {
            return Ok(());
        }
        return Err(e.error.into());
    }
    #[cfg(unix)]
    fs::File::open(dir)?.sync_all()?;
    Ok(())
}

/// A daemon started without configured credentials keeps its generated key
/// in the same private directory as the host identity.
pub fn load_or_create_key(path: &Path) -> Result<String> {
    let read = || -> Result<String> {
        let raw = read_private(path, 64)?;
        if !matches!(raw.len(), 32 | 64) || !raw.iter().all(u8::is_ascii_hexdigit) {
            return Err(io::Error::other("Invalid saved sharing key.").into());
        }
        Ok(String::from_utf8(raw).expect("hexadecimal ASCII"))
    };
    match read() {
        Err(TransportError::Io(e)) if e.kind() == io::ErrorKind::NotFound => {
            use ring::rand::{SecureRandom, SystemRandom};
            let mut bytes = [0; 16];
            SystemRandom::new()
                .fill(&mut bytes)
                .map_err(|_| io::Error::other("No secure random numbers available."))?;
            let key: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
            write_private(path, key.as_bytes(), false)?;
            read()
        }
        result => result,
    }
}

pub struct HostIdentity {
    pub(super) cert: CertificateDer<'static>,
    pub(super) key: PrivatePkcs8KeyDer<'static>,
}

impl HostIdentity {
    pub(super) fn generate() -> Result<Self> {
        let pair = rcgen::generate_simple_self_signed(vec!["sunna".into()])?;
        Ok(Self {
            cert: pair.cert.der().clone(),
            key: pair.key_pair.serialize_der().into(),
        })
    }

    pub fn pin(&self) -> Result<String> {
        auth::certificate_pin(&self.cert)
    }

    /// Read an existing private identity and check the certificate/key pair.
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = read_private(path, 16384)?;
        let (cert, key): (&[u8], &[u8]) = postcard::from_bytes(&bytes)?;
        let cert = CertificateDer::from(cert.to_vec());
        let key = PrivatePkcs8KeyDer::from(key.to_vec());
        rustls::sign::CertifiedKey::from_der(
            vec![cert.clone()],
            key.clone_key().into(),
            &super::crypto_provider(),
        )?
        .keys_match()?;
        Ok(Self { cert, key })
    }

    fn write(path: &Path, replace: bool) -> Result<Self> {
        let id = Self::generate()?;
        let bytes = postcard::to_stdvec(&(id.cert.as_ref(), id.key.secret_pkcs8_der()))?;
        write_private(path, &bytes, replace)?;
        Self::load(path)
    }

    pub fn load_or_create(path: &Path) -> Result<Self> {
        match Self::load(path) {
            Err(TransportError::Io(e)) if e.kind() == io::ErrorKind::NotFound => {
                Self::write(path, false)
            }
            result => result,
        }
    }

    pub fn rotate(path: &Path) -> Result<Self> {
        Self::write(path, true)
    }
}
