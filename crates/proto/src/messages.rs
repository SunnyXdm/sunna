//! Control-stream messages: handshake, keepalive/RTT, input events.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ControlMessage {
    /// First message from the client after the control stream opens.
    Hello {
        version: u16,
        name: String,
        /// Shared session token; the host refuses clients that don't match.
        token: String,
        /// Largest stream the viewer can show 1:1, in physical pixels. The
        /// host scales capture on its GPU to fit, instead of the viewer
        /// scaling (and blurring) a larger stream.
        max_size: Option<(u32, u32)>,
    },
    /// Host refusal (bad token, busy...), sent instead of `HelloAck`.
    Refused { reason: String },
    /// Host reply describing the stream it is about to send.
    HelloAck {
        version: u16,
        name: String,
        width: u32,
        height: u32,
        fps: u32,
        codec: String,
    },
    /// RTT probe. `t_us` is the sender's clock at send time.
    Ping { seq: u32, t_us: u64 },
    /// Reply to `Ping`: `peer_t_us` echoes the ping's `t_us`.
    Pong { seq: u32, t_us: u64, peer_t_us: u64 },
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
    MouseMoveRel { dx: f32, dy: f32 },
    /// Absolute mouse position, normalized to 0..1 of the host stream area.
    MouseMoveAbs { x: f32, y: f32 },
    MouseButton { button: MouseButton, pressed: bool },
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
    fn roundtrip() {
        let messages = [
            ControlMessage::Hello {
                version: 0,
                name: "test".into(),
                token: "secret".into(),
                max_size: Some((2304, 1440)),
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
        ];
        for msg in &messages {
            let bytes = encode(msg).unwrap();
            assert_eq!(&decode(&bytes).unwrap(), msg);
        }
    }
}
