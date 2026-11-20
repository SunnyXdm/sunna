//! Saved computers share their first-use decision with the viewer subprocess.
//! Serialize admission for a record until its first pin is durable or its
//! explicitly permitted legacy request has been sent.
use std::{fs, io::Write, path::PathBuf, time::Duration};

use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sunna_proto::messages::{ControlMessage, Proof};
use sunna_transport::{auth, quinn::Connection, ClientConnection, ControlChannel};

pub const OLD_HOST: &str = "Needs an update. Install a signed download on this computer by hand.";
pub const WRONG_KEY: &str = "Sunna is there, but the key doesn't match.";

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Trust {
    pub identity: Option<String>,
    pub allow_older: bool,
    pub upgraded: bool,
    pub record: Option<String>,
}

const ALLOW_OLDER: &str = "allow-older";

impl Trust {
    fn path(&self) -> Result<Option<PathBuf>> {
        let Some(id) = &self.record else {
            return Ok(None);
        };
        ensure!(
            !id.is_empty()
                && id.len() <= 64
                && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'),
            "Invalid computer record."
        );
        Ok(Some(
            super::state_dir()
                .context("Missing computer store.")?
                .join("identities")
                .join(id),
        ))
    }

    pub fn pin(&self) -> Result<Option<String>> {
        if let Some(path) = self.path()? {
            // Listing computers should not wait for a network proof. Normal
            // reads see an atomic snapshot; admission rechecks under the lock.
            if path.with_extension("pending").try_exists()? {
                let _lock = Self::policy_lock(&path)?;
                Self::recover(&path)?;
                return self.read_pin();
            }
        }
        self.read_pin()
    }

    fn read_pin(&self) -> Result<Option<String>> {
        if let Some(pin) = &self.identity {
            ensure!(auth::valid_pin(pin), "Invalid computer identity.");
        }
        let Some(path) = self.path()? else {
            return Ok(self.identity.clone());
        };
        match fs::read_to_string(path) {
            Ok(marker) if marker == ALLOW_OLDER || marker == "disabled" => {
                Ok(self.identity.clone())
            }
            Ok(pin) => {
                ensure!(auth::valid_pin(&pin), "Invalid saved identity.");
                ensure!(
                    self.identity
                        .as_ref()
                        .is_none_or(|expected| expected == &pin),
                    auth::IDENTITY_CHANGED
                );
                Ok(Some(pin))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(self.identity.clone()),
            Err(e) => Err(e.into()),
        }
    }

    pub(super) async fn current_pin(&self) -> Result<Option<String>> {
        let _lock = self.lock().await?;
        if let Some(path) = self.path()? {
            Self::recover(&path)?;
        }
        self.read_pin()
    }

    pub async fn connect(
        &self,
        addr: std::net::SocketAddr,
        name: &str,
    ) -> Result<ClientConnection> {
        let pin = self.current_pin().await?;
        Ok(sunna_transport::connect_peer(addr, name, pin.as_deref()).await?)
    }

    /// Write an identity explicitly supplied when adding this computer again.
    /// Old snapshots using this record must see the new pin before admission.
    pub fn pin_from_link(&self, pin: &str) -> Result<()> {
        ensure!(auth::valid_pin(pin), "Invalid computer identity.");
        let path = self.path()?.context("Missing computer record.")?;
        let _lock = Self::policy_lock(&path)?;
        Self::recover(&path)?;
        Self::write_atomic(&path, pin)
    }

    fn policy_lock(path: &std::path::Path) -> Result<fs::File> {
        fs::create_dir_all(path.parent().unwrap())?;
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path.with_extension("lock"))?;
        fs2::FileExt::lock_exclusive(&lock)?;
        Ok(lock)
    }

    // Android keeps machines.json in filesDir and native state in filesDir/sunna.
    fn records_dir(policy: &std::path::Path) -> &std::path::Path {
        let dir = policy.parent().unwrap().parent().unwrap();
        #[cfg(target_os = "android")]
        let dir = dir.parent().unwrap();
        dir
    }

    fn staged(policy: &std::path::Path) -> PathBuf {
        Self::records_dir(policy).join(format!(
            ".records-{}.tmp",
            policy.file_name().unwrap().to_string_lossy()
        ))
    }

    // The rename of the staged list to machines.json is the commit point.
    // A surviving stage means the old policy still applies. Otherwise finish
    // the committed decision before any reader can admit another operation.
    fn recover(policy: &std::path::Path) -> Result<()> {
        let journal = policy.with_extension("pending");
        let decision = match fs::read_to_string(&journal) {
            Ok(value) => value,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        let stage = Self::staged(policy);
        let aborted = stage.try_exists()?;
        if !aborted {
            // Make the commit durable before persisting the matching policy.
            #[cfg(unix)]
            fs::File::open(Self::records_dir(policy))?.sync_all()?;
            ensure!(
                decision == "disabled" || decision == ALLOW_OLDER || auth::valid_pin(&decision),
                "Invalid pending identity decision."
            );
            Self::write_atomic(policy, &decision)?;
        }
        fs::remove_file(journal)?;
        #[cfg(unix)]
        fs::File::open(policy.parent().unwrap())?.sync_all()?;
        if aborted {
            fs::remove_file(stage)?;
        }
        Ok(())
    }

    /// Atomically replace the list and this record's admission policy. The app
    /// serializes list writers; the record lock also covers viewer subprocesses.
    pub fn save_records(&self, records: &str, from_link: bool) -> Result<()> {
        let policy = self.path()?.context("Missing computer record.")?;
        let _lock = Self::policy_lock(&policy)?;
        Self::recover(&policy)?;
        let pin = if from_link {
            Some(self.identity.clone().context("Missing link identity.")?)
        } else {
            self.read_pin()?
        };
        if let Some(pin) = &pin {
            ensure!(auth::valid_pin(pin), "Invalid computer identity.");
        }
        let decision = pin
            .as_deref()
            .unwrap_or(if self.allow_older && !self.upgraded {
                ALLOW_OLDER
            } else {
                "disabled"
            });
        let stage = Self::staged(&policy);
        Self::write_atomic(&stage, records)?;
        Self::write_atomic(&policy.with_extension("pending"), decision)?;
        let dir = Self::records_dir(&policy);
        if let Err(error) = fs::rename(stage, dir.join("machines.json")) {
            // The stage still exists. Reconcile the abandoned save now if the
            // filesystem permits it, otherwise the next locked read retries.
            let _ = Self::recover(&policy);
            return Err(error.into());
        }
        // The list is committed. A transient finalization failure is retried
        // under the same lock by the next admission or save.
        let _ = Self::recover(&policy);
        Ok(())
    }

    fn write_atomic(path: &std::path::Path, value: &str) -> Result<()> {
        let dir = path.parent().unwrap();
        let mut file = tempfile::NamedTempFile::new_in(dir)?;
        file.write_all(value.as_bytes())?;
        file.as_file().sync_all()?;
        file.persist(path)?;
        #[cfg(unix)]
        fs::File::open(dir)?.sync_all()?;
        Ok(())
    }

    fn save_pin(path: &std::path::Path, pin: &str) -> Result<()> {
        // An imported record can already contain a pin while its older sidecar
        // still records the exception. Reconcile both under the record lock.
        if fs::read_to_string(path).ok().as_deref() != Some(pin) {
            Self::write_atomic(path, pin)?;
        }
        Ok(())
    }

    /// Commit Edit's legacy exception. An already pinned record stays pinned.
    pub fn set_allow_older(&self, allow: bool) -> Result<()> {
        let path = self.path()?.context("Missing computer record.")?;
        let _lock = Self::policy_lock(&path)?;
        Self::recover(&path)?;
        if let Some(pin) = self.read_pin()? {
            return Self::save_pin(&path, &pin);
        }
        if allow && !self.upgraded {
            Self::write_atomic(&path, ALLOW_OLDER)?;
        } else if path.try_exists()? {
            fs::remove_file(&path)?;
            #[cfg(unix)]
            fs::File::open(path.parent().unwrap())?.sync_all()?;
        }
        Ok(())
    }

    async fn lock(&self) -> Result<Option<fs::File>> {
        let Some(path) = self.path()? else {
            return Ok(None);
        };
        fs::create_dir_all(path.parent().unwrap())?;
        let f = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path.with_extension("lock"))?;
        loop {
            match fs2::FileExt::try_lock_exclusive(&f) {
                Ok(()) => return Ok(Some(f)),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    tokio::time::sleep(Duration::from_millis(10)).await
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    pub(super) async fn request(
        &self,
        conn: &Connection,
        control: &mut ControlChannel,
        key: &str,
        operation: impl FnOnce(String) -> ControlMessage,
    ) -> Result<Option<String>> {
        let _lock = self.lock().await?;
        if let Some(path) = self.path()? {
            Self::recover(&path)?;
        }
        let pin = self.read_pin()?;
        let actual = auth::peer_pin(conn)?;
        ensure!(
            pin.as_ref().is_none_or(|expected| expected == &actual),
            auth::IDENTITY_CHANGED
        );
        if !auth::supports_proof(conn) {
            ensure!(
                pin.is_none() && !self.upgraded && self.allow_older && self.record.is_some(),
                OLD_HOST
            );
            let path = self.path()?.context("Missing computer record.")?;
            ensure!(
                fs::read_to_string(path).ok().as_deref() == Some(ALLOW_OLDER),
                OLD_HOST
            );
            control.send(&operation(key.to_owned())).await?;
            return Ok(None);
        }
        let (nonce, tag) = auth::client_proof(conn, key)?;
        control
            .send(&ControlMessage::AuthProof {
                nonce: nonce.to_vec(),
                proof: Proof(tag),
            })
            .await?;
        match control.recv().await? {
            ControlMessage::AuthProofAck { nonce: host, proof } => {
                auth::verify_host(conn, key, &nonce, &host, &proof.0)?
            }
            ControlMessage::Refused { reason } if reason == WRONG_KEY => anyhow::bail!(WRONG_KEY),
            ControlMessage::Refused { reason }
                if reason == "Run sunna-host new-key and update the saved key." =>
            {
                anyhow::bail!(reason)
            }
            _ => anyhow::bail!(auth::IDENTITY_CHANGED),
        }
        if let Some(path) = self.path()? {
            Self::save_pin(&path, &actual)?;
        }
        control.send(&operation(String::new())).await?;
        Ok(Some(actual))
    }
}

#[cfg(test)]
mod transaction_tests {
    use super::*;

    #[test]
    fn recovery_selects_policy_at_the_record_rename() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("identities/fixture");
        fs::create_dir(path.parent().unwrap()).unwrap();
        let old = auth::encode_pin(&[1; 32]);
        let new = auth::encode_pin(&[2; 32]);
        let stage = Trust::staged(&path);
        let pending = path.with_extension("pending");
        for committed in [false, true] {
            Trust::write_atomic(&path, &old).unwrap();
            Trust::write_atomic(&stage, "updated records").unwrap();
            Trust::write_atomic(&pending, &new).unwrap();
            if committed {
                fs::rename(&stage, dir.path().join("machines.json")).unwrap();
            }
            Trust::recover(&path).unwrap();
            assert_eq!(
                fs::read_to_string(&path).unwrap(),
                if committed { &new } else { &old }.as_str()
            );
            assert!(!pending.exists() && !stage.exists());
            Trust::recover(&path).unwrap();
        }
    }
}
