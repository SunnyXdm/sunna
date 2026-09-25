Read ~/research-work/context.md first. Phase 2: write a COMPLETE ARCHITECTURE PROPOSAL for Sunna.

Inputs you must read in full:
- The three phase-1 dossiers: ~/research-work/dossier-astra.md (game-streaming engines), ~/research-work/dossier-fable.md (RustDesk, Tailscale/iroh, Parsec, ease of use, security, distribution), ~/research-work/dossier-claude.md (FreeRDP/RDP graphics, gnome-remote-desktop, macOS/Linux/Windows platform internals, Apple Screen Sharing).
- The Sunna repo (/home/sunny/projects/sunna): README, research/00..07, and the code. Read-only.
You may do additional targeted research (repos in ~/research-repos, web) to close gaps, but the dossiers are the main evidence base.

Goal (owner's words, paraphrased): the whole architecture that beats RustDesk, macOS Screen Sharing, and Moonlight/Sunshine (+ forks), is as easy to use as Parsec, and performs better than Parsec. Very, very thorough.

Write the proposal with these sections (all mandatory; go deep, be concrete — exact formats, numbers, APIs, state machines):
1. Product definition: who it's for, the promise, and measurable targets versus each competitor (latency p50/p99 on LAN and WAN, quality at fixed bandwidth, connection success rate, time-to-first-frame, steps-to-first-session).
2. System overview: components/processes per machine, crate layout, data-flow and control-flow (ASCII diagrams welcome), threading model, where every queue lives and its bound.
3. Host engine per OS (macOS first, Linux second, Windows third): capture, virtual display, encode (exact settings per codec/vendor), audio, input injection, cursor, clipboard, service/daemon model, permissions/onboarding, sleep/lock/login-window behaviour.
4. Clients per OS (+ browser, + mobile/tablet stance): decode, presentation, frame pacing, input capture fidelity (keyboard layouts, scroll phases, gestures, pen, gamepads), UI.
5. Media transport: packet formats (byte layouts), FEC/NACK/RTX policy, reference repair (VT LTR, NVENC RFI, etc.), congestion/rate control coupled to the encoder, pacing, stream prioritization (input > audio > video > refinement > bulk), encryption, MTU handling, QUIC usage details (quinn customization), relay-aware behaviour.
6. Quality system: desktop mode (text-sharp, lossless refinement, 4:4:4 strategy, color management), game mode (fps, latency, HDR), automatic content/mode switching, multi-monitor.
7. Connectivity: identity/addressing, rendezvous, NAT traversal, relays (hosted + self-hosted), LAN discovery, IPv6, mobile networks; explicit build-vs-reuse decision (own stack vs iroh vs tsnet/WireGuard vs WebRTC ICE) with reasons.
8. Security & trust: device identity, pairing flows, account-optional model, permissions per peer, unattended access, E2E guarantees, abuse/scam mitigation, audit log, update security.
9. Ease of use: exact first-run flows per platform with step counts and prompts, sharing/guest links, how permission onboarding is made painless, failure diagnostics.
10. Session model: multi-viewer, co-play, gamepads, app launching, resolution follow, reconnection semantics.
11. Observability & measurement: latency overlay, stage timestamps, benchmark harness, network simulation, and the competitive test protocol against Parsec / Moonlight+Sunshine / RustDesk / Apple Screen Sharing.
12. Engineering: languages (Rust core; where Swift/ObjC/C++ are justified), UI toolkit choice, FFI strategy, crate/module boundaries, testing (unit, sim, hardware-in-loop on the owner's Macs), CI, packaging.
13. Roadmap: phased milestones from the current code to "beats all of them", each with exit criteria/metrics; what ships first to users; what is deferred. Assume one developer plus AI agents.
14. Risks, open questions, and a decision log (each key decision with alternatives considered and why rejected).

Rules: cite dossier sections or sources for factual claims; mark UNVERIFIED assumptions; say explicitly where you disagree with a dossier and why. Prefer decisions over option lists.

Write to ~/research-work/proposal-NAME.md where NAME is given in your instructions. Do not modify /home/sunny/projects/sunna.
