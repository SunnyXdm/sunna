# Adjudication of D-01..D-36 (by Claude, coordinator)

Default: the recommendation in disagreements.md is ACCEPTED for every D-xx not listed below. Note for the record: the editor's recommendations favoured proposal A on most items; each high-impact item (D-01..D-05, D-08, D-11..D-13, D-15, D-22, D-24, D-25, D-27..D-29, D-32, D-35) was re-checked independently and the A recommendation holds on its merits unless overridden here.

## Overrides / amendments

**D-09 Wire format — AMEND.** Keep every semantic distinction from A §5.2–5.4 (packet sequence vs encoded-frame ID vs capture ID; source time never rewritten on RTX; explicit FEC group geometry; epochs; bounded validation) and keep canonical integer-keyed CBOR for reliable control records (schema evolution and deterministic parsing matter more than postcard's compactness). But replace the fixed 64-byte per-datagram header with a compact layout:
- No per-packet magic, version or session epoch (QUIC connection + ALPN scope them; epochs change via reliable control and appear as a 1-byte epoch tag).
- 32-bit wrapping packet sequence and 32-bit µs send timestamps with RFC 3550-style extension; 64-bit values stay in the FEC-protected object prefix.
- Per-kind headers, targets: video/FEC symbol ≤ 28 bytes; audio ≤ 12 bytes; input ≤ 12 bytes + body; feedback prefixes shrunk accordingly.
- Show the final byte layouts and the overhead arithmetic (e.g. 5 ms Opus at 160 kbit/s, 1,180-byte video datagrams) versus the 64-byte version.

**D-05 Linux capture and headless scope — AMEND.** Portal (ScreenCast/RemoteDesktop + libei) for an attended existing desktop remains first. But the first Linux-host milestone must ALSO include a headless virtual-output path, because "use my Linux box from my laptop" commonly means a monitorless machine: GNOME via Mutter's ScreenCast/RemoteDesktop virtual monitors (the mechanism gnome-remote-desktop headless uses) and KWin virtual outputs, gated by an early spike that verifies caller access on current GNOME/KDE releases (mark UNVERIFIED until the spike passes). Owned-compositor (Wolf/Moonshine-style) headless gaming stays later. KMS capture stays out of the first Linux milestone.

**D-30 Update trust — OVERRIDE.** For v1 use proven platform-native updaters with two independent signatures instead of building TUF-style threshold metadata: macOS Sparkle 2 (EdDSA-signed appcast) + Developer ID/notarization; Windows signed manifest + Authenticode; Linux signed repositories/Flatpak. Offline signing keys, key separation, staged rollout (F §8.8), rollback protection. Revisit TUF when there are multiple channels/servers or a team. Rationale: one developer; boring and proven beats bespoke.

**D-14 Pairing — AMEND.** Keep A's primitives (full-secret invite link/QR; reviewed SPAKE2 from an established crate, no custom crypto). Make the short-code SPAKE2 flow CO-PRIMARY for the most common case: pairing two of your own devices side by side (the owner's M1/M2 scenario), not a fallback.

## Additional instructions for the final plan
- D-01: record the concrete iroh risks from the dossiers (noq is a quinn fork; relay is WebSocket-over-TCP so media on relay suffers head-of-line blocking → the relay tier gets an explicit degraded-media policy; open issues #4390/#4476 → pin versions, test, keep the transport seam so a direct upstream-quinn path remains possible).
- D-16: keep SwiftUI (macOS) + Tauri (elsewhere), but state explicitly that shells are thin and share one Rust core via a stable API, to bound the one-developer cost.
- D-34 and the other business items go into "Decisions requiring the owner's input".
