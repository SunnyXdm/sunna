//! Linux capture backend — Milestone 3 (Linux host is a headline differentiator).
//!
//! Plan (research/03 §1): KMS/DRM scanout capture headless-first (needs
//! cap_sys_admin; works with no display attached — the homelab story), with
//! xdg-desktop-portal ScreenCast + PipeWire (DMA-BUF, not SHM) as the
//! compositor-friendly fallback. X11 treated as legacy.
