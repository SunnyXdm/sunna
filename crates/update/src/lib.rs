//! Signed releases and per-component atomic installation. No network input is
//! executed before both manifest signature and artifact digest are verified.
use anyhow::{bail, ensure, Context, Result};
use fs2::FileExt;
use ring::{digest, signature};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, Write},
    path::{Component, Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// Where installed copies look for releases. A name we own, so the files can
/// move (this VM, GitHub, a CDN) without stranding anything already installed.
pub const DEFAULT_URL: &str = "https://sunna.sunnydx.dev/releases/latest";
#[cfg(not(feature = "test-key"))]
#[no_mangle]
pub static SUNNA_RELEASE_VERIFY_KEY: [u8; 32] = *include_bytes!("../release.pub");
#[cfg(feature = "test-key")]
#[no_mangle]
pub static SUNNA_RELEASE_VERIFY_KEY: [u8; 32] =
    *include_bytes!(concat!(env!("OUT_DIR"), "/test.pub"));
const MAX_MANIFEST: u64 = 1024 * 1024;
const MAX_ARTIFACT: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub component: String,
    pub target: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub version: String,
    pub date: String,
    pub notes: String,
    pub minimum_protocol: u16,
    pub files: Vec<Artifact>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub format: u32,
    pub request: Option<String>,
    pub request_id: u64,
    pub pending_protocol: u16,
    pub error: Option<String>,
    pub minimum_protocol: u16,
    pub peer_protocol: Option<u16>,
    pub last_check: u64,
    pub available: Option<String>,
    pub pending: Option<String>,
    pub current: Option<String>,
    pub previous: Option<String>,
    pub failed: Vec<String>,
    pub starts: u32,
    pub healthy: bool,
    pub policy: String,
}
#[derive(Clone, Debug)]
pub struct Updater {
    pub root: PathBuf,
    pub component: String,
}
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn check_due(last: u64) -> bool {
    let time = now();
    last > time || time - last >= 86400
}

fn sync_tree(path: &Path) -> Result<()> {
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            sync_tree(&entry?.path())?;
        }
        #[cfg(not(unix))]
        return Ok(());
    }
    File::open(path)?.sync_all()?;
    Ok(())
}

pub fn target() -> String {
    format!(
        "{}-{}",
        std::env::consts::OS.replace("macos", "macos"),
        std::env::consts::ARCH
    )
}
pub fn data_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("SUNNA_UPDATE_ROOT") {
        return path.into();
    }
    #[cfg(windows)]
    return PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default()).join("Sunna");
    #[cfg(not(windows))]
    PathBuf::from(std::env::var_os("XDG_DATA_HOME").unwrap_or_else(|| {
        PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
            .join(".local/share")
            .into()
    }))
    .join("sunna")
}
pub fn base_url() -> String {
    std::env::var("SUNNA_UPDATE_URL")
        .unwrap_or_else(|_| DEFAULT_URL.into())
        .trim_end_matches('/')
        .into()
}
fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .redirects(0)
        .timeout(Duration::from_secs(120))
        .build()
}
fn checked_url(raw: &str) -> Result<url::Url> {
    let url = url::Url::parse(raw)?;
    ensure!(
        url.scheme() == "https"
            || (cfg!(any(test, feature = "test-key"))
                && url.scheme() == "http"
                && url.host().is_some_and(|h| match h {
                    url::Host::Ipv4(ip) => ip.is_loopback(),
                    url::Host::Ipv6(ip) => ip.is_loopback(),
                    _ => false,
                })),
        "release URL must use HTTPS"
    );
    Ok(url)
}
fn get(raw: &str, offset: u64) -> Result<ureq::Response> {
    let agent = agent();
    let mut url = checked_url(raw)?;
    for _ in 0..6 {
        let mut req = agent.get(url.as_str());
        if offset > 0 {
            req = req.set("Range", &format!("bytes={offset}-"));
        }
        let response = req.call()?;
        if !matches!(response.status(), 301 | 302 | 303 | 307 | 308) {
            return Ok(response);
        }
        let next = url.join(
            response
                .header("Location")
                .context("redirect without location")?,
        )?;
        url = checked_url(next.as_str())?;
    }
    bail!("too many redirects")
}
fn limited(url: &str, max: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    get(url, 0)?
        .into_reader()
        .take(max + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= max, "release metadata too large");
    Ok(bytes)
}
/// Used by native packaging jobs on the built executable, before signing.
pub fn publication_probe() -> Option<Result<()>> {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new("--verify-release-fixture")) {
        return None;
    }
    Some((|| {
        let key = fs::read(args.next().context("expected public key")?)?;
        ensure!(
            key == SUNNA_RELEASE_VERIFY_KEY,
            "unexpected embedded release key"
        );
        let bytes = fs::read(args.next().context("fixture manifest")?)?;
        let sig = fs::read(args.next().context("fixture signature")?)?;
        verify_manifest(&bytes, &sig)?;
        Ok(())
    })())
}

pub fn verify_manifest(bytes: &[u8], sig: &[u8]) -> Result<Manifest> {
    verify_with_key(bytes, sig, &SUNNA_RELEASE_VERIFY_KEY)
}
fn verify_with_key(bytes: &[u8], sig: &[u8], key: &[u8]) -> Result<Manifest> {
    ensure!(bytes.len() as u64 <= MAX_MANIFEST, "manifest too large");
    signature::UnparsedPublicKey::new(&signature::ED25519, key)
        .verify(bytes, sig)
        .map_err(|_| anyhow::anyhow!("release signature is invalid"))?;
    let manifest: Manifest = serde_json::from_slice(bytes)?;
    version(&manifest.version)?;
    for f in &manifest.files {
        ensure!(
            f.size > 0 && f.size <= MAX_ARTIFACT,
            "invalid artifact size"
        );
        ensure!(
            f.sha256.len() == 64 && f.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid SHA-256"
        );
        checked_url(&f.url)?;
    }
    Ok(manifest)
}
pub fn fetch_manifest() -> Result<Manifest> {
    let base = base_url();
    verify_manifest(
        &limited(&format!("{base}/manifest.json"), MAX_MANIFEST)?,
        &limited(&format!("{base}/manifest.json.sig"), 64)?,
    )
}
fn version(value: &str) -> Result<semver::Version> {
    ensure!(
        !value.contains('/') && !value.contains('\\') && value.len() < 100,
        "unsafe version"
    );
    Ok(semver::Version::parse(value)?)
}
pub fn sha256(path: &Path) -> Result<String> {
    hash_reader(&mut File::open(path)?)
}
fn hash_reader(file: &mut File) -> Result<String> {
    file.rewind()?;
    let mut ctx = digest::Context::new(&digest::SHA256);
    let mut buf = [0; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        ctx.update(&buf[..n]);
    }
    Ok(ctx
        .finish()
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}
/// Retain an incomplete .part for Range resume. Every byte is rehashed before use.
fn safe_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x00200000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .mode(0o600);
    }
    options
}
pub fn download(file: &Artifact, dest: &Path) -> Result<File> {
    match safe_options().read(true).open(dest) {
        Ok(mut cached) if cached.metadata()?.is_file() && cached.metadata()?.len() == file.size => {
            if hash_reader(&mut cached)? == file.sha256 {
                cached.rewind()?;
                return Ok(cached);
            }
        }
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
        _ => (),
    }
    let part = dest.with_extension("part");
    let mut out = match safe_options()
        .create_new(true)
        .read(true)
        .write(true)
        .open(&part)
    {
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            safe_options().read(true).write(true).open(&part)?
        }
        other => other?,
    };
    ensure!(out.metadata()?.is_file(), "invalid partial download");
    let size = out.metadata()?.len();
    let offset = if size < file.size { size } else { 0 };
    let response = get(&file.url, offset)?;
    let append = offset > 0 && response.status() == 206;
    if append {
        ensure!(
            response
                .header("Content-Range")
                .is_some_and(|v| v.starts_with(&format!("bytes {offset}-"))
                    && v.ends_with(&format!("/{}", file.size))),
            "invalid resume range"
        );
    } else {
        ensure!(response.status() == 200, "unexpected download status");
    }
    if append {
        out.seek(std::io::SeekFrom::End(0))?;
    } else {
        out.set_len(0)?;
    }
    let remaining = file.size - if append { offset } else { 0 };
    let copied = std::io::copy(&mut response.into_reader().take(remaining + 1), &mut out)?;
    out.sync_all()?;
    ensure!(
        copied == remaining,
        "truncated or oversized release download"
    );
    if hash_reader(&mut out)? != file.sha256 {
        fs::remove_file(part)?;
        bail!("release SHA-256 mismatch");
    }
    replace(&part, dest)?;
    out.rewind()?;
    Ok(out)
}
fn replace(from: &Path, to: &Path) -> Result<()> {
    #[cfg(not(windows))]
    fs::rename(from, to)?;
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn MoveFileExW(a: *const u16, b: *const u16, flags: u32) -> i32;
        }
        let a: Vec<_> = from.as_os_str().encode_wide().chain(Some(0)).collect();
        let b: Vec<_> = to.as_os_str().encode_wide().chain(Some(0)).collect();
        ensure!(
            unsafe { MoveFileExW(a.as_ptr(), b.as_ptr(), 1 | 8) } != 0,
            "atomic rename: {}",
            std::io::Error::last_os_error()
        );
    }
    #[cfg(unix)]
    File::open(to.parent().context("parent")?)?.sync_all()?;
    Ok(())
}
fn atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("new");
    // A leftover temporary file is never opened. Writers serialize on update.lock.
    match fs::remove_file(&tmp) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
        _ => (),
    }
    let mut file = safe_options().create_new(true).write(true).open(&tmp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    replace(&tmp, path)
}
#[cfg(not(windows))]
fn link(target: &Path, path: &Path) -> Result<()> {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, path)?;

    Ok(())
}
fn unpack(file: File, archive: &Path, dest: &Path) -> Result<()> {
    let mut total = 0u64;
    if archive.extension().is_some_and(|e| e == "zip") {
        let mut zip = zip::ZipArchive::new(file)?;
        ensure!(zip.len() <= 16384, "too many archive entries");
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)?;
            let path = entry.enclosed_name().context("unsafe zip path")?;
            ensure!(!entry.is_symlink(), "archive links are not allowed");
            let out = dest.join(path);
            if entry.is_dir() {
                fs::create_dir_all(out)?;
            } else {
                fs::create_dir_all(out.parent().unwrap())?;
                let budget = MAX_ARTIFACT * 4 - total;
                let count = std::io::copy(
                    &mut (&mut entry).take(budget + 1),
                    &mut safe_options().create_new(true).write(true).open(&out)?,
                )?;
                ensure!(
                    count <= budget && count == entry.size(),
                    "invalid zip output size"
                );
                total += count;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(
                        out,
                        fs::Permissions::from_mode(entry.unix_mode().unwrap_or(0o644) & 0o755),
                    )?;
                }
            }
        }
    } else {
        let mut tar =
            tar::Archive::new(flate2::read::GzDecoder::new(file).take(MAX_ARTIFACT * 4 + 1));
        tar.set_mask(0o077);
        for (i, entry) in tar.entries()?.raw(true).enumerate() {
            ensure!(i < 16384, "too many archive entries");
            let mut entry = entry?;
            let path = entry.path()?.into_owned();
            ensure!(
                path.components()
                    .all(|c| matches!(c, Component::Normal(_) | Component::CurDir)),
                "unsafe archive path"
            );
            ensure!(
                entry.header().entry_type().is_file() || entry.header().entry_type().is_dir(),
                "archive links and special files are not allowed"
            );
            total = total
                .checked_add(entry.size())
                .context("archive overflow")?;
            ensure!(total <= MAX_ARTIFACT * 4, "archive too large");
            ensure!(entry.unpack_in(dest)?, "unsafe archive path");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = if entry.header().entry_type().is_dir() {
                    0o700
                } else {
                    entry.header().mode()? & 0o755
                };
                fs::set_permissions(dest.join(path), fs::Permissions::from_mode(mode))?;
            }
        }
    }
    Ok(())
}
fn binary_version(binary: &Path) -> Result<String> {
    use std::process::Stdio;
    let mut child = Command::new(binary).arg("--version").stdout(Stdio::piped()).stderr(Stdio::null()).spawn()
        .context("New binary cannot start. Linux apps need WebKitGTK 4.1 (libwebkit2gtk-4.1-0 on Debian/Ubuntu, webkit2gtk4.1 on Fedora, webkit2gtk-4.1 on Arch).")?;
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!("new binary timed out during pre-flight");
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    ensure!(
        status.success(),
        "New binary failed pre-flight. Check WebKitGTK 4.1 is installed for the Linux app."
    );
    let mut output = String::new();
    child
        .stdout
        .take()
        .context("version output")?
        .take(8192)
        .read_to_string(&mut output)?;
    Ok(output)
}
fn preflight(binary: &Path, expected: &str) -> Result<()> {
    let output = binary_version(binary)?;
    ensure!(
        output.split_whitespace().any(|v| v == expected),
        "binary version differs from manifest"
    );
    Ok(())
}
#[cfg(windows)]
fn check_acl(path: &Path, tree: bool) -> Result<()> {
    ensure!(path.is_absolute(), "update path must be absolute");
    let shell = PathBuf::from(std::env::var_os("SystemRoot").context("missing SystemRoot")?)
        .join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let status = Command::new(shell)
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            include_str!("check-root.ps1"),
        ])
        .env("SUNNA_ACL_CHECK", path)
        .env("SUNNA_ACL_TREE", if tree { "1" } else { "0" })
        .status()?;
    ensure!(status.success(), "unsafe update path {}", path.display());
    Ok(())
}
fn check_root(path: &Path) -> Result<()> {
    #[cfg(windows)]
    check_acl(path, false)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(
            path.is_absolute() && !path.components().any(|c| matches!(c, Component::ParentDir)),
            "unsafe update root"
        );
        for parent in path.ancestors().collect::<Vec<_>>().into_iter().rev() {
            match fs::symlink_metadata(parent) {
                Ok(m) => ensure!(
                    m.is_dir()
                        && m.mode() & 0o022 == 0
                        && (m.uid() == 0 || m.uid() == unsafe { libc::geteuid() }),
                    "unsafe update directory {}",
                    parent.display()
                ),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => break,
                Err(e) => return Err(e.into()),
            }
        }
    }
    Ok(())
}
impl Updater {
    pub fn new(component: &str) -> Result<Self> {
        ensure!(matches!(component, "host" | "app"), "unknown component");
        let root = data_dir().join(component);
        check_root(&root)?;
        Ok(Self {
            root,
            component: component.into(),
        })
    }
    fn lock_file(&self, name: &str) -> Result<File> {
        check_root(&self.root)?;
        let mut dirs = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            dirs.mode(0o700);
        }
        dirs.recursive(true).create(&self.root)?;
        let file = safe_options()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.root.join(name))
            .or_else(|e| {
                if (name == "daemon.lock" || cfg!(target_os = "macos") && name == "session.lock")
                    && matches!(
                        e.kind(),
                        std::io::ErrorKind::ReadOnlyFilesystem
                            | std::io::ErrorKind::PermissionDenied
                    )
                {
                    safe_options().read(true).open(self.root.join(name))
                } else {
                    Err(e)
                }
            })?;
        Ok(file)
    }
    #[cfg(target_os = "macos")]
    pub fn wait_for_mac_host_idle(&self) -> Result<File> {
        let file = self.lock_file("session.lock")?;
        FileExt::lock_exclusive(&file)?;
        Ok(file)
    }
    pub fn lock(&self, name: &str, shared: bool) -> Result<File> {
        let file = self.lock_file(name)?;
        if shared {
            FileExt::try_lock_shared(&file)?;
        } else {
            file.try_lock_exclusive()?;
        }
        Ok(file)
    }
    #[cfg(target_os = "linux")]
    pub fn waiting_for_desktop(&self, sequence: u64) -> Result<()> {
        if let Some(path) = std::env::var_os("SUNNA_READY_FILE") {
            ensure!(self.is_managed_process(), "selection changed during probation");
            // Ephemeral, per-child liveness in the supervisor's private directory.
            fs::write(path, format!("{} {} waiting {}", std::process::id(), std::env::var("SUNNA_SELECTED")?, sequence))?;
        }
        Ok(())
    }
    pub fn signal_ready(&self) -> Result<()> {
        if let Some(path) = std::env::var_os("SUNNA_READY_FILE") {
            ensure!(
                self.is_managed_process(),
                "selection changed before readiness"
            );
            atomic(
                Path::new(&path),
                format!(
                    "{} {}",
                    std::process::id(),
                    std::env::var("SUNNA_SELECTED")?
                )
                .as_bytes(),
            )?;
        }
        Ok(())
    }
    pub fn is_managed_process(&self) -> bool {
        if std::env::var_os("SUNNA_RECOVERY_ONLY").is_some() {
            return false;
        }
        match (
            self.binary(&self.root.join("current")).canonicalize(),
            std::env::current_exe(),
        ) {
            #[cfg(target_os = "macos")]
            (Ok(installed), Ok(running)) => running.canonicalize().is_ok_and(|exe| installed == exe),
            #[cfg(not(target_os = "macos"))]
            (Ok(installed), Ok(running)) => installed == running,
            _ => false,
        }
    }
    pub fn state(&self) -> Result<State> {
        Ok(self.read_record("state.json")?.unwrap_or_default())
    }
    fn read_record(&self, name: &str) -> Result<Option<State>> {
        let path = self.root.join(name);
        let parsed = fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice::<State>(&b).ok());
        Ok(parsed.filter(|s| s.format <= 1))
    }
    fn save(&self, state: &State) -> Result<()> {
        let mut state = state.clone();
        state.format = 1;
        atomic(
            &self.root.join("state.json"),
            &serde_json::to_vec_pretty(&state)?,
        )
    }
    fn record_request(&self, action: &str, peer: Option<u16>) -> Result<()> {
        let _lock = self.lock("update.lock", false)?;
        let mut state = self.state()?;
        if action == "rollback" {
            ensure!(state.previous.is_some(), "no previous release");
        } else if let Some(peer) = peer {
            ensure!(
                peer >= state.minimum_protocol.max(state.pending_protocol),
                "update the app first"
            );
        }
        state.request_id = state
            .request_id
            .checked_add(1)
            .context("request counter exhausted")?;
        state.request = Some(action.into());
        state.peer_protocol = peer;
        state.error = None;
        self.save(&state)
    }
    pub fn request_from(&self, peer: u16) -> Result<()> {
        self.record_request("update", Some(peer))
    }
    pub fn report_result(&self, request_id: u64, error: Option<String>) -> Result<()> {
        let _lock = self.lock("update.lock", false)?;
        let mut state = self.state()?;
        if request_id != state.request_id {
            return Ok(());
        }
        if error.is_some() {
            state.available = None;
        }
        state.error = error;
        state.request = None;
        state.peer_protocol = None;
        self.save(&state)
    }
    pub fn policy(&self) -> Result<String> {
        let state = self.state()?;
        let policy = std::env::var("SUNNA_UPDATE").unwrap_or_else(|_| {
            if state.policy.is_empty() {
                "notify".into()
            } else {
                state.policy
            }
        });
        ensure!(
            matches!(policy.as_str(), "off" | "notify" | "auto"),
            "SUNNA_UPDATE must be auto, notify or off"
        );
        Ok(policy)
    }
    pub fn set_policy(&self, policy: &str) -> Result<()> {
        ensure!(
            matches!(policy, "off" | "notify" | "auto"),
            "invalid update policy"
        );
        let _lock = self.lock("update.lock", false)?;
        let mut state = self.state()?;
        state.policy = policy.into();
        self.save(&state)
    }
    pub fn request_rollback(&self) -> Result<()> {
        self.record_request("rollback", None)
    }
    pub fn request(&self) -> Result<()> {
        ensure!(
            self.root.join("current").exists(),
            "This is not a managed installation"
        );
        self.record_request("update", None)
    }
    fn floor(&self, state: &State) -> Result<semver::Version> {
        let mut floor = version(
            state
                .current
                .as_deref()
                .unwrap_or(env!("CARGO_PKG_VERSION")),
        )?;
        for v in state
            .previous
            .iter()
            .chain(state.failed.iter())
            .chain(state.pending.iter())
        {
            floor = floor.max(version(v)?);
        }
        match fs::read_to_string(self.root.join("highest")) {
            Ok(v) => Ok(floor.max(version(&v)?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(floor),
            Err(e) => Err(e.into()),
        }
    }
    pub fn due_in_process(&self, last: u64, clock: &mut Option<(u64, std::time::Instant)>) -> bool {
        if let Some((stamp, instant)) = clock {
            if *stamp == last {
                return instant.elapsed() >= Duration::from_secs(86400);
            }
        }
        check_due(last)
    }
    pub fn check(&self) -> Result<Manifest> {
        let _lock = self.lock("update.lock", false)?;
        let mut state = self.state()?;
        state.last_check = now();
        self.save(&state)?;
        let manifest = fetch_manifest()?;
        let current = state
            .current
            .as_deref()
            .unwrap_or(env!("CARGO_PKG_VERSION"));
        state.minimum_protocol = manifest.minimum_protocol;
        state.available = if version(&manifest.version)? > version(current)?
            && !version(&manifest.version)?
                .cmp_precedence(&self.floor(&state)?)
                .is_lt()
            && !state.failed.contains(&manifest.version)
        {
            Some(manifest.version.clone())
        } else {
            None
        };
        self.save(&state)?;
        Ok(manifest)
    }
    /// Validate previously installed files too, including libraries loaded by the selected executable.
    pub fn launch_binary(&self, selected: &str) -> Result<PathBuf> {
        version(selected)?;
        let tree = self.root.join("versions").join(selected);
        check_root(&tree)?;
        #[cfg(windows)]
        check_acl(&tree, true)?;
        #[cfg(unix)]
        fn check_entry(path: &Path) -> Result<()> {
            use std::os::unix::fs::MetadataExt;
            let m = fs::symlink_metadata(path)?;
            ensure!(
                (m.is_dir() || m.is_file())
                    && m.mode() & 0o022 == 0
                    && (m.uid() == 0 || m.uid() == unsafe { libc::geteuid() }),
                "unsafe installed file {}",
                path.display()
            );
            if m.is_dir() {
                for child in fs::read_dir(path)? {
                    check_entry(&child?.path())?;
                }
            }
            Ok(())
        }
        #[cfg(unix)]
        check_entry(&tree)?;
        Ok(self.binary(&tree))
    }
    pub fn binary(&self, directory: &Path) -> PathBuf {
        // Windows cannot rename a directory over an existing junction atomically.
        // Its `current` is a regular version-pointer file, replaced with
        // MoveFileExW. The stable launcher reads it before each startup.
        #[cfg(windows)]
        let resolved;
        #[cfg(windows)]
        let directory = if directory == self.root.join("current") {
            let ver = fs::read_to_string(directory).unwrap_or_default();
            if version(ver.trim()).is_err() {
                return self.root.join("invalid-current");
            }
            resolved = self.root.join("versions").join(ver.trim());
            &resolved
        } else {
            directory
        };

        #[cfg(target_os = "macos")]
        if self.component == "app" {
            return directory.join("Sunna.app/Contents/MacOS/Sunna-real");
        }
        directory.join("bin").join(if self.component == "host" {
            format!("sunnad{}", std::env::consts::EXE_SUFFIX)
        } else {
            format!("sunna-real{}", std::env::consts::EXE_SUFFIX)
        })
    }
    /// Prepare without switching. Re-fetch and verify at this trust boundary.
    pub fn prepare(&self) -> Result<State> {
        let _lock = self.lock("update.lock", false)?;
        self.recover_locked()?;
        let manifest = fetch_manifest()?;
        let mut state = self.state()?;
        if let Some(peer) = state.peer_protocol {
            ensure!(peer >= manifest.minimum_protocol, "update the app first");
        }
        state.minimum_protocol = manifest.minimum_protocol;
        ensure!(
            !state.failed.contains(&manifest.version),
            "this release failed previously; waiting for a newer release"
        );
        let offered = version(&manifest.version)?;
        let highest = self.floor(&state)?;
        if offered.cmp_precedence(&highest).is_lt() {
            bail!("release is below the accepted version floor");
        }
        if let Some(current) = &state.current {
            if offered.cmp_precedence(&version(current)?).is_le() && !current.ends_with("+legacy") {
                return Ok(state);
            }
        }
        let file = manifest
            .files
            .iter()
            .find(|f| f.component == self.component && f.target == target())
            .context("release has no download for this platform")?;
        let cache = self.root.join("downloads");
        check_root(&cache)?;
        fs::create_dir_all(&cache)?;
        let archive = cache.join(format!(
            "{}.{}",
            file.sha256,
            if file.url.ends_with(".zip") {
                "zip"
            } else {
                "tar.gz"
            }
        ));
        let verified = download(file, &archive)?;
        let versions = self.root.join("versions");
        check_root(&versions)?;
        fs::create_dir_all(&versions)?;
        let stage = versions.join(format!("{}.staging", manifest.version));
        if stage.exists() {
            fs::remove_dir_all(&stage)?;
        }
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&stage)?;
        let prepared = (|| -> Result<()> {
            unpack(verified, &archive, &stage)?;
            preflight(&self.binary(&stage), &manifest.version)?;
            #[cfg(target_os = "macos")]
            if self.component == "app" {
                ensure!(
                    Command::new(stage.join("bin/sign-mac.sh"))
                        .arg(stage.join("Sunna.app"))
                        .arg(self.root.join("current/Sunna.app"))
                        .status()?
                        .success(),
                    "local code signing failed"
                );
            }
            preflight(
                &stage
                    .join("bin")
                    .join(format!("sunna-update{}", std::env::consts::EXE_SUFFIX)),
                &manifest.version,
            )?;
            if self.component == "app" {
                let launcher = if cfg!(target_os = "macos") {
                    "Sunna.app/Contents/MacOS/Sunna"
                } else {
                    "bin/sunna"
                };
                preflight(&stage.join(launcher), &manifest.version)?;
            }
            #[cfg(target_os = "linux")]
            if self.root == Path::new("/opt/sunna/host") {
                use std::os::unix::fs::PermissionsExt;
                ensure!(unsafe { libc::geteuid() } == 0, "system update requires root");
                ensure!(stage.join("share/sunna/system-host.sh").is_file(), "system host helper missing");
                // Session workers must traverse to the installed program and its
                // signed libraries. Settings, update state and downloads stay private.
                for dir in [self.root.parent().unwrap(), &self.root, &versions, &stage,
                            &stage.join("bin"), &stage.join("lib"), &stage.join("share"), &stage.join("share/sunna")] {
                    fs::set_permissions(dir, fs::Permissions::from_mode(0o711))?;
                }
            }
            Ok(())
        })();
        if let Err(error) = prepared {
            fs::remove_dir_all(&stage)?;
            if state.current.is_some() {
                state.failed.push(manifest.version);
            }
            state.available = None;
            state.pending = None;
            self.save(&state)?;
            return Err(error);
        }
        let dest = versions.join(&manifest.version);
        if dest.exists() {
            fs::remove_dir_all(&dest)?;
        }
        sync_tree(&stage)?;
        replace(&stage, &dest)?;
        atomic(&self.root.join("highest"), manifest.version.as_bytes())?;
        state.available = Some(manifest.version.clone());
        state.pending_protocol = manifest.minimum_protocol;
        state.pending = Some(manifest.version);
        state.last_check = now();
        self.save(&state)?;
        Ok(state)
    }
    fn flip(&self, ver: &str) -> Result<()> {
        version(ver)?;
        ensure!(
            self.binary(&self.root.join("versions").join(ver)).is_file(),
            "version is missing"
        );
        #[cfg(windows)]
        {
            atomic(&self.root.join("current"), ver.as_bytes())
        }
        #[cfg(not(windows))]
        {
            let next = self.root.join("current.new");
            let _ = fs::remove_file(&next);
            link(Path::new("versions").join(ver).as_path(), &next)?;
            replace(&next, &self.root.join("current"))
        }
    }
    /// Caller keeps the session lock through process restart, preventing a new
    /// viewer from arriving between the idle check and the switch.
    pub fn activate(&self) -> Result<State> {
        let _lock = self.lock("update.lock", false)?;
        self.recover_locked()?;
        let mut state = self.state()?;
        let Some(next) = state.pending.take() else {
            return Ok(state);
        };
        if state.healthy {
            if let Some(good) = &state.current {
                self.remember_good(good)?;
            }
        }
        if state.healthy || state.previous.is_none() {
            state.previous = state.current.clone();
        }
        state.current = Some(next.clone());
        state.starts = 0;
        state.healthy = false;
        self.commit(state)
    }
    fn commit(&self, mut state: State) -> Result<State> {
        state.format = 1;
        state.pending_protocol = 0;
        state.request = None;
        state.peer_protocol = None;
        state.error = None;
        let journal = self.root.join("transaction.json");
        let before = self.state()?;
        let mut record = serde_json::to_value(&state)?;
        record["before"] = serde_json::to_value(&before)?;
        atomic(&journal, &serde_json::to_vec(&record)?)?;
        if let Err(error) = self.save(&state) {
            // A directory fsync may fail after the state rename succeeded.
            if self.state()?.current != before.current {
                self.save(&before)?;
            }
            fs::remove_file(journal)?;
            return Err(error);
        }
        if let Err(error) = self.flip(state.current.as_deref().context("missing target")?) {
            if let Some(old) = &before.current {
                self.flip(old)?;
            }
            self.save(&before)?;
            fs::remove_file(journal)?;
            return Err(error);
        }
        if state.healthy {
            self.remember_good(
                state
                    .current
                    .as_deref()
                    .context("missing rollback target")?,
            )?;
        }
        // Switch committed. A leftover journal is idempotent.
        let _ = fs::remove_file(journal);
        Ok(state)
    }
    pub fn recover(&self) -> Result<()> {
        let _lock = self.lock("update.lock", false)?;
        self.recover_locked()
    }
    fn recover_locked(&self) -> Result<()> {
        // Import pre-journal request markers before another transaction can consume them.
        let flags = ["rollback-requested", "requested", "apply-pending"];
        if self.read_record("state.json")?.is_some()
            && flags.iter().any(|name| self.root.join(name).is_file())
        {
            let mut state = self.state()?;
            if state.request.is_none() {
                if self.root.join("rollback-requested").is_file() && state.previous.is_some() {
                    state.request = Some("rollback".into());
                } else if self.root.join("requested").is_file()
                    || (self.root.join("apply-pending").is_file() && state.pending.is_some())
                {
                    state.request = Some("update".into());
                }
                self.save(&state)?;
            }
            for name in flags {
                let path = self.root.join(name);
                if path.is_file() {
                    fs::remove_file(path)?;
                }
            }
            #[cfg(unix)]
            File::open(&self.root)?.sync_all()?;
        }
        let valid = |v: &str| {
            version(v).is_ok() && self.binary(&self.root.join("versions").join(v)).is_file()
        };
        let mut saved = self.read_record("state.json")?;
        for (name, valid_record) in [
            ("state.json", saved.is_some()),
            (
                "transaction.json",
                self.read_record("transaction.json")?.is_some(),
            ),
        ] {
            let path = self.root.join(name);
            if !valid_record && path.exists() {
                fs::rename(path, self.root.join(format!("{name}.invalid-{}", now())))?;
            }
        }
        let journal = self.root.join("transaction.json");
        if let Some(mut next) = self.read_record("transaction.json")? {
            // A worker which survived a failed commit must keep serving its selected binary.
            if self.is_managed_process() {
                let record: serde_json::Value = serde_json::from_slice(&fs::read(&journal)?)?;
                if let Some(before) = record.get("before") {
                    let prior: State = serde_json::from_value(before.clone())?;
                    if prior.current.as_deref().is_some_and(|v| {
                        self.binary(&self.root.join("versions").join(v))
                            .canonicalize()
                            .ok()
                            == std::env::current_exe().ok()
                    }) {
                        next = prior;
                    }
                }
            }
            let runnable = next
                .current
                .as_deref()
                .filter(|v| valid(v))
                .is_some_and(|v| {
                    let Ok(exe) = self.launch_binary(v) else {
                        return false;
                    };
                    // A legacy install that never reported a version has no version command.
                    v == "0.0.0+legacy"
                        || preflight(
                            &exe,
                            v.split("+dev.").next().unwrap().trim_end_matches("+legacy"),
                        )
                        .is_ok()
                });
            if runnable {
                self.save(&next)?;
                self.flip(next.current.as_deref().unwrap())?;
                if next.healthy {
                    self.remember_good(next.current.as_deref().unwrap())?;
                }
                saved = Some(next);
            } else if saved.as_ref().and_then(|s| s.current.as_ref()) == next.current.as_ref() {
                saved = None;
            }
            fs::remove_file(journal)?;
        }
        let link = self.root.join("current");
        #[cfg(unix)]
        let resolved = fs::canonicalize(&link).ok();
        #[cfg(unix)]
        let points_at = |v: &str| {
            resolved.as_ref().is_some_and(|target| {
                fs::canonicalize(self.root.join("versions").join(v)).ok().as_ref() == Some(target)
            })
        };
        #[cfg(windows)]
        let pointer = fs::read_to_string(&link).ok();
        #[cfg(windows)]
        let points_at = |v: &str| pointer.as_deref() == Some(v);
        if let Some(current) = saved
            .as_ref()
            .and_then(|s| s.current.as_deref())
            .filter(|v| valid(v))
        {
            if !points_at(current) {
                self.flip(current)?;
            }
            return Ok(());
        }
        #[cfg(unix)]
        let pointer = fs::read_link(link).ok()
            .and_then(|p| p.file_name()?.to_str().map(str::to_owned))
            .filter(|v| valid(v) && points_at(v));
        let good = fs::read_to_string(self.root.join("known-good"))
            .ok()
            .filter(|v| valid(v))
            .or_else(|| pointer.filter(|v| valid(v)));
        if let Some(good) = good {
            self.flip(&good)?;
            self.save(&State {
                current: Some(good),
                healthy: true,
                ..State::default()
            })?;
        }
        Ok(())
    }
    pub fn rollback(&self) -> Result<State> {
        let _lock = self.lock("update.lock", false)?;
        self.recover_locked()?;
        let mut state = self.state()?;
        let highest = self.floor(&state)?.to_string();
        let previous = state.previous.take().context("no previous release")?;
        let floor_path = self.root.join("highest");
        if fs::read_to_string(&floor_path).ok().as_deref() != Some(&highest) {
            atomic(&floor_path, highest.as_bytes())?;
        }
        if let Some(failed) = state.current.take() {
            state.failed.push(failed);
        }
        state.current = Some(previous.clone());
        state.pending = None;
        state.available = None;
        state.starts = 0;
        state.healthy = true;
        self.commit(state)
    }
    pub fn starting(&self) -> Result<bool> {
        self.recover()?;
        let lock = self.lock("update.lock", false)?;
        let mut state = self.state()?;
        if state.current.is_none() || state.healthy {
            return Ok(false);
        }
        state.starts += 1;
        self.save(&state)?;
        let rollback = state.starts > 2 && state.previous.is_some();
        drop(lock);
        if rollback {
            self.rollback()?;
        }
        Ok(rollback)
    }
    fn remember_good(&self, selected: &str) -> Result<()> {
        let path = self.root.join("known-good");
        if fs::read_to_string(&path).ok().as_deref() != Some(selected) {
            atomic(&path, selected.as_bytes())?;
        }
        Ok(())
    }
    pub fn healthy(&self, selected: &str) -> Result<()> {
        let _lock = self.lock("update.lock", false)?;
        let mut state = self.state()?;
        ensure!(
            state.current.as_deref() == Some(selected),
            "selection changed during startup"
        );
        self.remember_good(selected)?;
        state.healthy = true;
        state.starts = 0;
        self.save(&state)?;
        // Keep current and previous; never delete a pending staged release.
        for entry in fs::read_dir(self.root.join("versions"))? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if entry.file_type()?.is_dir()
                && ![&state.current, &state.previous, &state.pending]
                    .iter()
                    .any(|v| v.as_deref() == Some(&name))
            {
                fs::remove_dir_all(entry.path())?;
            }
        }
        if self.root.join("downloads").exists() {
            fs::remove_dir_all(self.root.join("downloads"))?;
        }
        Ok(())
    }
    /// Developer-only local installation: no feed and no signature claim.
    /// The caller has explicitly chosen to execute its own source build.
    pub fn adopt(&self, source: &Path) -> Result<()> {
        self.migrate()?;
        let lock = self.lock("update.lock", false)?;
        let ver = format!("{}+dev.{}", env!("CARGO_PKG_VERSION"), now());
        let dest = self.root.join("versions").join(&ver);
        fn copy(from: &Path, to: &Path) -> Result<()> {
            fs::create_dir_all(to)?;
            for e in fs::read_dir(from)? {
                let e = e?;
                ensure!(
                    !e.file_type()?.is_symlink(),
                    "local staging tree contains a symlink"
                );
                if e.file_type()?.is_dir() {
                    copy(&e.path(), &to.join(e.file_name()))?;
                } else {
                    fs::copy(e.path(), to.join(e.file_name()))?;
                }
            }
            Ok(())
        }
        copy(source, &dest)?;
        sync_tree(&dest)?;
        #[cfg(unix)]
        File::open(dest.parent().unwrap())?.sync_all()?;
        let mut state = self.state()?;
        state.pending = Some(ver);
        self.save(&state)?;
        drop(lock);
        #[cfg(target_os = "macos")]
        let _idle = self.wait_for_mac_host_idle()?;
        #[cfg(not(target_os = "macos"))]
        let _idle = self.lock("session.lock", false)?;
        self.activate()?;
        self.install_links()
    }
    /// Import files installed by the old source scripts before replacing links.
    pub fn migrate(&self) -> Result<()> {
        let _lock = self.lock("update.lock", false)?;
        let mut state = self.state()?;
        if state.current.is_some() {
            return Ok(());
        }
        let bin =
            PathBuf::from(std::env::var_os("HOME").context("HOME missing")?).join(".local/bin");
        let name = if self.component == "host" {
            "sunnad"
        } else {
            "sunna"
        };
        if !fs::symlink_metadata(bin.join(name)).is_ok_and(|m| m.file_type().is_file()) {
            return Ok(());
        }
        let reported = binary_version(&bin.join(name))
            .ok()
            .and_then(|out| out.split_whitespace().find_map(|v| version(v).ok()));
        let legacy = format!(
            "{}+legacy",
            reported
                .unwrap_or_else(|| semver::Version::new(0, 0, 0))
                .to_string()
                .split('+')
                .next()
                .unwrap()
        );
        let ver = legacy.as_str();
        let dest = self.root.join("versions").join(ver).join("bin");
        fs::create_dir_all(&dest)?;
        for name in if self.component == "host" {
            vec!["sunnad", "sunna-host"]
        } else {
            vec!["sunna"]
        } {
            if bin.join(name).exists() {
                fs::copy(
                    bin.join(name),
                    dest.join(if name == "sunna" { "sunna-real" } else { name }),
                )?;
            }
        }
        state.current = Some(ver.into());
        state.healthy = true;
        sync_tree(dest.parent().unwrap())?;
        #[cfg(unix)]
        File::open(self.root.join("versions"))?.sync_all()?;
        self.flip(ver)?;
        self.remember_good(ver)?;
        self.save(&state)
    }
    #[cfg(target_os = "linux")]
    pub fn install_recovery(&self) -> Result<()> {
        use std::os::unix::fs::PermissionsExt;
        let version = self.state()?.current.context("missing installed host")?;
        let helper = self.launch_binary(&version)?.with_file_name("sunna-update");
        let mut file = tempfile::NamedTempFile::new_in(&self.root)?;
        std::io::copy(&mut File::open(helper)?, file.as_file_mut())?;
        file.as_file().set_permissions(fs::Permissions::from_mode(0o755))?;
        file.as_file().sync_all()?;
        file.persist(self.root.join("recovery"))?;
        File::open(&self.root)?.sync_all()?;
        Ok(())
    }
    pub fn install_links(&self) -> Result<()> {
        #[cfg(target_os = "linux")]
        if self.component == "host" { self.install_recovery()?; }
        #[cfg(windows)]
        {
            ensure!(
                self.component == "host",
                "Windows app releases are not available"
            );
            fs::write(self.root.join("sunna-host.cmd"), concat!(
                "@echo off\r\nset /p sunnaVersion=<\"%~dp0current\"\r\n",
                "if /i \"%~1\"==\"update\" goto update\r\n",
                "\"%~dp0versions\\%sunnaVersion%\\bin\\sunna-update.exe\" supervise host %*\r\nexit /b %errorlevel%\r\n",
                ":update\r\n\"%~dp0versions\\%sunnaVersion%\\bin\\sunna-update.exe\" update host %2 %3 %4 %5 %6 %7 %8 %9\r\n"
            ))?;
            return Ok(());
        }

        let bin =
            PathBuf::from(std::env::var_os("HOME").context("HOME is missing")?).join(".local/bin");
        fs::create_dir_all(&bin)?;
        #[cfg(unix)]
        for name in if self.component == "host" {
            vec!["sunnad", "sunna-host", "sunna-update"]
        } else {
            vec!["sunna"]
        } {
            let path = bin.join(name);
            let tmp = bin.join(format!("{name}.new"));
            let _ = fs::remove_file(&tmp);
            std::os::unix::fs::symlink(
                if cfg!(target_os = "macos") && name == "sunna" {
                    self.root.join("current/Sunna.app/Contents/MacOS/Sunna")
                } else {
                    self.root.join("current/bin").join(name)
                },
                &tmp,
            )?;
            replace(&tmp, &path)?;
        }
        #[cfg(target_os = "linux")]
        if self.component == "app" {
            let share = data_dir().parent().context("data parent")?.to_path_buf();
            fs::create_dir_all(share.join("applications"))?;
            let executable = bin
                .join("sunna")
                .display()
                .to_string()
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('%', "%%");
            let icon = self.root.join("current/share/sunna/icons/icon.svg");
            fs::write(share.join("applications/dev.sunna.app.desktop"), format!("[Desktop Entry]\nType=Application\nName=Sunna\nExec=\"{executable}\"\nIcon={}\nTerminal=false\nCategories=Network;RemoteAccess;\n", icon.display()))?;
        }
        #[cfg(target_os = "macos")]
        if self.component == "app" {
            let applications = if let Some(path) = std::env::var_os("SUNNA_APPLICATIONS") {
                PathBuf::from(path)
            } else {
                let system = PathBuf::from("/Applications");
                let probe = system.join(format!(".sunna-install-{}-{}", std::process::id(), now()));
                match OpenOptions::new().write(true).create_new(true).open(&probe) {
                    Ok(file) => {
                        drop(file);
                        fs::remove_file(probe)?;
                        system
                    }
                    Err(_) => PathBuf::from(std::env::var_os("HOME").context("HOME is missing")?)
                        .join("Applications"),
                }
            };
            fs::create_dir_all(&applications)?;
            let target = applications.join("Sunna.app");
            if target.is_dir() && !target.is_symlink() {
                fs::rename(
                    &target,
                    applications.join(format!("Sunna-before-download-{}.app", now())),
                )?;
            }
            let next = applications.join("Sunna.app.new");
            let _ = fs::remove_file(&next);
            link(&self.root.join("current/Sunna.app"), &next)?;
            replace(&next, &target)?;
        }
        Ok(())
    }
}

/// Session lifetime guard shared with the updater, across processes.
pub fn session_guard() -> Result<Option<File>> {
    if std::env::var_os("SUNNA_RECOVERY_ONLY").is_some() {
        return Ok(None);
    }
    match Updater::new("host") {
        Ok(updater)
            if updater.is_managed_process() || std::env::var_os("SUNNA_SUPERVISED").is_some() =>
        {
            Ok(Some(updater.lock("session.lock", true)?))
        }
        _ => Ok(None),
    }
}
pub mod host;
#[cfg(target_os = "linux")]
pub mod system_install;

#[cfg(test)]
mod tests;
