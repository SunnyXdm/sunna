//! Windows capture backend — Milestone 0c.
//!
//! Plan (research/03 §1): DXGI Desktop Duplication (`DuplicateOutput1` →
//! `AcquireNextFrame`) as the primary full-screen path, delivering shared
//! D3D11 textures straight into the encoder (zero-copy). Windows.Graphics.Capture
//! as fallback (hybrid-GPU laptops, 24H2 MPO regressions, HDR, per-window).
//! FP16 capture + P010 conversion for HDR later.
//!
//! Candidate crates: `windows` (windows-rs) directly; `windows-capture` for WGC.
