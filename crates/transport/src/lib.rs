//! QUIC transport for Sunna, on quinn.
//!
//! One reliable bidirectional stream carries control messages ([`ControlChannel`]),
//! media rides QUIC unreliable datagrams (RFC 9221) sent directly by the host
//! pipeline via [`quinn::Connection::send_datagram`].
//!
pub mod auth;
mod identity;
pub use identity::{load_or_create_key, HostIdentity};

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use quinn::{ClientConfig, Connection, Endpoint, ServerConfig, TransportConfig};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use sunna_proto::messages::{self, ControlMessage};

pub use quinn;

const MAX_CONTROL_MESSAGE: usize = 1024 * 1024;
const IDLE_TIMEOUT: Duration = Duration::from_secs(10);
const KEEP_ALIVE: Duration = Duration::from_secs(2);
/// Send side is deliberately small: it bounds how much stale video can queue
/// when the path stalls (latest-frame-wins must hold at the sender too), while
/// still fitting one large keyframe. The host also drops whole frames at the
/// source when the buffer backs up (see sunna-host).
pub const DATAGRAM_SEND_BUFFER_SIZE: usize = 1024 * 1024;
const DATAGRAM_RECV_BUFFER: usize = 4 * 1024 * 1024;

#[derive(thiserror::Error, Debug)]
pub enum TransportError {
    #[error("{0}")]
    Authentication(&'static str),
    #[error("tls setup: {0}")]
    Tls(#[from] rustls::Error),
    #[error("certificate generation: {0}")]
    CertGen(#[from] rcgen::Error),
    #[error("quic crypto config: {0}")]
    Crypto(#[from] quinn::crypto::rustls::NoInitialCipherSuite),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("connect: {0}")]
    Connect(#[from] quinn::ConnectError),
    #[error("connection: {0}")]
    Connection(#[from] quinn::ConnectionError),
    #[error("stream write: {0}")]
    Write(#[from] quinn::WriteError),
    #[error("stream read: {0}")]
    Read(#[from] quinn::ReadExactError),
    #[error("codec: {0}")]
    Codec(#[from] postcard::Error),
    #[error("control message too large ({0} bytes)")]
    MessageTooLarge(usize),
    #[error("stream read: {0}")]
    ReadToEnd(#[from] quinn::ReadToEndError),
    #[error("stream closed: {0}")]
    Closed(#[from] quinn::ClosedStream),
}

/// First bytes of a clipboard transfer stream. Each transfer gets its own
/// unidirectional stream, below the control stream's priority, so a large
/// image never queues ahead of input and pings.
pub const CLIPBOARD_STREAM_MAGIC: [u8; 4] = *b"SCB1";
/// 16 MiB of content plus the postcard envelope.
const MAX_CLIPBOARD_STREAM: usize = 16 * 1024 * 1024 + 64;

/// Send one clipboard transfer on a stream of its own.
pub async fn send_clipboard(
    connection: &quinn::Connection,
    data: &messages::ClipboardData,
) -> Result<()> {
    let body = postcard::to_stdvec(data)?;
    if body.len() > MAX_CLIPBOARD_STREAM {
        return Err(TransportError::MessageTooLarge(body.len()));
    }
    let mut stream = connection.open_uni().await?;
    stream.set_priority(-1)?;
    stream.write_all(&CLIPBOARD_STREAM_MAGIC).await?;
    stream.write_all(&body).await?;
    stream.finish()?;
    Ok(())
}

/// Read a clipboard transfer from a stream whose magic was already read.
pub async fn read_clipboard(mut stream: quinn::RecvStream) -> Result<messages::ClipboardData> {
    let body = stream.read_to_end(MAX_CLIPBOARD_STREAM).await?;
    Ok(postcard::from_bytes(&body)?)
}

pub type Result<T> = std::result::Result<T, TransportError>;

fn crypto_provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

fn transport_config() -> TransportConfig {
    let mut config = TransportConfig::default();
    // Congestion control for everything on the connection, video datagrams
    // included. BBR measures what the path carries; the loss-based default
    // (Cubic) reads Wi-Fi's random loss as congestion and, at a few percent
    // loss, throttles video to ~2 Mbps. SUNNA_CC=cubic|newreno for experiments.
    match std::env::var("SUNNA_CC").as_deref() {
        Ok("cubic") => {}
        Ok("newreno") => {
            config.congestion_controller_factory(Arc::new(quinn::congestion::NewRenoConfig::default()));
        }
        _ => {
            config.congestion_controller_factory(Arc::new(quinn::congestion::BbrConfig::default()));
        }
    }
    config.max_idle_timeout(Some(IDLE_TIMEOUT.try_into().expect("valid idle timeout")));
    config.keep_alive_interval(Some(KEEP_ALIVE));
    config.datagram_receive_buffer_size(Some(DATAGRAM_RECV_BUFFER));
    config.datagram_send_buffer_size(DATAGRAM_SEND_BUFFER_SIZE);
    // Probe for larger packets only up to what a 1280-byte path carries
    // over IPv4 (Tailscale's MTU, and IPv6's minimum). Probing past the
    // path's MTU just fails to send: Linux ignores that, Windows logs every
    // failure as a warning. Video datagrams stay at 1200 bytes anyway.
    let mut mtu = quinn::MtuDiscoveryConfig::default();
    mtu.upper_bound(MTU_PROBE_LIMIT);
    config.mtu_discovery_config(Some(mtu));
    config
}

/// The largest UDP payload MTU discovery tries: 1280 bytes less the IPv4
/// and UDP headers.
const MTU_PROBE_LIMIT: u16 = 1280 - 20 - 8;

/// QUIC listener presenting the configured host identity.
pub struct Server {
    pub endpoint: Endpoint,
    pub cert: CertificateDer<'static>,
}

impl Server {
    pub fn bind(addr: SocketAddr) -> Result<Self> {
        Self::with_identity(addr, HostIdentity::generate()?)
    }

    pub fn with_identity(addr: SocketAddr, identity: HostIdentity) -> Result<Self> {
        let cert = identity.cert;
        let key = PrivateKeyDer::from(identity.key);
        let mut tls = rustls::ServerConfig::builder_with_provider(crypto_provider())
            .with_protocol_versions(&[&rustls::version::TLS13])?
            .with_no_client_auth()
            .with_single_cert(vec![cert.clone()], key)?;
        tls.alpn_protocols = vec![auth::ALPN.to_vec(), sunna_proto::ALPN.to_vec()];

        let crypto = quinn::crypto::rustls::QuicServerConfig::try_from(tls)?;
        let mut server_config = ServerConfig::with_crypto(Arc::new(crypto));
        server_config.transport_config(Arc::new(transport_config()));

        let endpoint = Endpoint::server(server_config, addr)?;
        Ok(Self { endpoint, cert })
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.endpoint.local_addr()?)
    }

    /// Accept the next connection. `None` when the endpoint is closed.
    pub async fn accept(&self) -> Option<Result<Connection>> {
        let incoming = self.endpoint.accept().await?;
        Some(async { Ok(incoming.await?) }.await)
    }
}

/// Client connection; keeps its endpoint alive alongside the connection.
pub struct ClientConnection {
    pub endpoint: Endpoint,
    pub connection: Connection,
}

/// Connect trusting exactly `cert` (in-process bench, tests).
pub async fn connect_trusted(
    addr: SocketAddr,
    server_name: &str,
    cert: &CertificateDer<'static>,
) -> Result<ClientConnection> {
    let mut roots = rustls::RootCertStore::empty();
    roots.add(cert.clone())?;
    let tls = rustls::ClientConfig::builder_with_provider(crypto_provider())
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .with_root_certificates(roots)
        .with_no_client_auth();
    finish_connect(tls, addr, server_name).await
}

/// Connect without verifying the server certificate. Dev tooling only —
/// encrypts against passive listeners but is trivially MITM-able.
pub async fn connect_insecure(addr: SocketAddr, server_name: &str) -> Result<ClientConnection> {
    connect_peer(addr, server_name, None).await
}

/// A missing pin requires application-level mutual proof before first use.
pub async fn connect_peer(addr: SocketAddr, server_name: &str, pin: Option<&str>) -> Result<ClientConnection> {
    let mismatch = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let provider = crypto_provider();
    let tls = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(PeerVerifier(provider, pin.map(str::to_owned), mismatch.clone())))
        .with_no_client_auth();
    let result = finish_connect(tls, addr, server_name).await;
    if mismatch.load(std::sync::atomic::Ordering::Acquire) {
        return Err(TransportError::Authentication(auth::IDENTITY_CHANGED));
    }
    result
}

async fn finish_connect(
    mut tls: rustls::ClientConfig,
    addr: SocketAddr,
    server_name: &str,
) -> Result<ClientConnection> {
    tls.alpn_protocols = vec![auth::ALPN.to_vec(), sunna_proto::ALPN.to_vec()];
    let crypto =
        quinn::crypto::rustls::QuicClientConfig::try_from(tls).map_err(TransportError::Crypto)?;
    let mut client_config = ClientConfig::new(Arc::new(crypto));
    client_config.transport_config(Arc::new(transport_config()));

    let bind: SocketAddr = if addr.is_ipv6() {
        "[::]:0".parse().expect("valid bind addr")
    } else {
        "0.0.0.0:0".parse().expect("valid bind addr")
    };
    let mut endpoint = Endpoint::client(bind)?;
    endpoint.set_default_client_config(client_config);
    let connection = endpoint.connect(addr, server_name)?.await?;
    Ok(ClientConnection {
        endpoint,
        connection,
    })
}

#[derive(Debug)]
struct PeerVerifier(Arc<rustls::crypto::CryptoProvider>, Option<String>, Arc<std::sync::atomic::AtomicBool>);

impl rustls::client::danger::ServerCertVerifier for PeerVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        if self.1.as_ref().is_some_and(|pin| auth::certificate_pin(end_entity).as_ref().ok() != Some(pin)) {
            self.2.store(true, std::sync::atomic::Ordering::Release);
            return Err(rustls::Error::InvalidCertificate(rustls::CertificateError::ApplicationVerificationFailure));
        }
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

/// Length-prefixed postcard control messages over one bidirectional stream.
pub struct ControlChannel {
    sender: ControlSender,
    receiver: ControlReceiver,
}

impl ControlChannel {
    /// Client side: open the stream. Must send the admission message
    /// promptly — QUIC streams are lazy and the server cannot accept an unopened stream.
    pub async fn open(connection: &Connection) -> Result<Self> {
        let (send, recv) = connection.open_bi().await?;
        Ok(Self::from_streams(send, recv))
    }

    /// Host side: accept the client's control stream.
    pub async fn accept(connection: &Connection) -> Result<Self> {
        let (send, recv) = connection.accept_bi().await?;
        Ok(Self::from_streams(send, recv))
    }

    fn from_streams(send: quinn::SendStream, recv: quinn::RecvStream) -> Self {
        Self {
            sender: ControlSender { send },
            receiver: ControlReceiver { recv },
        }
    }

    pub async fn send(&mut self, msg: &ControlMessage) -> Result<()> {
        self.sender.send(msg).await
    }

    /// Not cancellation-safe: see [`ControlReceiver::recv`].
    pub async fn recv(&mut self) -> Result<ControlMessage> {
        self.receiver.recv().await
    }

    /// Deliver a final reply before dropping the last connection handle.
    pub async fn finish(mut self) -> Result<()> {
        self.sender.send.finish()?;
        let _ = self.sender.send.stopped().await;
        Ok(())
    }

    /// Split into halves so a dedicated task can own the receiver.
    pub fn into_split(self) -> (ControlSender, ControlReceiver) {
        (self.sender, self.receiver)
    }
}

pub struct ControlSender {
    send: quinn::SendStream,
}

impl ControlSender {
    pub async fn send(&mut self, msg: &ControlMessage) -> Result<()> {
        let bytes = messages::encode(msg)?;
        if bytes.len() > MAX_CONTROL_MESSAGE {
            return Err(TransportError::MessageTooLarge(bytes.len()));
        }
        let mut framed = Vec::with_capacity(4 + bytes.len());
        framed.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        framed.extend_from_slice(&bytes);
        self.send.write_all(&framed).await?;
        Ok(())
    }
}

pub struct ControlReceiver {
    recv: quinn::RecvStream,
}

impl ControlReceiver {
    /// **Not cancellation-safe.** A message is read in two steps (length,
    /// then body); dropping this future between them loses the bytes already
    /// read and desyncs the framing. Never race it in `tokio::select!` —
    /// give the receiver its own task and forward messages over a channel.
    pub async fn recv(&mut self) -> Result<ControlMessage> {
        let mut len_bytes = [0u8; 4];
        self.recv.read_exact(&mut len_bytes).await?;
        let len = u32::from_be_bytes(len_bytes) as usize;
        if len > MAX_CONTROL_MESSAGE {
            return Err(TransportError::MessageTooLarge(len));
        }
        let mut buf = vec![0u8; len];
        self.recv.read_exact(&mut buf).await?;
        Ok(messages::decode(&buf)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    #[test]
    fn pinned_server_still_needs_a_valid_handshake_signature() {
        use rustls::client::danger::ServerCertVerifier;
        use rustls::internal::msgs::codec::Codec;
        let id = HostIdentity::generate().unwrap();
        let verifier = PeerVerifier(crypto_provider(), Some(id.pin().unwrap()), Default::default());
        // ECDSA P-256, followed by a one-byte invalid signature.
        let sig = rustls::DigitallySignedStruct::read_bytes(&[4, 3, 0, 1, 0]).unwrap();
        assert!(verifier.verify_tls13_signature(b"fixture", &id.cert, &sig).is_err());
    }

    #[tokio::test]
    async fn loopback_control_and_datagram() {
        let server = Server::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let addr = server.local_addr().unwrap();
        let cert = server.cert.clone();

        let server_task = tokio::spawn(async move {
            let connection = server.accept().await.unwrap().unwrap();
            let mut control = ControlChannel::accept(&connection).await.unwrap();
            let hello = control.recv().await.unwrap();
            assert!(matches!(hello, ControlMessage::Hello { .. }));
            control
                .send(&ControlMessage::HelloAck {
                    version: sunna_proto::PROTOCOL_VERSION,
                    name: "test-host".into(),
                    width: 640,
                    height: 360,
                    fps: 60,
                    codec: "raw".into(),
                    fast_lane: false,
                    audio: false,
                })
                .await
                .unwrap();
            connection
                .send_datagram(Bytes::from_static(b"media"))
                .unwrap();
            // Keep the connection alive until the client is done.
            let bye = control.recv().await.unwrap();
            assert!(matches!(bye, ControlMessage::Bye));
        });

        let client = connect_trusted(addr, "sunna", &cert).await.unwrap();
        let mut control = ControlChannel::open(&client.connection).await.unwrap();
        control
            .send(&ControlMessage::Hello {
                version: sunna_proto::PROTOCOL_VERSION,
                name: "test-client".into(),
                token: String::new(),
                stream: messages::StreamSettings::default(),
                device: String::new(),
            })
            .await
            .unwrap();
        let ack = control.recv().await.unwrap();
        assert!(matches!(ack, ControlMessage::HelloAck { width: 640, .. }));
        let datagram = client.connection.read_datagram().await.unwrap();
        assert_eq!(&datagram[..], b"media");
        control.send(&ControlMessage::Bye).await.unwrap();

        server_task.await.unwrap();
    }
}
