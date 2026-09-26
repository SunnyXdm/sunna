//! Control-stream messages: handshake, keepalive/RTT, input events.

use serde::{Deserialize, Serialize};

/// Requested changes; absent fields keep the current value or host default.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StreamSettings {
    /// "hevc", "h264", or "raw".
    pub codec: Option<String>,
    /// Largest stream the viewer wants, in physical pixels.
    pub max_size: Option<(u32, u32)>,
    pub max_bitrate_kbps: Option<u32>,
    pub fps: Option<u32>,
    pub fast_lane: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ControlMessage {
    /// First message from the client after the control stream opens.
    Hello {
        version: u16,
        name: String,
        /// Shared session token; the host refuses clients that don't match.
        token: String,
        stream: StreamSettings,
    },
    /// Host refusal (bad token, busy...), sent instead of `HelloAck`.
    Refused {
        reason: String,
    },
    /// Host reply describing the stream it is about to send.
    HelloAck {
        version: u16,
        name: String,
        width: u32,
        height: u32,
        fps: u32,
        codec: String,
        fast_lane: bool,
    },
    SetStream(StreamSettings),
    StreamChanged {
        epoch: u8,
        width: u32,
        height: u32,
        fps: u32,
        codec: String,
        fast_lane: bool,
    },
    SetStreamFailed {
        reason: String,
    },
    Probe {
        token: String,
    },
    ProbeAck {
        name: String,
        version: u16,
        busy: bool,
        token_ok: bool,
    },
    /// RTT probe. `t_us` is the sender's clock at send time.
    Ping {
        seq: u32,
        t_us: u64,
    },
    /// Reply to `Ping`: `peer_t_us` echoes the ping's `t_us`.
    Pong {
        seq: u32,
        t_us: u64,
        peer_t_us: u64,
    },
    Input(InputEvent),
    /// Client lost a frame (or can't decode): the next frame must be an IDR.
    /// The codec-level recovery backstop (research/03 §4) until LTR/RFI lands.
    RequestKeyframe,
    /// Per-second receiver stats; drives the host's bitrate adaptation.
    ReceiverReport {
        frames_complete: u32,
        frames_dropped: u32,
        chunks_recovered: u32,
        /// 0 when no samples this window.
        e2e_p95_us: u64,
    },
    /// Per-second host stats, for the viewer's stats overlay.
    HostStats(HostStats),
    Bye,
    /// Follows `ProbeAck` when the token matched, so launchers can show the
    /// machine properly. Last in the enum and sent after the ack: older
    /// launchers stop reading at the ack, and older hosts never send it.
    HostInfo(HostAbout),
}

/// What a host is, for display.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostAbout {
    /// "macOS 26.0", "Arch Linux", "Ubuntu 22.04.5 LTS".
    pub os: String,
    /// "laptop", "desktop", "vm", "server", or empty when unknown.
    pub device: String,
    /// "MacBook Air", "Mac mini"; empty when unknown.
    pub model: String,
    /// The shared display in pixels.
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClipboardKind {
    Text,
    Png,
}

/// One clipboard transfer. Sent on its own unidirectional stream (see
/// `sunna_transport::send_clipboard`), not the control stream.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipboardData {
    pub kind: ClipboardKind,
    pub data: Vec<u8>,
}

// Logged in error paths; clipboard contents stay private.
impl std::fmt::Debug for ClipboardData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClipboardData")
            .field("kind", &self.kind)
            .field("size", &self.data.len())
            .finish()
    }
}

/// What the host did in the last second.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HostStats {
    /// Frames encoded and sent.
    pub fps: u32,
    pub sent_kbps: u32,
    /// Where the host's congestion control has the encoder aimed.
    pub target_kbps: u32,
    pub encode_us_p50: u32,
    pub encode_us_p95: u32,
    pub keyframes: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    X1,
    X2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GestureKind {
    Swipe,
    Pinch,
    Rotate,
    SmartZoom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GesturePhase {
    Begin,
    Update,
    End,
    Cancel,
}

/// Input events. Sent client → host over the reliable control stream for now;
/// may move to duplicated datagrams if stream head-of-line ever shows up in traces.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InputEvent {
    /// Keyboard by HID/OS scancode — never translated characters (see research/03 §6).
    Key {
        scancode: u16,
        pressed: bool,
        /// OS key auto-repeat (the host OS won't repeat injected keys itself).
        repeat: bool,
    },
    /// Relative mouse motion (FPS games, pointer-lock mode).
    MouseMoveRel {
        dx: f32,
        dy: f32,
    },
    /// Absolute mouse position, normalized to 0..1 of the host stream area.
    MouseMoveAbs {
        x: f32,
        y: f32,
    },
    MouseButton {
        button: MouseButton,
        pressed: bool,
    },
    /// Scroll with trackpad phase information so hosts can replay native
    /// momentum scrolling (research/05 §4: the one high-fidelity mapping).
    Scroll {
        dx: f32,
        dy: f32,
        phase: Option<GesturePhase>,
        momentum: bool,
    },
    /// Semantic gesture channel (research/05 §4). Raw contacts are a planned
    /// extension of this message, kept out of v0 for simplicity.
    Gesture {
        kind: GestureKind,
        phase: GesturePhase,
        fingers: u8,
        dx: f32,
        dy: f32,
        velocity_x: f32,
        velocity_y: f32,
        /// log2 of the scale delta for pinch; 0.0 otherwise.
        scale_delta: f32,
        /// radians; 0.0 unless kind == Rotate.
        rotation_delta: f32,
    },
}

pub fn encode(msg: &ControlMessage) -> Result<Vec<u8>, postcard::Error> {
    postcard::to_stdvec(msg)
}

pub fn decode(bytes: &[u8]) -> Result<ControlMessage, postcard::Error> {
    postcard::from_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_debug_redacts_contents() {
        let data = ClipboardData {
            kind: ClipboardKind::Text,
            data: b"private clipboard".to_vec(),
        };
        let debug = format!("{data:?}");
        assert!(debug.contains("Text"));
        assert!(debug.contains("size: 17"));
        assert!(!debug.contains("private"));
        assert!(!debug.contains("112, 114"));
    }

    #[test]
    fn roundtrip() {
        let messages = [
            ControlMessage::Hello {
                version: 0,
                name: "test".into(),
                token: "secret".into(),
                stream: StreamSettings {
                    max_size: Some((2304, 1440)),
                    ..Default::default()
                },
            },
            ControlMessage::Input(InputEvent::Gesture {
                kind: GestureKind::Swipe,
                phase: GesturePhase::Update,
                fingers: 3,
                dx: -12.5,
                dy: 0.0,
                velocity_x: -300.0,
                velocity_y: 0.0,
                scale_delta: 0.0,
                rotation_delta: 0.0,
            }),
            ControlMessage::Ping { seq: 7, t_us: 123 },
            ControlMessage::SetStream(StreamSettings {
                codec: Some("h264".into()),
                max_size: Some((160, 90)),
                max_bitrate_kbps: Some(2000),
                fps: Some(30),
                fast_lane: Some(false),
            }),
            ControlMessage::StreamChanged {
                epoch: 255,
                width: 160,
                height: 90,
                fps: 30,
                codec: "h264".into(),
                fast_lane: true,
            },
            ControlMessage::SetStreamFailed {
                reason: "unsupported codec".into(),
            },
            ControlMessage::Probe {
                token: "secret".into(),
            },
            ControlMessage::ProbeAck {
                name: "host".into(),
                version: 3,
                busy: true,
                token_ok: true,
            },
            ControlMessage::HostInfo(HostAbout {
                os: "Arch Linux".into(),
                device: "desktop".into(),
                model: String::new(),
                width: 2560,
                height: 1440,
            }),
            ControlMessage::HelloAck {
                version: 3,
                name: "host".into(),
                width: 320,
                height: 180,
                fps: 60,
                codec: "raw".into(),
                fast_lane: false,
            },
        ];
        for msg in &messages {
            let bytes = encode(msg).unwrap();
            assert_eq!(&decode(&bytes).unwrap(), msg);
        }
    }
}
