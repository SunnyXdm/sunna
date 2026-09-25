# Shared context: Sunna architecture research

## The ask (from the project owner, verbatim intent)
Design the WHOLE architecture of Sunna so that it beats RustDesk, macOS Screen Sharing (High Performance mode), and Moonlight/Sunshine (and forks like Apollo, Wolf, Moonshine); is as easy to use as Parsec; and performs better than Parsec. The owner wants a very, very thorough plan grounded in how the real open-source systems actually work.

## The project
- Repo: /home/sunny/projects/sunna (Rust workspace). Read README.md, research/00..07 (07-v1-plan.md is the current near-term plan), and the code under crates/ and apps/. DO NOT modify anything in that repo.
- Current state: macOS host (CGDisplayStream capture, now zero-copy IOSurface into VideoToolbox H.264), QUIC (quinn) datagrams for media + one reliable control stream, XOR FEC, keyframe-on-loss, 1 s AIMD, winit+softbuffer viewer, CGEventPost input. No auth, no NAT traversal, no audio, no Linux/Windows host.
- Positioning decided so far: quality-first, "the remote machine feels natively installed"; macOS and Linux hosts first-class; gaming is a valid use of the same pipeline. The new ask widens the target to also beat Sunshine/Moonlight (gaming) and Parsec, so the architecture must cover Windows host + gamepads eventually. Prioritization is part of the plan.
- Owner's hardware: M1 MacBook Air, M2 MacBook Air (connected over Tailscale, different networks), a Linux x86 VM. Owner tested Apple Screen Sharing HP mode over Tailscale and found it "not even that good".

## Resources
- Shallow clones in ~/research-repos: Sunshine, Apollo, moonlight-qt, moonlight-common-c, wolf, moonshine (Rust Sunshine-compatible host), rustdesk, rustdesk-server, tailscale, quinn, str0m, DeskPad (CGVirtualDisplay), selkies (WebRTC/GStreamer streaming), FreeRDP, gnome-remote-desktop, SudoVDA (Windows IddCx virtual display), neko (WebRTC).
- You may `git clone --depth 1` more repos into ~/research-repos, and use web search/fetch freely (docs, GitHub issues, blog posts, papers, WWDC sessions).

## Standards
- Cite concrete evidence: file paths with line numbers in the cloned repos, URLs for web sources. Mark anything you could not verify as (UNVERIFIED).
- Prefer exact settings/numbers (encoder params, FEC ratios, timeouts, packet formats) over generalities.
- Be critical: what each system gets wrong, known bugs/complaints (GitHub issues, Reddit), not just what it does.
