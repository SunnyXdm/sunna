//! macOS capture backend — Milestone 0b.
//!
//! Plan (research/03 §1): ScreenCaptureKit `SCStream` with small `queueDepth`,
//! `minimumFrameInterval` at native refresh, IOSurface-backed sample buffers
//! handed zero-copy to VideoToolbox. Handle the "no new frame on static screen"
//! case with a keepalive timer; Screen Recording TCC permission required
//! (Sequoia re-prompts monthly — surface this in the host UI).
//!
//! Candidate binding crate: `screencapturekit-rs`; fall back to objc2 bindings.
