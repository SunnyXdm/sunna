//! Wire protocol for Sunna: control messages, media packetization, latency stats.
//!
//! Media frames travel as QUIC unreliable datagrams (packetized in [`media`]),
//! control and input travel over one reliable stream ([`messages`]).

pub mod media;
pub mod messages;
pub mod stats;
pub mod tiles;

/// ALPN identifier for the Sunna protocol.
pub const ALPN: &[u8] = b"sunna/0";

/// Bumped on every incompatible wire change while the protocol is unstable.
pub const PROTOCOL_VERSION: u16 = 2;

/// Current wall-clock time in microseconds since the unix epoch.
///
/// Used for wire timestamps. Comparisons are only meaningful between processes
/// on the same machine until in-protocol clock sync exists (Milestone 1).
pub fn now_us() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_micros() as u64
}
