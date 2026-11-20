//! Proofs are specific to one TLS handshake and one direction.
use super::{Result, TransportError};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use quinn::Connection;
use ring::{
    digest, hmac,
    rand::{SecureRandom, SystemRandom},
};
use rustls::pki_types::CertificateDer;

pub const IDENTITY_CHANGED: &str = "This isn't the computer you added, or it was set up again. If you reinstalled it, add it again with its new link.";
const INVALID: TransportError =
    TransportError::Authentication("The computer's proof didn't match.");
pub const ALPN: &[u8] = b"sunna/8/auth1";

pub fn encode_pin(digest: &[u8; 32]) -> String {
    URL_SAFE_NO_PAD.encode(digest)
}

pub fn valid_pin(pin: &str) -> bool {
    URL_SAFE_NO_PAD.decode(pin).is_ok_and(|v| v.len() == 32)
}

pub fn certificate_pin(cert: &CertificateDer<'_>) -> Result<String> {
    let parsed = webpki::EndEntityCert::try_from(cert).map_err(|_| INVALID)?;
    Ok(URL_SAFE_NO_PAD.encode(digest::digest(
        &digest::SHA256,
        parsed.subject_public_key_info().as_ref(),
    )))
}

pub fn peer_pin(connection: &Connection) -> Result<String> {
    let certs = connection
        .peer_identity()
        .and_then(|v| v.downcast::<Vec<CertificateDer<'static>>>().ok())
        .ok_or(INVALID)?;
    certificate_pin(certs.first().ok_or(INVALID)?)
}

pub fn supports_proof(connection: &Connection) -> bool {
    connection
        .handshake_data()
        .and_then(|data| data.downcast::<quinn::crypto::rustls::HandshakeData>().ok())
        .is_some_and(|data| data.protocol.as_deref() == Some(ALPN))
}

fn nonce() -> Result<[u8; 32]> {
    let mut value = [0; 32];
    SystemRandom::new()
        .fill(&mut value)
        .map_err(|_| TransportError::Authentication("No secure random numbers available."))?;
    Ok(value)
}

fn material(
    connection: &Connection,
    key: &str,
    role: &[u8],
    client: &[u8],
    host: &[u8],
) -> Result<(hmac::Key, Vec<u8>)> {
    if !matches!(key.len(), 32 | 64) || !key.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(TransportError::Authentication(
            "Run sunna-host new-key and update the saved key.",
        ));
    }
    if client.len() != 32 || (role == b"host" && host.len() != 32) {
        return Err(INVALID);
    }
    let mut exporter = [0; 32];
    connection
        .export_keying_material(&mut exporter, b"sunna auth v1", b"")
        .map_err(|_| INVALID)?;
    Ok((
        hmac::Key::new(hmac::HMAC_SHA256, key.as_bytes()),
        [role, &exporter, client, host].concat(),
    ))
}

pub fn client_proof(connection: &Connection, key: &str) -> Result<([u8; 32], Vec<u8>)> {
    let client = nonce()?;
    let (key, data) = material(connection, key, b"client", &client, &[])?;
    Ok((client, hmac::sign(&key, &data).as_ref().to_vec()))
}

pub fn answer(
    connection: &Connection,
    key: &str,
    client: &[u8],
    proof: &[u8],
) -> Result<([u8; 32], Vec<u8>)> {
    let (secret, data) = material(connection, key, b"client", client, &[])?;
    hmac::verify(&secret, &data, proof).map_err(|_| INVALID)?;
    let host = nonce()?;
    let (secret, data) = material(connection, key, b"host", client, &host)?;
    Ok((host, hmac::sign(&secret, &data).as_ref().to_vec()))
}

pub fn verify_host(
    connection: &Connection,
    key: &str,
    client: &[u8],
    host: &[u8],
    proof: &[u8],
) -> Result<()> {
    let (key, data) = material(connection, key, b"host", client, host)?;
    hmac::verify(&key, &data, proof).map_err(|_| INVALID)
}
