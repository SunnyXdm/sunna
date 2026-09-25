# VideoToolbox / CGVirtualDisplay probe results

> Placeholder. The week-1 probe battery fills this in; see [08-architecture-plan.md](08-architecture-plan.md), "What to build first", task 6 and §3.3. Run it on each Air.

For each machine (M1 Air, M2 Air), record:
- **Machine:** model, macOS version, Sunna commit.
- **H.264/HEVC profiles and chroma:** what `VTCopySupportedPropertyDictionaryForEncoder` reports, plus the SPS `chroma_format_idc` actually produced (does 4:4:4 work in hardware?).
- **Low-latency properties:** LTR token semantics, `BaseLayerFrameRateFraction`, `DataRateLimits`, `MaxAllowedFrameQP`/`MinAllowedFrameQP`. For each, record whether the setter is accepted *and* whether the output behaves as requested.
- **Virtual display:** does `CGVirtualDisplay` + ScreenCaptureKit deliver frames on this macOS version? How does it behave with the lid closed?

| Probe | M1 Air | M2 Air |
|---|---|---|
| H.264 High, hardware | | |
| HEVC Main, hardware | | |
| HEVC 4:4:4 (chroma_format_idc = 3) | | |
| LTR tokens | | |
| DataRateLimits honoured | | |
| Max/Min QP honoured | | |
| CGVirtualDisplay + SCK frames | | |
| Clamshell (lid closed) | | |
