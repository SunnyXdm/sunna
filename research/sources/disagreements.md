# Sunna disagreements — adjudication sheet

36 decisions, ordered by architectural impact; IDs remain stable rather than indicating rank. Recommendations are draft defaults; confidence concerns the choice, not achieved performance. Full settings and repeated cross-section IDs are in the merge draft (superseded by [08-architecture-plan.md](../08-architecture-plan.md)).

**Read locators:** A = [proposal-astra.md](proposal-astra.md); F = [proposal-fable.md](proposal-fable.md); C = [proposal-claude.md](proposal-claude.md). `A §7.1` means that file’s uniquely numbered heading 7.1. DA/DF/DC are the corresponding `dossier-*.md` files, retaining source citations. **C — means no position:** §§6–14 were not written; earlier explicit positions are cited only where they exist.

### D-01 · Native connectivity stack (high)

A/C: iroh behind a seam.  
F: build traversal around upstream Quinn.

DF §3b + iroh hooks. Choose iroh; avoid duplicating path-state work. Read A §7.1; F §7.1; C §5.8.

### D-03 · Process privilege, QUIC ownership, viewer isolation (high)

A: unprivileged broker, isolated viewer.  
F: root broker/agent endpoint conflict. C: root stable broker.

DC §3.4. Choose A privilege + A/C stable QUIC ownership. Read A §2.1; F §§2.1,3.1.8; C §§2.1,3.1.10.

### D-02 · Congestion authority and pacing (high)

A: conservative baseline, validated replacement.  
F/C: early app controller, enlarged follower window.

DA §7.1. Choose A safety/pacing; retain C state-machine form. Read A §§5.7–5.8; F §§5.7–5.8; C §§5.4,5.8.

### D-04 · Desktop graphics consistency (high)

A: cumulative tile generations over full video.  
F: masks, blits and intermediate refresh. C: consistency —.

DC §1 ordered RDP. Choose A; gate F passes on matching-base proof. Read A §§6.1–6.2; F §6.2; C §3.1.4.

### D-05 · Linux capture and headless scope (high)

A: portal existing desktop first.  
F/C: private compositor APIs first; earlier KMS/headless scope.

DC §4. Choose A; private caller access and greeter support need tests. Read A §3.6; F §3.2; C §3.2.

### D-06 · Browser transport and trust (med)

A: WebRTC/ICE/TURN. F: direct WebTransport.  
C: relayed WebTransport plus inner Noise.

DA §7.2; C §4.5 NAT limit. Choose A for guest reach; origin remains trusted. Read A §4.5; F §4.4; C §4.5.

### D-09 · Wire format and schema (med)

A: 64-byte header, wide IDs, CBOR.  
F: 28/32 bytes, postcard. C: 20 bytes, postcard.

Epoch/token completeness. Choose A; account for its small-packet overhead. Read A §§5.2–5.4; F §5.2; C §5.1.

### D-10 · FEC block size and repair method (med)

A: small GF256 blocks + RTX. F: larger GF256 blocks.  
C: frame-wide GF65536 + new repair shards.

DA §§1.4,5.3. Choose A bounded wire-time groups; never mix field implementations. Read A §5.5; F §§5.2,5.4–5.5; C §5.2.

### D-11 · Input delivery and stale-control leases (high)

A: journal + cumulative state, short leases.  
F: resend transitions, send deltas once. C: cumulative snapshots.

Lost/late event semantics. Choose A+C; separate motion and transition sequences. Read A §§4.4,5.4; F §§3.1.6,5.2; C §5.1.4.

### D-12 · Reference repair and temporal-layer defaults (high)

A: probe-gated tokens/layers and recovery barrier.  
F: broadly enabled LTR/layers. C: RFI plus narrower probes.

DC §3.2/DA §1.3. Choose A; VT tokens remain opaque, not assumed u64. Read A §§3.3,3.7,5.6; F §§3.1.2,5.6; C §5.3.

### D-14 · Pairing primitive and primary flow (med)

A: full-secret invite; reviewed SPAKE2 fallback.  
F: short CPace code first. C: CPace named, flow —.

DF §6.1. Choose A; preserve QR and code convenience without custom crypto. Read A §8.2; F §8.2; C intro/§2.2 (no §8).

### D-29 · Discovery privacy and relay contract (high)

A: private lookup, authenticated relay envelope.  
F: public discovery, minimal envelope. C: detail —.

DF §3b + return-path/replay rules. Choose A; TCP and UDP remain distinct tiers. Read A §§7.2–7.4; F §§7.3–7.7; C §5.9 (no §7).

### D-07 · Delivery sequence, authentication and effort (med)

A: auth first; early alpha, 18–30 months broad scope.  
F: auth at shipping phase, compressed phases. C: roadmap —.

Current insecure path + hardware gates. Choose A estimates; not a deadline. Read A §13; F §13; C intro/§1.4 (no §13).

### D-08 · Latency definition and superiority targets (high)

A: absolute optical latency + relative gates.  
F/C: added latency; RTT/2 WAN; different win thresholds.

DA §9. Choose A; local-baseline deltas remain secondary, not RTT/2 claims. Read A §§1.2–1.3; F §1.3; C §§1.3–1.4.

### D-32 · Measurement rigor and onboarding counts (high)

A: 10k events/intervals, separate OS actions.  
F: 200; camera fallback. C: sample policy —.

DA §9; 4.17 ms camera steps. Choose A claim gates; short runs for development. Read A §§9.1,11.1,11.5; F §11.5; C §1.3 (no §11).

### D-13 · Queue and resource bounds (high)

A: count/byte/age limits, four frames.  
F: three frames. C: eight frames and larger histories.

DA §7.1 queue arithmetic. Choose A; bound all journals and decoder catch-up. Read A §§2.5,12.4; F §2.5; C §2.6.

### D-26 · Multi-viewer ownership and capacity (med)

A: two renditions/display, session-wide controller.  
F: per-viewer encoders. C: session —; 16 client pads.

DA §§3.3,6.4. Choose A bounded sharing, four pads, controller priority. Read A §§6.6,10.1–10.2; F §§10.1–10.3; C §4.1.3 (no §10).

### D-27 · Reconnect scope, tickets and caches (high)

A: 60 s scoped ticket, reset stale state.  
F: 10-minute token/cache retention. C: details —.

Identity, user and grant boundaries. Choose A; live path migration is distinct. Read A §10.5; F §10.6; C §5.8 (no §10).

### D-19 · Canonical color and YUV range (med)

A: canonical sRGB, limited video, P3 later.  
F: P3 limited. C: P3 full-range.

DC §6.2. Choose A explicit transform; do not mix ranges or mislabel P3. Read A §§3.3,6.4; F §§3.1.1–3.1.2,6.2; C §3.1.3.

### D-25 · Exact refinement, tile size, codec and cache (high)

A: 128 px/Zstd, bounded convergence claim.  
F: 64 px/custom codec/photo skips. C: 64 px/500 ms claim.

Entropy/wire arithmetic. Choose A; smaller tiles/codecs remain corpus experiments. Read A §§1.3,6.1–6.2; F §6.2; C §§1.3,3.1.4.

### D-21 · Virtual-display defaults and geometry leases (high)

A: optional, 30 s grace, 250 ms resize.  
F: main/5 s/300 ms. C: Replace/30 s/300 ms.

DC §3.5. Choose A ownership/fallback; physical panels are not silently rearranged. Read A §§3.2,10.4; F §§3.1.4,10.5; C §3.1.2.

### D-22 · Lock, login, sleep and support promises (high)

A: signed per-state gates, awake user first.  
F: broad lock/greeter claims. C: mixed claims/unknowns.

DC §§3.3–3.4,4.1. Choose A; permission, service and capture readiness differ. Read A §§3.5–3.8; F §§3.1.8–3.1.9,3.3; C §§3.1.10,3.2–3.3.

### D-36 · Windows virtual-device distribution (med)

A: maintained optional adapter/backend. F: ship SudoVDA.  
C: design-derived driver, archived pad fallback.

DA §§1.5,3.2. Choose A; driver signing/license/support remain release gates. Read A §3.8; F §3.3; C §3.3.

### D-16 · Desktop UI toolkit (med)

A: Tauri everywhere.  
F/C: SwiftUI on Mac, Tauri elsewhere.

Existing Apple bridge work. Choose F/C; media remains native and shell-independent. Read A §12.1; F §12.2; C §§2.1,4.1.4.

### D-31 · Native libraries and FFI boundary (med)

A: constrained FFmpeg allowed. F: direct-only.  
C: FFmpeg decode, UniFFI shells.

DA §2.2 + ownership cost. Choose A allowance, C ABI media, optional UI bindings. Read A §§3.7,12.1–12.2; F §§12.1–12.3; C §§2.2,4.2.

### D-17 · VideoToolbox settings (med)

A: max 34/40, no minimum, coarse rate bound.  
F/C: minimum QP and one-frame DataRateLimits.

DC §3.2 property uncertainty. Choose A defaults; measure accepted output behavior. Read A §3.3; F §3.1.2; C §3.1.3.

### D-18 · Non-Apple hardware encoder policy (med)

A: P1/one-frame VBV. F: P3/1.5 frames/four slices.  
C: P1 game, P4 desktop.

DA §§1.3,6.3. Choose A baseline; preserve preset/quality A/B gate. Read A §3.7; F §§3.2.3,3.3; C §3.3.

### D-20 · Audio capture, packet duration and buffering (med)

A: SCK/5 ms. F: taps/10 ms/30 ms buffer.  
C: taps/10 ms desktop, 5 ms game.

DC §3.7. Choose SCK + C durations; 40 ms cap; silence is inconclusive. Read A §§3.4,5.4; F §§3.1.5,2.5; C §3.1.5.

### D-23 · Presentation defaults and decoder overload (med)

A: bounded responsive mode. F: arrival fullscreen game.  
C: two-second auto A/B; keep late references.

DC §6.1. Choose A; keep arrival experiment, not a short automatic winner test. Read A §§4.1–4.3; F §4.1; C §§4.1.1–4.1.2.

### D-24 · Keyboard layout and gesture replay (high)

A/F: physical/text choice. C: switch host layout.  
F/C: broader native-contact ambitions.

DC §§3.6,4.2. Choose explicit mode, preserve rich data, gate actual replay. Read A §4.4; F §§3.1.6,4.1; C §§3.1.7,4.1.3.

### D-30 · Update trust and rollout (med)

A: TUF-style threshold metadata. F: signed manifest.  
C: Sparkle named, trust detail —.

DF §7.4. Choose A trust + F staged rollout; protect independent keys. Read A §8.5; F §8.8; C §2.2 (no §8).

### D-33 · Automatic mode switching (med)

A: damage hysteresis without codec churn.  
F: fullscreen/motion/gamepad rule. C: no position.

Unmeasured thresholds. Choose A + explicit app hints; validate mixed-content corpus. Read A §6.5; F §6.4; C —.

### D-28 · Remote application launching (high)

A: opaque allowlisted app IDs. F: commands/pre-post hooks.  
C: no position.

Process authority boundary. Choose A schema-validated local launch profiles. Read A §10.3; F §10.4; C —.

### D-15 · Guest scope, persistent grants and scam UX (high)

A: short view-only invite; contextual warning.  
F: view+pad/24 h and countdown. C: detail —.

DF §6.5. Choose A; co-play and persistent control require explicit scope. Read A §§8.3–8.4; F §§8.2,8.4–8.6; C §1.2 (no §8).

### D-35 · First-run tasks and permission UI (high)

A: 8 Mac product tasks + OS actions. F: 4+3 total.  
C: ≤8 plus toggles, detailed flow —.

DF §5/TCC. Choose A counts + F live checklist; measure clean signed installs. Read A §§9.1–9.4; F §§9.1–9.2; C §1.3 (no §9).

### D-34 · Commercial promise and hosted obligations (low)

A: quotas/commercial scope open. F: open core/paid teams.  
C: free/self-hostable promise.

Relay egress cost. Keep architecture; owner sets license, pricing and commitments. Read A §13.4; F §§13,14.1; C §1.2.

