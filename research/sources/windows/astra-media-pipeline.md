# Sunna Windows host: media pipeline and latency

Research date: 2026-10-11 UTC. Scope: capture, encode, colour, pacing, audio, measurement, and read-only Valkyris probes. BRIEF.md was read first. No Sunna source was edited, no software was installed on Valkyris, and no settings, services, displays, or sessions were changed. The permitted DxDiag temporary file was deleted in a `finally` block.

**Recommendation:** build a GPU-resident D3D11 pipeline with DXGI Desktop Duplication first, WGC as a selectable fallback, native NVENC/AMF/Intel VPL backends, explicit colour metadata, bounded queues, and client presentation telemetry. Prioritize predictable frame age and loss recovery over a headline encoder number. HDR, good text rendering, and reliable headless operation are plausible product advantages. **A universal latency win over Parsec on every GPU is UNVERIFIED and cannot honestly be promised.** Machines without usable hardware encoding need a deliberately reduced software mode.

Evidence labels used below: **SOURCE** means documented API behavior or inspected implementation; **OBSERVED** means a read-only result from this PC; **DESIGN/TARGET** means a proposed implementation or acceptance budget; **UNVERIFIED** means no adequate measurement or compatibility proof was obtained. Source capabilities are not measurements on Valkyris.

## 1. Evidence baseline and important corrections

Local source versions inspected:

| Checkout | Commit | Commit date |
|---|---|---|
| Sunshine | `05350c052a6251d0e27439c7148a85d3197f1800` | 2026-09-24 |
| Apollo | `adc5c5a0bd80831ce495434bb16aee2cd4175fb8` | 2026-05-21 |
| moonlight-qt | `032529d782242e3833e0b3b147dbbf96e878e3ca` | 2026-09-20 |
| moonlight-common-c | `62e066388f1a1b133e0bee947b9a374311a3354b` | 2026-09-08 |
| quinn reference checkout | `a577f35bd5bb84beb7d3b7ba36081b88fdde4a5d` | 2026-09-23 |

Local file citations refer to these checkouts, not a claim about today's upstream master. Vendor documentation was consulted live; downloaded AMF documents and the NVENC header are retained under [astra-sources](/home/sunny/sunna-windows-research/astra-sources). Their unpinned upstream URLs can change; [manifest.json](/home/sunny/sunna-windows-research/astra-sources/manifest.json) records URLs and SHA-256 hashes. The Quinn reference checkout is API evidence, not an assertion that Sunna uses that exact revision.

Several assumptions from prior plans need correction:

- Desktop Duplication is **not inherently SDR-only**. The old `DuplicateOutput` description is BGRA8; `DuplicateOutput1` supports format negotiation. Sunshine actually requests FP16.
- WGC is **not proved universally one frame slower**, nor universally faster. Windows build, display topology, game present mode, GPU driver, and capture interval matter. No adequate 23H2/24H2/25H2 comparative latency dataset was found.
- A D3D11 texture is not automatically compatible with an encoder on a different adapter. Hybrid graphics requires deliberate adapter selection or a measured transfer path.
- “4:4:4,” “10-bit,” and “HDR” are separate features. Parsec documents experimental 10-bit SDR colour improvement while still documenting HDR as unsupported. Do not advertise 10-bit alone as a unique advantage. [Parsec colour settings](https://support.parsec.app/hc/en-us/articles/32381785123860-Improve-Stream-Quality-and-Color-Accuracy), [HDR limitation](https://support.parsec.app/hc/en-us/articles/32361385772052-Error-Codes-15106-Your-Encoder-Cannot-Capture-Your-Display).
- NVENC frame throughput is not single-frame latency. `nvidia-smi -q` is not a codec/profile capability probe.
- Sunna already has XOR FEC and selective retransmission; the task is to improve their deadlines and codec integration, not add recovery from nothing. [media.rs:1](/home/sunny/projects/sunna/crates/proto/src/media.rs:1).

## 2. Capture on Windows 11

### 2.1 API choice and Windows versions

| Issue | DXGI Desktop Duplication | Windows.Graphics.Capture | Sunna decision |
|---|---|---|---|
| Unit of capture | Active output; D3D11 device must belong to its adapter | Monitor or window; WinRT capture item and D3D11 frame pool | Whole-monitor gaming first; add window capture separately |
| Delivery | `AcquireNextFrame`, frame metadata, present/pointer timestamps | `FrameArrived`, `TryGetNextFrame`, `SystemRelativeTime` | Event-driven capture, timestamp actual content, discard stale pending work |
| SDR | BGRA8; newer API negotiates other formats | BGRA8 frame pool | Explicit source format, stride, size, rotation |
| HDR | `DuplicateOutput1` with FP16 supported in inspected Sunshine | FP16 frame pool is Microsoft's recommended HDR capture path | Preserve FP16 until HDR conversion; never silently clamp to BGRA8 |
| 120/144/240 Hz | No documented universal 60 Hz API ceiling; follows available output updates | High refresh requires checking build/API behavior and update interval | Measure unique captured frames, not callback count |
| Window capture | Not a direct per-window API | Native capability | WGC when the user requests one window |
| Secure desktop | Documented privilege requirements, invalidation on switches | No verified general secure-desktop contract here | Privileged DDA worker for explicitly authorized unattended access |

Sources: [DuplicateOutput](https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_2/nf-dxgi1_2-idxgioutput1-duplicateoutput), [DuplicateOutput1](https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_5/nf-dxgi1_5-idxgioutput5-duplicateoutput1), [Microsoft WGC capture guidance](https://learn.microsoft.com/en-us/windows/uwp/audio-video-camera/screen-capture).

Windows 11 23H2 uses build 22631, 24H2 uses 26100, and 25H2 uses 26200. The `MinUpdateInterval` documentation lists build 26100 support. Use runtime property detection rather than assuming an SDK declaration exists on every OS. On 23H2, preserve a path without that property. On 24H2/25H2, request a suitable interval, record the effective delivered cadence, and expose a fallback if it cannot sustain the negotiated rate. A requested interval cannot manufacture frames absent from the display/game. [Windows release information](https://learn.microsoft.com/en-us/windows/release-health/windows11-release-information), [MinUpdateInterval](https://learn.microsoft.com/en-us/uwp/api/windows.graphics.capture.graphicscapturesession.minupdateinterval?view=winrt-26100).

**SOURCE:** Sunshine uses a free-threaded two-frame WGC pool, BGRA8 or FP16, optional border removal, and `MinUpdateInterval(4ms)`—a 250 Hz request. Its callback replaces and closes an unconsumed previous frame. Apollo instead requests `10,000,000 / (fps * 2)` 100-ns units: twice the negotiated rate. Neither is evidence of actual 250/480 fps delivery or an optimum for Sunna. [Sunshine display_wgc.cpp:103](/home/sunny/research-repos/Sunshine/src/platform/windows/display_wgc.cpp:103), [callback:181](/home/sunny/research-repos/Sunshine/src/platform/windows/display_wgc.cpp:181), [Apollo display_wgc.cpp:141](/home/sunny/research-repos/Apollo/src/platform/windows/display_wgc.cpp:141).

**DESIGN:** use DDA's present timestamp and WGC's `SystemRelativeTime` to measure content age. WinRT TimeSpan is expressed in 100-ns units; do not confuse its representation with arbitrary-frequency raw QPC ticks. Normalize into one host monotonic timebase. Count pointer-only updates separately. A 240 Hz callback stream of repeated 60 Hz images is not 240 Hz capture.

### 2.2 Frame pacing and ownership

Sunshine and Apollo both select DDA first and WGC second when capture mode is automatic. Sunshine's DDA code prefers `IDXGIOutput5::DuplicateOutput1`, synchronizes the capture thread to the current desktop, and retries after transitions. It only uses old `DuplicateOutput` when Output5 is unavailable; it deliberately avoids downgrading after a transient Output1 failure. [Sunshine display_base.cpp:60](/home/sunny/research-repos/Sunshine/src/platform/windows/display_base.cpp:60), [selection:1018](/home/sunny/research-repos/Sunshine/src/platform/windows/display_base.cpp:1018), [Apollo selection:996](/home/sunny/research-repos/Apollo/src/platform/windows/display_base.cpp:996).

Sunshine's pacing groups start from captured image timestamps, calculate rational frame deadlines, and use zero-timeout snapshots when pacing. This is more careful than a `sleep(1000/fps)` loop. Its comments document an unfair D3D11 device lock held during `AcquireNextFrame`: when nothing updates, repeated blocking acquisition can starve encoder initialization for seconds. It yields after idle timeouts. Its conversion/encoder path creates a separate D3D11 device and opens shared capture textures. [display_base.cpp:210](/home/sunny/research-repos/Sunshine/src/platform/windows/display_base.cpp:210), [lock explanation:309](/home/sunny/research-repos/Sunshine/src/platform/windows/display_base.cpp:309), [display_vram.cpp:825](/home/sunny/research-repos/Sunshine/src/platform/windows/display_vram.cpp:825).

**DESIGN:** begin with distinct capture and encode D3D11 devices on the same adapter, a preallocated shared-texture pool, and explicit GPU synchronization. One captured candidate may wait for the encoder; replace it with newer content. Keep sufficient surfaces for operations already in flight, but never treat pool capacity as permission to queue that many frames. Copy the borrowed duplication surface into owned GPU storage before releasing/reusing it; signal ownership only when the relevant copy/convert work is correctly ordered. Benchmark a same-device variant, but do not hold a capture API lock around encoder submission. “Zero-copy” here means **no raw-frame CPU readback**, not literally no GPU copy, colour conversion, or compressed-bitstream CPU access.

### 2.3 Fullscreen, independent flip, MPO, cursor and DPI

Microsoft documents capture of fullscreen DirectX applications and protected-content restrictions. `DuplicateOutput1` can avoid conversion of a fullscreen application's original backbuffer format. That does **not** establish that every Vulkan/DX12 game, MPO plane, HDR overlay, frame-generation path, or independent-flip configuration will work identically. Capture may affect the composition/presentation path itself. Microsoft's flip-model documentation describes independent flip and MPO as hardware/geometry-dependent optimizations. [Desktop duplication overview](https://learn.microsoft.com/en-us/windows-hardware/drivers/display/desktop-duplication-api), [flip model](https://learn.microsoft.com/en-us/windows/win32/direct3ddxgi/dxgi-flip-model).

**UNVERIFIED:** a universal “DDA misses MPO” rule, a universal “WGC captures all generated frames” rule, and any per-build latency delta. Test D3D11, D3D12, Vulkan, real exclusive fullscreen, borderless, and independent-flip modes; include HDR, overlays, alt-tab, resolution switches and frame generation. Verify actual present mode with PresentMon/ETW, not the game's fullscreen label. Do not ship registry changes disabling MPO or HAGS as a prerequisite.

DDA exposes pointer position, visibility and shape; the pointer may already be in the image or may need composition. Handle monochrome, colour and masked-colour shapes, hotspots and rotation; avoid drawing it twice. WGC provides `IsCursorCaptureEnabled`. For gaming, default to correct remote cursor semantics; a local predictive cursor can improve perceived desktop responsiveness but does not reduce a game's input latency and must not be used to inflate click-to-photon results. [Microsoft pointer handling](https://learn.microsoft.com/en-us/windows/win32/direct3ddxgi/desktop-dup-api), [Sunshine cursor handling:display_vram.cpp:300](/home/sunny/research-repos/Sunshine/src/platform/windows/display_vram.cpp:300), [WGC cursor toggle:244](/home/sunny/research-repos/Sunshine/src/platform/windows/display_wgc.cpp:244).

Use per-monitor DPI awareness V2, physical texture dimensions and explicit logical-to-physical coordinate transforms. Account for negative desktop coordinates, rotated displays, DPI changes and display cropping. Microsoft's current duplication sample sets this DPI awareness before capture. WGC resizing requires respecting ContentSize and recreating the pool as necessary. [Microsoft duplication sample](https://github.com/microsoft/Windows-classic-samples/blob/main/Samples/DXGIDesktopDuplication/cpp/DesktopDuplication.cpp), [WGC guidance](https://learn.microsoft.com/en-us/windows/uwp/audio-video-camera/screen-capture).

### 2.4 Hybrid laptops

For an iGPU-driven internal panel with a game rendered on the dGPU, the final desktop output belongs to the iGPU. DDA must use a device on that output adapter. **DESIGN:** prefer its QSV/AMF encoder if it meets the session requirements; that avoids an additional return transfer to the dGPU. Compare against dGPU encode only with a measured transfer path. Match adapter LUIDs, not adapter indices or friendly-name substrings. Standard same-adapter shared-texture code is not a universal cross-adapter solution. CPU staging is an explicit degraded fallback, with measured transfer cost.

WGC can be investigated as a cross-adapter capture option, but “captured into a dGPU device” does not prove that Windows performed no cross-GPU copy. A Sunna virtual display bound to the desired render adapter can offer a different topology; it does not automatically move a running game or remove the laptop's MUX constraints. Vulkan/OpenGL compatibility and power use need real hybrid hardware. DDA's adapter requirement is documented in [DuplicateOutput](https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_2/nf-dxgi1_2-idxgioutput1-duplicateoutput); the rest is a design consequence to benchmark.

### 2.5 UAC, lock, secure desktop and no display

DDA can return access-denied, access-lost, unavailable or disconnected results at desktop/session transitions. Microsoft explicitly identifies LOCAL_SYSTEM as required for secure desktop capture. Merely running an elevated user application is insufficient. A service in Session 0 is also not, by itself, an interactive-session capturer: the worker must be in the appropriate session/desktop. Preserve the user/session boundary, rebuild capture after switches, and discard stale textures. Ordinary user-mode WGC secure-desktop coverage is **UNVERIFIED**; design it to suspend/fail cleanly and use an authorized privileged DDA path where supported. Lock/unlock, UAC and disconnected sessions need separate acceptance cases. [DuplicateOutput contract](https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_2/nf-dxgi1_2-idxgioutput1-duplicateoutput), [Microsoft desktop attachment sample](https://github.com/microsoft/Windows-classic-samples/blob/main/Samples/DXGIDesktopDuplication/cpp/DesktopDuplication.cpp).

No physical monitor does not prevent NVENC/AMF/QSV from encoding supplied textures, but it may leave no active output for DDA and no suitable display for the game. Window capture is not a general headless gaming replacement: minimized, exclusive, launch-resolution and HDR behavior still matter. Stock Windows supplies IddCx infrastructure, **not Sunna's virtual monitor**. A reliable plug-and-play headless host needs a Sunna-installed, signed and maintained virtual display driver, or an explicitly optional physical EDID emulator. Do not depend on Parsec's proprietary driver being installed.

Apollo implements this concretely: when headless/virtual-display mode is requested, or no active display is available, it creates a virtual display with requested dimensions/rate, optionally selects a render adapter, and captures that output. This is driver-backed output creation, not an encoder trick. [Apollo process.cpp:236](/home/sunny/research-repos/Apollo/src/process.cpp:236), [virtual_display.cpp:624](/home/sunny/research-repos/Apollo/src/platform/windows/virtual_display.cpp:624), [bundled SudoVDA INF](/home/sunny/research-repos/Apollo/src_assets/windows/drivers/sudovda/SudoVDA.inf). IddCx gives a driver DirectX desktop surfaces; direct driver-to-encoder handoff is a future optimization, **UNVERIFIED** for Sunna, with substantially more lifecycle responsibility. [Microsoft IDD overview](https://learn.microsoft.com/en-us/windows-hardware/drivers/display/indirect-display-driver-model-overview).

## 3. Hardware encoding from D3D11 textures

### 3.1 Capability policy

Probe **codec + profile + bit depth + chroma + dimensions + frame rate + input format + adapter + driver**, then encode/decode a short validation sequence. API availability or a GPU family name alone is insufficient. Negotiate against the client's decoder and presentation capability too. Vendor runtimes normally arrive with the GPU driver, not every bare Windows installation; Microsoft Basic Display Adapter does not establish hardware encoding support. Sunna must detect absent/outdated runtimes and provide a useful software mode.

| Hardware family | Practical SDR route | HDR 10-bit route | 4:4:4 route / limitations |
|---|---|---|---|
| NVIDIA older NVENC GPUs | H.264; HEVC depends on SKU/generation | HEVC Main10 from supported Pascal-era and newer hardware | H.264 High 4:4:4 Predictive and HEVC RExt on supported hardware; probe older SKUs |
| NVIDIA Turing/Ampere/Ada | H.264 High, HEVC Main; Ada adds AV1 Main | HEVC Main10; Ada AV1 Main 10-bit | H.264 8-bit 4:4:4; HEVC 8/10-bit 4:4:4. Ada AV1 is 4:2:0, not 4:4:4 |
| NVIDIA Blackwell | Above, with additional capabilities | Adds H.264 High10; HEVC/AV1 remain preferable interoperable HDR choices | New 4:2:2 capability is not 4:4:4 AV1; probe exact codec |
| AMD RDNA1 / VCN2 | H.264, HEVC | HEVC Main10 | Published matrix: 4:2:0 |
| AMD RDNA2 / VCN3.x | H.264, HEVC; AV1 decode is not encode | HEVC Main10 | RX 6400/6500-class Navi24 is a major no-encoder exception |
| AMD RDNA3 / VCN4, RDNA4 / VCN5 | H.264, HEVC, AV1 | HEVC Main10, AV1 Main 10-bit | Published VCN matrix remains 4:2:0; APU marketing names do not uniquely identify VCN generation |
| Intel older iGPUs | H.264; HEVC/10-bit vary substantially | Probe model-specific HEVC support | Do not promise 4:4:4 from the Intel CPU brand alone |
| Intel 11th–13th-gen Xe iGPUs | H.264, HEVC | HEVC Main10 | Intel documents HEVC 8/10-bit 4:4:4; these iGPUs do not gain AV1 encode merely because they decode AV1 |
| Intel Arc A-series | H.264, HEVC, AV1 | HEVC Main10, AV1 Main 10-bit | HEVC 8/10-bit 4:4:4; AV1 matrix lists 4:2:0 |
| Newer Intel Xe/Core Ultra/Arc generations | Query runtime, not an inherited A-series table | Generation-specific | Current Xe3 documentation lists AV1 High 4:4:4 8/10-bit; do not extrapolate backward |
| No usable hardware encoder | Software H.264, lower resolution/rate | Not an initial support promise | Software 4:4:4 possible with a suitable codec implementation; performance/decoder support must qualify it |

Sources for the table: [NVIDIA SDK 13.1 capability tables](https://docs.nvidia.com/video-technologies/video-codec-sdk/13.1/nvenc-application-note/index.html), [per-SKU NVIDIA matrix](https://developer.nvidia.com/video-encode-decode-support-matrix), [AMD GPU/APU and VCN matrix](https://github.com/GPUOpen-LibrariesAndSDKs/AMF/wiki/GPU-and-APU-HW-Features-and-Support), [Intel hardware encode tables](https://www.intel.com/content/www/us/en/docs/onevpl/developer-reference-media-intel-hardware/1-1/overview.html), [Xe3 encode documentation](https://edc.intel.com/content/www/us/en/design/platforms/core-processor-series-3-datasheet-volume-1-of-2/002/hardware-accelerated-video-encode/). None guarantees the listed maximum at 4K144 while a game saturates the GPU. Intel CPUs with disabled/absent graphics, including Valkyris's i3-10105F, do not supply an iGPU encoder.

At 3840×2160×144, the pipeline processes about 1.194 billion pixels/s. A 4K resolution check alone is therefore insufficient: validate the codec level, maximum luma sample rate, decoder throughput and texture allocation at the requested frame rate. For example, Sunshine's AMF HEVC selection only uses level 5.2 up to 1,069,547,520 pixels/s, then defers to automatic level selection; 4K144 exceeds that threshold. [video.cpp:1034](/home/sunny/research-repos/Sunshine/src/video.cpp:1034).

### 3.2 NVIDIA implementation

Open an NVENC session with the D3D11 device using `NV_ENC_DEVICE_TYPE_DIRECTX`. Preallocate encoder-compatible textures; register them using `NV_ENC_INPUT_RESOURCE_TYPE_DIRECTX`; map, submit, wait for completion, consume compressed output, unmap, and reuse only after completion. Sunshine's native D3D11 backend is a concrete example. It rejects direct D3D11 10-bit planar 4:4:4 and uses D3D11/CUDA interop for that case. Thus “NVENC supports this profile” does not imply every D3D11 texture layout can feed it. [nvenc_d3d11_native.cpp:14](/home/sunny/research-repos/Sunshine/src/nvenc/nvenc_d3d11_native.cpp:14), [nvenc_d3d11_on_cuda.cpp:90](/home/sunny/research-repos/Sunshine/src/nvenc/nvenc_d3d11_on_cuda.cpp:90).

ARGB/ABGR are accepted NVENC input formats on supporting hardware, allowing driver/GPU colour conversion without CPU conversion. Their names are **word ordered**: an ARGB word is BGRA bytes on little-endian Windows. Do not interchange ARGB and ABGR based on the capture API's spelling. Enumerate supported input formats. RGB input is useful for an initial experiment; explicit GPU conversion to NV12/P010 is preferable when exact matrix, range, chroma siting and HDR behavior must be controlled. VUI tagging alone is not proof of the transform applied internally. Compare known colour patterns through both paths. [NVENC header:393](/home/sunny/sunna-windows-research/astra-sources/nvEncodeAPI.h:393), [upstream header](https://github.com/FFmpeg/nv-codec-headers/blob/master/include/ffnvcodec/nvEncodeAPI.h).

**DESIGN baseline:** P1 + ultra-low-latency tuning, no lookahead, no reordering B frames, single-pass CBR, approximately one-frame VBV, long/infinite GOP with explicit recovery IDRs. P1 is a speed starting point, not the default winner in quality per bit: compare P2/P3 under load and choose the best quality that meets p99 deadlines. Sunshine explicitly sets no lookahead, `frameIntervalP=1`, zero reorder delay, and single-frame-sized VBV when supported. It allows AQ and multipass to be configured. [nvenc_base.cpp:254](/home/sunny/research-repos/Sunshine/src/nvenc/nvenc_base.cpp:254), [initialization:638](/home/sunny/research-repos/Sunshine/src/nvenc/nvenc_base.cpp:638).

Windows WDDM async mode uses registered completion events. Put output waiting on a dedicated worker; async does not mean “queue several old frames.” Resource count and frames actually queued are different. Support synchronous operation where async capability is unavailable. [NVENC programming guide](https://docs.nvidia.com/video-technologies/video-codec-sdk/13.0/nvenc-video-encoder-api-prog-guide/index.html), [header:2261](/home/sunny/sunna-windows-research/astra-sources/nvEncodeAPI.h:2261).

For recovery, keep meaningful frame IDs in `inputTimeStamp`. `NvEncInvalidateRefFrames` excludes corrupt references and their dependents; it may force intra encoding if no usable references remain. Maintain an acknowledged safe reference window and fall back to IDR when history is too old. Sunshine expands a reported loss range through the last encoded frame, invalidates references, and chooses IDR if the window exceeds its DPB. A larger DPB does not require B-frame reordering, but costs memory and decoder compatibility. [header:4240](/home/sunny/sunna-windows-research/astra-sources/nvEncodeAPI.h:4240), [Sunshine RFI:795](/home/sunny/research-repos/Sunshine/src/nvenc/nvenc_base.cpp:795).

Intra refresh can spread refresh cost across frames instead of producing large periodic IDRs. It requires a supported codec configuration and decoder recovery semantics. Sunshine's requested H.264/HEVC mode uses period 300/count 299 and recovery-point signaling; that is implementation evidence, not a recommended Sunna recovery duration. At 60 fps, hundreds of refresh frames are seconds, not instant repair. [nvenc_base.cpp:331](/home/sunny/research-repos/Sunshine/src/nvenc/nvenc_base.cpp:331).

### 3.3 AMD AMF implementation

Use a D3D11-initialized AMF context and wrap compatible native textures with `CreateSurfaceFromDX11Native`; submit `AMFSurface`s without a CPU round trip. Convert BGRA/FP16 to NV12/P010 with Sunna's shader or an explicitly configured AMF converter. Keep wrapped resources alive until AMF releases them. [AMF Context API](https://github.com/GPUOpen-LibrariesAndSDKs/AMF/blob/master/amf/public/include/core/Context.h).

Set usage before initialization: `ULTRA_LOW_LATENCY` for initial game-streaming trials; explicitly disable preanalysis/lookahead/B-picture buffering, set rate control/VBV and measure output cadence. AMF's AVC documentation says transcoding waits for at least three inputs, whereas low-latency usage can emit the first encoded input. A preset name is not sufficient: properties are codec-specific, and defaults can change. Avoid generic millisecond sleeps after `AMF_REPEAT`; use supported `QUERY_TIMEOUT`/bounded output waiting and promptly retry `AMF_INPUT_FULL` without accumulating stale frames. [AMF AVC guide](https://github.com/GPUOpen-LibrariesAndSDKs/AMF/blob/master/amf/doc/AMF_Video_Encode_API.md), [retained AVC guide:98](/home/sunny/sunna-windows-research/astra-sources/amf-avc.md:98).

HEVC and AV1 need their own property namespaces, colour metadata and 10-bit input validation. Current AMF documents expose slice/tile output modes and LTR controls; these are worth testing rather than assuming only NVIDIA offers recovery primitives. Do not equate AMF LTR selection with an already-implemented NVENC-style invalidate API. [HEVC guide](https://github.com/GPUOpen-LibrariesAndSDKs/AMF/blob/master/amf/doc/AMF_Video_Encode_HEVC_API.md), [AV1 guide](https://github.com/GPUOpen-LibrariesAndSDKs/AMF/blob/master/amf/doc/AMF_Video_Encode_AV1_API.md).

Sunshine uses FFmpeg's AMF encoders with D3D11 surfaces, NV12/P010, async depth 1 and latency settings, and advertises no AMF 4:4:4 formats in this checkout. It includes an H.264 fallback from ultra-low to low latency for an AMF compatibility issue. A native Rust binding may simplify control, but replacing FFmpeg alone is **not proven** to save milliseconds. [video.cpp:977](/home/sunny/research-repos/Sunshine/src/video.cpp:977).

### 3.4 Intel VPL/QSV and Media Foundation

Use the VPL dispatcher to enumerate a hardware implementation for the chosen adapter; attach a D3D11 device (`MFX_HANDLE_D3D11_DEVICE`) and video-memory surfaces/allocator. Query actual encode parameters and input layouts. Start `AsyncDepth=1`, `GopRefDist=1`, no lookahead, with supported low-delay rate-control settings and a small buffer. Run `EncodeFrameAsync` with completion handled away from capture; correctly handle device-busy, more-data and surface ownership. VPL is the current API family; legacy Intel runtimes may require a separate compatibility path. [Intel dispatcher/session guide](https://intel.github.io/libvpl/latest/programming_guide/VPL_prg_session.html), [async operation guide](https://intel.github.io/libvpl/latest/programming_guide/VPL_prg_transcoding.html), [handle enum](https://intel.github.io/libvpl/latest/API_ref/VPL_enums.html), [current dispatcher repository](https://github.com/intel/libvpl).

Sunshine provides practical QSV examples: D3D11VA-derived QSV device, NV12/P010 and 4:4:4 formats, `async_depth=1`, `low_delay_brc=1`, low-power encode, codec-specific profiles and fallback when low-power encode is unsupported. Its configuration advertises possible H.264/AV1 4:4:4 paths, then probes them; this is **not proof every Intel GPU supports those paths**. [video.cpp:863](/home/sunny/research-repos/Sunshine/src/video.cpp:863), [capability validation:3177](/home/sunny/research-repos/Sunshine/src/video.cpp:3177).

Media Foundation is a useful secondary route: enumerate hardware encoder MFTs, require D3D11 awareness, provide an `IMFDXGIDeviceManager`, wrap textures in DXGI media buffers, and request `CODECAPI_AVLowLatencyMode`. Probe `ICodecAPI` property support and verify actual sample delay; the flag is not a performance guarantee. It gives less portable control over vendor-specific RFI, LTR and slice readback than native backends. Windows includes MF infrastructure, but the required hardware MFT/profile depends on the installed driver and OS components; do not assume an optional codec extension is present. [DXGI surface buffers](https://learn.microsoft.com/en-us/windows/win32/api/mfapi/nf-mfapi-mfcreatedxgisurfacebuffer), [low-latency property](https://learn.microsoft.com/en-us/windows/win32/medfound/codecapi-avlowlatencymode).

### 3.5 Software fallback and numbers we can substantiate

Retain Sunna's OpenH264 fallback for compatibility. It currently expects CPU BGRA and converts to I420; reuse it as an explicitly degraded path, beginning at 720p30 or a measured 1080p60 on a strong CPU. Reduce resolution/rate under load rather than building a queue. It is not a credible universal 4K120/144 fallback while gaming. Higher-end software codecs or 4:4:4 modes require a separate quality/performance decision and cannot be assumed from OpenH264 support. [openh264_codec.rs:55](/home/sunny/projects/sunna/crates/codec/src/openh264_codec.rs:55).

| Evidence | Number | What it does and does not mean |
|---|---|---|
| NVIDIA SDK 13.1, RTX 4090, 1080p 8-bit 4:2:0, maximum video clock, P1 CBR LL | H.264 910 fps; HEVC 1055 fps; AV1 1090 fps per engine | Throughput, not measured first-frame or click-to-photon latency. Reciprocals are about 1.10/0.95/0.92 ms service intervals, not a latency promise |
| Parsec historical engineering article | Combined encode/decode below 10 ms on some hardware | Historical vendor observation, not a current per-GPU result |
| Valkyris read-only NVML-backed snapshot | One encoder session, average 19 fps, latency 7744 µs = 7.744 ms | A real uncontrolled aggregate snapshot; codec, dimensions, power state and session ownership were not established |
| AMD/Intel matched interactive latency at requested resolutions | **UNVERIFIED** | No sufficiently controlled primary comparison found; measure locally rather than cite anecdotes |

Sources: [NVENC performance table and methodology](https://docs.nvidia.com/video-technologies/video-codec-sdk/13.1/nvenc-application-note/index.html), [Parsec engineering article](https://parsec.app/blog/description-of-parsec-technology-b2738dcc3842), [probe output](/home/sunny/sunna-windows-research/astra-valkyris-probe.txt), [NVML encoder-stat units](https://docs.nvidia.com/deploy/nvml-api/latest/api/group__nvmlDeviceQueries.html).

## 4. Colour correctness and sharp text

**DESIGN SDR:** capture BGRA8; convert with a D3D11 pixel/compute shader into encoder-compatible NV12, or an appropriate 4:4:4 surface. Use explicit coefficients, transfer characteristics, range and chroma siting, and send the same metadata in the codec and Sunna protocol. BT.709 primaries/matrix are the normal SDR video baseline at all requested resolutions; do not switch to BT.601 based on a decoder default. sRGB and BT.709 transfer curves differ: define whether the goal is faithful desktop code values or a colour-managed video transform and validate end to end. Do not blindly apply the sRGB curve twice.

**DESIGN HDR:** capture linear FP16 scRGB, preserve extended values, convert linear BT.709/scRGB primaries to BT.2020, scale using scRGB's 1.0 = 80-nit convention, apply ST.2084 PQ, then convert to BT.2020 nonconstant-luminance Y′CbCr and quantize to P010. Handle gamut/negative values at a deliberate stage; early `saturate()` on scRGB clips HDR. Signal 10-bit, BT.2020 primaries, PQ transfer and BT.2020-NCL matrix, with valid mastering/content-light metadata when available. For SDR clients, do a deliberate tone map with known white level. “PQ-tagged BGRA8” is not HDR. [Microsoft Advanced Color](https://learn.microsoft.com/en-us/windows/win32/direct3darticles/high-dynamic-range), [Sunshine common.hlsl:7](/home/sunny/research-repos/Sunshine/src_assets/windows/assets/shaders/directx/include/common.hlsl:7), [video_colorspace.cpp:90](/home/sunny/research-repos/Sunshine/src/video_colorspace.cpp:90).

Sunshine explicitly avoids R10G10B10A2 capture in this path because its developers observed clamping of DWM scRGB values rather than reliable preconverted PQ; it requests FP16 plus SDR formats. Treat this as implementation experience requiring regression tests, not a universal DXGI prohibition. [display_vram.cpp:2011](/home/sunny/research-repos/Sunshine/src/platform/windows/display_vram.cpp:2011).

Full range and limited range are both valid if encoded and decoded consistently. Limited 8-bit uses luma 16–235 and chroma 16–240; corresponding 10-bit bounds are 64–940 and 64–960. Full-range output must not be stretched again as limited range, and limited-range output must not be rendered as full range. Full range alone does not restore lost 4:2:0 chroma or make lossy encoding exact. Sunshine has explicit range/depth coefficient generation and VUI flags; Sunna's current CPU path is BT.709 limited-range I420. [Sunshine video_colorspace.cpp:130](/home/sunny/research-repos/Sunshine/src/video_colorspace.cpp:130), [nvenc_base.cpp:311](/home/sunny/research-repos/Sunshine/src/nvenc/nvenc_base.cpp:311), [Sunna yuv.rs:1](/home/sunny/projects/sunna/crates/codec/src/yuv.rs:1).

For sharp text, test one-pixel coloured lines, small coloured glyphs and subpixel antialiasing separately from natural images. 4:2:0 removes chroma detail that bitrate cannot fully recover. A 4:4:4 option helps only if the client can decode and render it without a slow software path. Avoid unintended scaling, match negotiated desktop/client dimensions, and use an explicit quality scaler when they differ. For AMD or restricted clients, consider a future dual-stream chroma reconstruction mode, but mark its bitrate cost, synchronization and latency **UNVERIFIED**. Lossy 4:4:4 is still not mathematically exact RGB; promise bounded error and measured appearance, not exact pixels.

The owner's M1/M2 clients should initially negotiate H.264/HEVC hardware decoding. M2's published media engine lists those codecs, not AV1 hardware decoding; AV1 is not a useful default merely because Valkyris can encode it. Probe profile/depth/chroma capabilities via VideoToolbox and check whether hardware decoding was actually selected. Test HDR presentation capability independently from 10-bit decode. [M2 Air specifications](https://support.apple.com/en-us/111867), [M1 Air specifications](https://support.apple.com/en-us/111883). Android and Linux capabilities remain device-specific and unprobed.

## 5. Pacing and a complete latency budget

### 5.1 Define the measurement before adding numbers

For a particular click and its resulting frame:

`click-to-photon = input-device/client delay + upstream network + host input/game scheduling/rendering + capture delivery/age + GPU copy/colour conversion + encoder queue/service + packetization/send pacing + downstream network/serialization + repair/reassembly + decode queue/service + client render/present wait + scanout/pixel response`.

Every term must refer to the **same causal frame**. Do not add independently measured p99 values and call the sum an end-to-end p99. CPU submit completion is not GPU completion; calling Present is not seeing photons. When stages overlap, use a timestamped critical path, not a sum that double-counts the overlap.

At 60/120/144/240 Hz, a frame interval is respectively 16.667/8.333/6.944/4.167 ms. A uniformly phased wait for the next cadence has mean half an interval and worst case almost one interval. That applies to an independent polling gate, game sampling or fixed-refresh presentation—not automatically to every DDA call. Time blocked in `AcquireNextFrame` waiting for an image that does not yet exist is not all added latency of the eventually captured image. Measure source-present-to-acquire age separately.

Interpret “30 ms WAN” as **30 ms RTT** below: approximately 15 ms each way only if symmetric. Click-to-photon includes both directions. If the user means 30 ms one-way, the network contribution doubles. Measure Tailscale's actual direct/relay route, RTT and packet sizes; relay or Wi-Fi delay is not codec time.

### 5.2 Explicit engineering budget, not benchmark results

All numbers in the next two tables are **DESIGN TARGETS / UNVERIFIED**. They illustrate where time can go on a capable GPU/client with no queued old frames. They are not sourced performance claims for any resolution, vendor, Parsec, or Valkyris. Replace them with measured distributions before making comparisons.

| Stage, milliseconds | 1080p | 1440p | 4K | Qualification |
|---|---:|---:|---:|---|
| Source present to acquired frame | 0.5 | 0.5 | 0.5 | Target event-driven delivery; an extra pacing gate adds 0–T |
| GPU copy + colour conversion | 0.15 | 0.30 | 0.60 | Same adapter, no CPU readback |
| Encoder service, including output readiness | 1.50 | 2.50 | 4.00 | Target no backlog; quality preset must fit |
| Packetize + local send work | 0.15 | 0.20 | 0.30 | Excludes link serialization and intentional pacing queue |
| Downstream LAN / WAN | 1 / 15 | 1 / 15 | 1 / 15 | Illustrative one-way values |
| FEC/repair wait on a clean stream | 0 | 0 | 0 | Do not wait for parity once all data arrived |
| Decode service | 1.00 | 1.50 | 2.50 | Hardware path, no reorder queue |
| Client render submission/GPU work | 0.20 | 0.30 | 0.40 | No extra full-frame copy |
| Fixed-refresh present phase | T/2 | T/2 | T/2 | Uniform-phase illustrative mean |
| Center-screen scanout | T/2 | T/2 | T/2 | Simplified progressive scanout; real panel timing differs |
| Pixel response | measured P | measured P | measured P | Not guessed; add to totals |

Derived source-present-to-center-screen values, excluding P, serialization beyond the network allowance, and any additional capture gate:

| Stream / matching client refresh | LAN model | 30-ms-RTT WAN model | Frame period |
|---|---:|---:|---:|
| 1080p60 | 21.17 ms | 35.17 ms | 16.67 ms |
| 1080p120 | 12.83 ms | 26.83 ms | 8.33 ms |
| 1080p144 | 11.44 ms | 25.44 ms | 6.94 ms |
| 1440p60 | 22.97 ms | 36.97 ms | 16.67 ms |
| 1440p120 | 14.63 ms | 28.63 ms | 8.33 ms |
| 1440p144 | 13.24 ms | 27.24 ms | 6.94 ms |
| 4K60 | 25.97 ms | 39.97 ms | 16.67 ms |
| 4K120 | 17.63 ms | 31.63 ms | 8.33 ms |
| 4K144 | 16.24 ms | 30.24 ms | 6.94 ms |

To turn a row into click-to-photon, add measured input delay, upstream one-way network, and host game time. Example only: with 0.5 ms input, 1 ms LAN upstream, 6 ms host game time and 1 ms pixel response, modeled 1080p120 click-to-photon is **21.33 ms**. Substituting 15 ms each way gives **49.33 ms** on the WAN. Host game timing and panel behavior can easily outweigh a sub-millisecond encoder improvement.

Serialization needs a separate check: a 100-Mbit/s stream at 120 fps averages about 104 kB/frame. Its payload takes roughly 0.83 ms over 1 Gbit/s, or 8.33 ms over 100 Mbit/s, before protocol overhead. Add 20% FEC and the 100-Mbit/s path cannot sustain it. A CBR setting limits a time average; it does not prevent individual IDR bursts. Read actual encoded frame sizes and actual wire bytes.

### 5.3 What Parsec and Moonlight/Sunshine actually establish

Parsec's historical engineering article describes DDA, GPU-resident capture/conversion/encode, direct vendor APIs, hardware decode, GPU shaders and frame dropping around synchronized presentation. These are features Sunna must match before claiming an advantage. The article's then-current flip-sequential/OpenGL choices are historical, **not verified internals of 150-103a**. Its older DIY camera experiment reports 16 ms local versus 23 ms streamed, hence 7 ms added latency on that setup. This is neither a universal bound nor a breakdown by stage. [Parsec engineering description](https://parsec.app/blog/description-of-parsec-technology-b2738dcc3842), [DIY latency experiment](https://parsec.app/blog/testing-game-streaming-input-latency-on-parsec-with-diy-instructions-49ae838f45a7), [qualified marketing claim](https://parsec.app/technology).

Parsec's current troubleshooting page calls Network the ping/RTT and describes Encode/Decode as frame handling durations. It also acknowledges latency that those overlay numbers omit, and effects from resolution, refresh, VSync and HAGS. The exact inclusion of capture wait and driver synchronization in current overlay fields is **UNVERIFIED**. Do not infer Parsec's p99, current FEC policy, slice sending, RFI algorithm or stage costs from its closed implementation. [Parsec overlay definitions](https://support.parsec.app/hc/en-us/articles/32381352822804-Troubleshooting-Lag-Latency-and-Quality-Issues).

Sunshine's NVENC path waits for encoded-frame completion and copies the completed bitstream into a vector. Its network thread then sees a complete frame, inserts headers and creates Reed-Solomon FEC blocks. Thus configuring multiple slices is **not evidence that this implementation sends slices before the frame finishes encoding**. [nvenc_base.cpp:711](/home/sunny/research-repos/Sunshine/src/nvenc/nvenc_base.cpp:711), [stream.cpp:1558](/home/sunny/research-repos/Sunshine/src/stream.cpp:1558), [FEC:845](/home/sunny/research-repos/Sunshine/src/stream.cpp:845).

Sunshine's transmitted host processing latency is computed from captured frame timestamp to the network thread before FEC/send. It is broader than pure encoder time and excludes subsequent network work. Moonlight receives that field and has its own depacketization, decode and pacing stages. Its pacer allows bounded queues, uses history to drop frames, and includes a 3 ms timing slack constant; these are not necessarily an unconditional three-frame or 3 ms delay. [Sunshine stream.cpp:1585](/home/sunny/research-repos/Sunshine/src/stream.cpp:1585), [Moonlight VideoDepacketizer.c:488](/home/sunny/research-repos/moonlight-common-c/src/VideoDepacketizer.c:488), [pacer.cpp:16](/home/sunny/research-repos/moonlight-qt/app/streaming/video/ffmpeg-renderers/pacer/pacer.cpp:16), [pacing history:199](/home/sunny/research-repos/moonlight-qt/app/streaming/video/ffmpeg-renderers/pacer/pacer.cpp:199).

### 5.4 Concrete opportunities and their evidence level

| Change | Evidence and expected mechanism | What must be proved |
|---|---|---|
| Remove Sunna CPU colour conversion/readback | Existing NVENC path calls `to_cpu()` and fills I420. Windows GPU surfaces can eliminate this work | GPU timestamps and end-to-end improvement under gaming load |
| Remove Linux viewer GPU→CPU→GPU path | FFmpeg shim calls `av_hwframe_transfer_data`; Rust copies planes into a new CPU vector | Decoder-surface import through the Linux renderer; p99 and power improvement |
| Bound capture/encode queues, preallocate resources | Avoid old-frame latency and initialization allocations; Sunshine documents lock starvation | No periodic spikes after idle, resizing, device loss or game transitions |
| Repair by presentation deadline | Existing Sunna retry = RTT+10 ms, give-up = 2×RTT+30 ms with clamps | At 30 ms RTT that is 40/90 ms: several 144 Hz intervals. RFI/IDR-aware dropping can improve freshness without corrupt references |
| Keep spare GPU rendering headroom | Conversion/copy and game still share scheduling/memory even with a dedicated encoder | Compare host game p99 too; avoid “winning” by degrading game performance |
| Select codec/preset by measured capability | Faster preset or more efficient codec can reduce service/serialization | Quality at equal bitrate and decode cost on each client |
| Subframe encode and early sending | APIs expose relevant features; Sunshine's inspected path is frame-oriented | Early byte availability, actual on-wire overlap, decode acceptance and power cost |
| Better client presentation | Remove missed-refresh and redundant queues | Optical measurement; a submitted frame may still wait for the compositor |

Source anchors: [Sunna nvenc/mod.rs:426](/home/sunny/projects/sunna/crates/codec/src/nvenc/mod.rs:426), [FFmpeg shim.c:184](/home/sunny/projects/sunna/crates/codec/src/ffmpeg/shim.c:184), [CPU plane copies:117](/home/sunny/projects/sunna/crates/codec/src/ffmpeg/mod.rs:117), [recovery deadlines:353](/home/sunny/projects/sunna/crates/proto/src/media.rs:353).

**Subframe precision:** NVIDIA's current header documents `enableSubFrameWrite`, partial slice/tile availability through polling, a capability query, and restrictions on `reportSliceOffsets` with async encoding. HEVC split-frame encoding has incompatible features including subframe mode. Multiple physical NVENC engines/split-frame encode are a different optimization from sending one frame's slices early; the RTX 4060 should not be treated like a dual-encoder 4090. AMD exposes output modes and slice-output caps, too. [NVENC header:1002](/home/sunny/sunna-windows-research/astra-sources/nvEncodeAPI.h:1002), [initialization flags:2261](/home/sunny/sunna-windows-research/astra-sources/nvEncodeAPI.h:2261), [AMF HEVC output:1428](/home/sunny/sunna-windows-research/astra-sources/amf-hevc.md:1428), [NVIDIA SKU matrix](https://developer.nvidia.com/video-encode-decode-support-matrix).

**DESIGN:** investigate early sending after a working baseline. Packet/FEC blocks must not require final frame size; the protocol needs end-of-frame and fragment identity. Whole-frame hardware decoders may still require the final slice, but sending early can overlap encode with network serialization. More slices can reduce compression efficiency; partial-output polling can hurt Valkyris's four-core CPU. Glass-to-glass benefit remains **UNVERIFIED** until all stages support it. Do not promise raster-level interactive response merely from slice encoding.

FEC can avoid an RTT but consumes bitrate and waits for enough symbols. Use small bounded blocks and do not block complete frames waiting for parity. RTX on a 30 ms RTT may already miss several display deadlines; selectively repair only if useful before deadline, otherwise use a known-good reference or request refresh. Do not discard arbitrary encoded P frames and continue decoding dependents. Reference invalidation/LTR needs protocol acknowledgements, IDs, epoch resets and decoder recovery tests. Intra refresh, FEC and IDR each address different failures; retain an IDR escape path.

QUIC datagrams remove transport retransmission/head-of-line delivery requirements, but remain subject to congestion control. Sunna's ordered reassembly can still create an application-level wait behind a lost frame. Keep the datagram buffer small enough to bound age, adapt encoded bitrate before it fills, reserve timely capacity for input/audio, and respect `max_datagram_size()` after path/MTU changes. Quinn's `send_datagram()` may evict older unsent datagrams; `send_datagram_wait()` instead waits for space and favors old work. Neither means a successful call is an on-wire timestamp. Do not enqueue entire seconds of frames or repair traffic into QUIC. Measure Tailscale/encryption/packetization CPU overhead instead of assuming raw UDP would automatically win. [RFC 9221](https://www.rfc-editor.org/rfc/rfc9221.html), [Quinn connection.rs:497](/home/sunny/research-repos/quinn/quinn/src/connection.rs:497), [buffer controls:306](/home/sunny/research-repos/quinn/quinn-proto/src/config/transport.rs:306).

For a **future Windows client**, test flip-discard, a frame-latency waitable swapchain, `IDXGISwapChain2::SetMaximumFrameLatency(1)`, and tearing/VRR when supported. This is not identical to setting device-wide maximum latency to 1. Moonlight's current D3D11 renderer deliberately avoids the latter because it can block SyncInterval-0 Present on DWM; it uses extra buffers without assuming they are all queued for display. [Microsoft swapchain API](https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_3/nf-dxgi1_3-idxgiswapchain2-setmaximumframelatency), [Moonlight d3d11va.cpp:551](/home/sunny/research-repos/moonlight-qt/app/streaming/video/ffmpeg-renderers/d3d11va.cpp:551).

Sunna has no Windows viewer today. The owner's clients need equivalent native work: VideoToolbox/Metal display timing on macOS; MediaCodec surface output and frame scheduling on Android; hardware-surface import and compositor feedback on Linux. VRR can reduce a fixed-refresh phase wait within its working range, but cannot repair jitter beyond that range or guarantee immediate scanout. Test host VRR and client VRR independently; neither is an automatic latency win for capture. Sunshine also contains an NVIDIA/HAGS scheduling-priority workaround: do not globally force real-time GPU priority. [display_base.cpp:678](/home/sunny/research-repos/Sunshine/src/platform/windows/display_base.cpp:678).

## 6. Audio

Use event-driven shared-mode WASAPI loopback (`IMMDeviceEnumerator`, `IAudioClient`, `IAudioCaptureClient`) for desktop audio. It captures a render endpoint's mix, without requiring “Stereo Mix.” Protected streams and exclusive paths have limitations. Handle silence, discontinuities, device invalidation, timestamps and endpoint changes. Sunshine uses shared loopback + event callback, 48 kHz float capture and automatic resampling. [Microsoft loopback](https://learn.microsoft.com/en-us/windows/win32/coreaudio/loopback-recording), [Sunshine audio.cpp:369](/home/sunny/research-repos/Sunshine/src/platform/windows/audio.cpp:369).

There is no honest universal “WASAPI loopback is 1 ms” claim. Microsoft's low-latency article distinguishes engine processing from device periods and APOs; driver buffers may support 10, 5, 3 or 1 ms. Query periods and measure callback-to-audible latency. `IAudioClient3` helps query/control shared-engine periods, but its initialization documentation lists only EVENTCALLBACK as supported stream flags: do not assume adding LOOPBACK provides a portable 1-ms loopback mode. Prototype any render-client-assisted low-period strategy separately. [Low Latency Audio](https://learn.microsoft.com/en-us/windows-hardware/drivers/audio/low-latency-audio), [InitializeSharedAudioStream](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudioclient3-initializesharedaudiostream).

For per-app audio, use process loopback through `ActivateAudioInterfaceAsync` and `AUDIOCLIENT_ACTIVATION_PARAMS`, with include/exclude process-tree mode. Microsoft's sample requires build 20348+, covering the requested Windows 11 versions, and explicitly decouples capture from one physical endpoint. Track launcher/child-process changes. It does not prove that a game will start producing audio when no render endpoint exists at all. That scenario is **UNVERIFIED** and must be tested. [Microsoft application-loopback sample](https://github.com/microsoft/Windows-classic-samples/blob/main/Samples/ApplicationLoopback/README.md).

**DESIGN:** initially use 48 kHz Opus at 5 or 10 ms packets, with independently bounded audio queues. Record the encoder lookahead, packet accumulation, network jitter buffer and client hardware delay; none is zero. A 20 ms packet already consumes a 20 ms accumulation window. Drive drift correction from audio clocks, avoid accumulating synchronization delay, and compare audio/video skew after long sessions. Smaller packets trade latency for overhead and loss sensitivity; use Opus PLC/FEC where it helps. Measure click-to-sound with a physical loopback or microphone, not just packet timestamps.

Mic passthrough is the reverse path: client capture → Opus → network → host virtual microphone. Playing samples through a WASAPI speaker does not make them available as a microphone to an arbitrary game. General compatibility needs an installed virtual capture endpoint, normally paired with a virtual render endpoint and a user-mode bridge. On a headless PC, ordinary endpoint loopback still needs a usable render endpoint; no physical speakers is harmless if a suitable endpoint remains active, but unplugged HDMI/jack devices may disappear.

Sunna therefore needs a clear signed/maintained virtual audio-device story for general headless output and microphone input, or an explicitly documented optional third-party dependency. Do not depend on Steam Streaming Speakers or Valkyris's Voicemeeter installation. Microsoft's SysVAD is reference source, not a production device automatically present on Windows; the sample simulates capture/loopback with a tone and does not supply the required production audio bridge. [SysVAD sample limitations](https://learn.microsoft.com/en-us/samples/microsoft/windows-driver-samples/sysvad-virtual-audio-device-driver-sample/). [Microsoft SysVAD sample](https://github.com/microsoft/Windows-driver-samples/tree/main/audio/sysvad), [Sunshine virtual sink discovery:887](/home/sunny/research-repos/Sunshine/src/platform/windows/audio.cpp:887). A Sunna-owned driver must be installed and serviced as such; ordinary loopback/process-loopback APIs themselves require no custom driver.

## 7. Measuring “beats Parsec” honestly

### 7.1 Instruments and boundaries

Use a photodiode at a fixed client-screen location and a common-clock electrical click trigger, or film the input-trigger LED and client display together at 1000 fps or faster. At 240 fps, frame quantization is 4.17 ms; that cannot substantiate a 1 ms win. Document exposure, rolling shutter, refresh, screen position and luminance threshold. Measure both first changed pixel and a stable-completion criterion so partial updates do not disguise slow completion. Use identical display modes and test both click-to-flash and an actual game's response. Parsec's DIY experiment is a reproducible methodological starting point, not a result to inherit. [DIY method](https://parsec.app/blog/testing-game-streaming-input-latency-on-parsec-with-diy-instructions-49ae838f45a7).

FrameView/PresentMon and NVIDIA PCL/Reflex markers help diagnose **host PC** latency and frame pacing. PCL requires applicable instrumentation and is not optical click-to-photon through a remote client. FrameView can consume PCL ETW events in supported games; these do not include Sunna network transit or the client's panel. Add markers to a Sunna test application, not hypothetical markers inside unmodified games. Use GPUView/WPA/ETW and GPU timestamp queries for scheduling/copy/encode stalls. [NVIDIA PC-latency explanation](https://developer.nvidia.com/blog/understanding-and-measuring-pc-latency/), [FrameView guide](https://images.nvidia.com/content/geforce/technologies/frameview/frameview-1-4-user-guide-web-version.pdf), [PresentMon](https://github.com/GameTechDev/PresentMon).

Every frame should carry epoch/frame ID, source timestamp and host stage durations. Log capture arrival, conversion enqueue/complete, encode submit/complete, first/last packet send, first/last packet receive, FEC/repair completion, decode submit/complete, render submit and actual presentation feedback. Use a visual frame ID in the controlled test pattern to detect repeated/dropped images. Store input event IDs to relate a click to its first resulting frame. Record queue depths, encoded sizes, repair counts, clock/power state and reasons for dropping/reinitializing.

QPC is a within-host clock, not synchronization between machines. Use repeated four-timestamp exchanges to estimate offset/drift with uncertainty, or keep host/client durations separate and use optical measurement for the total. RTT/2 is a symmetry assumption, not a measured one-way delay. Do not subtract capture timestamps from an unrelated client clock without calibration. GPU clocks need CPU/GPU timestamp correlation where used.

### 7.2 Metrics and comparison rules

- Report p50, p95, p99 click-to-photon, confidence intervals, sample count, frame-age distribution, dropped/duplicate frames, deadline misses and longest freeze. Separate startup, steady state and recovery. Aim for 10,000 triggered events across repeated runs for a credible p99; show uncertainty and temporal clustering.
- Compare **same game workload, resolution, actual refresh, chroma, depth, bitrate policy, client renderer, sync mode, network route and hardware state**. Record versions, driver, HAGS, Game Mode, foreground state, power, HDR, scaling and display topology. Randomize A/B order; alternate blocks to avoid thermal bias.
- Report equal encoded-video bitrate **and** equal total wire bitrate including audio, FEC, encryption and retransmission. A stronger FEC stream can otherwise consume more network capacity while appearing to use the same bitrate.
- At equal bitrate, use deterministic motion clips plus real-game sequences, sharp text, gradients, dark scenes, foliage, particles and scrolling. Measure PSNR/SSIM/VMAF where appropriate, but do not treat luma-heavy natural-video metrics as sufficient for coloured text or HDR. Inspect chroma error, temporal pumping, banding and reconstructed RGB/linear-light error; use colour patches/Delta-E with a specified colour transform and HDR-aware measurement. [VMAF implementation and models](https://github.com/Netflix/vmaf).
- Compare latency at matched visual quality too. Plot a bitrate/quality/latency frontier; a 1 ms win achieved by unacceptable compression is not a win on the stated task. Clearly separate Parsec default/free settings from its available 4:4:4/10-bit modes, recording entitlement and compatibility.
- **Proposed acceptance rule:** in a predeclared supported test cell, reduce median click-to-photon by at least max(2 ms, measurement uncertainty) and do not regress p99, with noninferior quality at equal total bitrate. Separately claim quality wins with no latency regression. Publish losses and unsupported cells. This is a proposed gate, not a measured outcome.

### 7.3 Concrete test campaign

**Already completed:** read-only inventory of Valkyris. **Not executed:** game launches, benchmark installation, capture sessions, display/rate/HDR changes, HAGS toggles or comparative streams. Those would exceed the brief's read-only rule. The following is the implementation/owner-run test plan after separate authorization, not actions performed during this research.

1. Preserve the current Parsec version/configuration and acquire an optical baseline with the owner driving the existing setup. Record its native 3420×2148@60 virtual display and actual stream resolution; do not label that 1440p or 4K. Keep Parsec's service running. During later A/B runs, have the owner end the other active stream to avoid simultaneous encoder contention.
2. Add the Sunna instrumented host and controlled click-to-flash/colour-pattern application when installation is authorized. First compare on an identical output topology. Use H.264 8-bit 4:2:0, then HEVC, with both default and tuned settings. Include Moonlight/Sunshine as a diagnostic third comparator, not as a substitute for Parsec.
3. Run 1080p, 1440p and 3840×2160 at 60/120/144 fps **only when an approved output and client can sustain those modes**. Add 240 Hz at 1080p for capture/pacing stress. Budget initial equal-rate trials around 20/40/80 Mbit/s video at 60 fps, then sweep rates and frame rates; these are test points, not recommended sufficient bitrates.
4. M1 and M2 MacBook Airs: use wired Ethernet adapters first, then Wi-Fi; AC power; document panel refresh and scaling. Built-in Air panels are not 120/144 Hz validation displays. Higher-rate decode/capture can still be characterized separately; optical high-refresh claims need a compatible external display/client path. Start with HEVC/H.264 hardware decode, explicitly test 10-bit/4:4:4 capability, and avoid AV1 software decode in the baseline.
5. Android phone: inventory SoC, MediaCodec names, hardware acceleration, supported profiles/levels, refresh modes, HDR formats and sustained thermal behavior. Test touch and a wired mouse separately, because touch sampling adds a different input stage. Linux: record GPU, FFmpeg backend, X11/Wayland/compositor, refresh/VRR and whether decoded frames stay on the GPU. No Android/Linux model assumptions can be made from this brief.
6. LAN: wired low-loss baseline and real Wi-Fi. WAN: actual ~30 ms RTT plus a controlled Linux/router shaper adding 15 ms each way, configured outside Valkyris when authorized. Sweep jitter 0/2/5 ms, random loss 0/0.1/1%, short bursts, reordering and bandwidth contention. Record direct Tailscale versus relay; compare matched routes where possible and disclose when Parsec uses a different route.
7. Game load: controlled desktop, a CPU-bound title, a GPU-bound D3D12 title, D3D11 and Vulkan; fixed camera paths. Run near-full GPU load and a modest-headroom condition. Keep Reflex/game cap settings identical within each comparison; test frame generation separately from actual input-responsive frame rate.
8. Fault matrix: alt-tab, window/fullscreen transition, output resize/rate change, idle/resume, encoder contention, network stalls, UAC/lock/unlock, display disconnect, audio endpoint loss. Perform disruptive cases only with the owner present and recovery access; do not disable secure desktop or anti-cheat protections.
9. Add an AMD RDNA2/3 machine, an Intel Xe/Arc machine, a hybrid laptop and an encoder-less GPU/software case before an “any GPU” launch claim. Test physical HDR/VRR displays and a clean Sunna-only headless installation. Valkyris cannot cover these cells.

## 8. Valkyris read-only results

Raw evidence: [PowerShell/NVIDIA probe](/home/sunny/sunna-windows-research/astra-valkyris-probe.txt), [DxDiag and endpoint probe](/home/sunny/sunna-windows-research/astra-valkyris-dxdiag.txt). Reproducible scripts: [astra-probe.ps1](/home/sunny/sunna-windows-research/astra-probe.ps1), [astra-dxdiag-probe.ps1](/home/sunny/sunna-windows-research/astra-dxdiag-probe.ps1). Both were sent with `powershell -NoProfile -NonInteractive -EncodedCommand` over SSH. The outer SSH startup still printed the known profile-policy warning; queries completed successfully. Probe output includes a UTC timestamp of 2026-10-11T20:44:16Z; `nvidia-smi` displayed the machine's local date/time.

| Item | OBSERVED | Interpretation / limitation |
|---|---|---|
| OS | Windows 11 Pro, 25H2, build 26200.9457 | Only this build was probed; no 23H2/24H2 runtime tests |
| GPU | RTX 4060, Ada, driver 610.88; 8188 MiB total reported memory | NVENC capabilities inferred from vendor matrix, not queried directly |
| NVIDIA physical display | `Display Attached: No`, `Display Active: Disabled` | Confirms headless NVIDIA output |
| WMI video modes | Parsec VDA 3420×2148@60, driver 0.45.0.0; NVIDIA 1024×768@60 | WMI mode data are not a definitive active-path enumeration |
| DxDiag outputs | Both adapters: Current Mode/HDR/Topology/Colour Space unknown | SSH/session context limits display diagnostics; do not conclude HDR is supported or absent |
| WDDM / HAGS | WDDM 3.2; `DriverSupportState:Stable Enabled:True`; `HwSchMode=2` | HAGS enabled, corroborated by DxDiag; no toggle performed |
| MPO | MaxPlanes 0, caps/formats not supported for both entries | Current headless/session report, not proof RTX 4060 lacks physical-output MPO |
| MPO registry override | `OverlayTestMode` property absent | No override found at queried location; absence does not prove MPO active |
| Game Mode | SSH user `valkyris\sunny`; AllowAutoGameMode=1, AutoGameModeEnabled=1 | User preferences enabled; does not prove a particular game is in Game Mode |
| NVENC live stats | One active session; 19 average fps, 7744 µs average latency; encoder utilization 15% | Uncontrolled existing workload; not attributed conclusively to Parsec and not a benchmark |
| GPU power/link snapshot | P8, graphics 210 MHz, video 1185 MHz; PCIe host max Gen3, link width x8 | Low-power/idle-like snapshot; not sustained encode/render capability |
| Processes | Parsec processes, DWM, WUDFHost and NVIDIA Overlay present | Potential shared GPU/encoder workload; no process was stopped |
| Audio | NVIDIA virtual/HDA devices, generic HDA, and VB-Audio Voicemeeter VAIO | Installed audio devices are not stock assumptions |
| Endpoint PnP | Many Voicemeeter inputs/outputs OK; HiBy/headphone/speaker entries also appear as Unknown | PnP status is not an IMMDevice active/default-endpoint or latency query |

`nvidia-smi -q` reports runtime utilization/session statistics, not NVENC supported input formats, codec GUIDs, 4:4:4, maximum dimensions, intra refresh or subframe capabilities. Those require `NvEncGetEncodeGUIDs`, profile/input-format enumeration and `NvEncGetEncodeCaps`. I did not compile or run such a helper on the read-only PC. No runtime capture, MPO-plane interaction, per-codec encode latency, audio-engine period, decoder or optical latency was measured.

An eventual read-only diagnostic executable should query active `QueryDisplayConfig` paths, adapter LUIDs, `IDXGIOutput6::GetDesc1` colour information, overlay/colour-space capability, per-codec encoder caps, and active/default IMMDevice formats/periods from the interactive session. It should print capabilities without creating outputs, changing modes or changing defaults. That executable is a future task, not an installed artifact from this research.

## 9. Recommended pipeline design

Use the Rust `windows`/`windows-core` bindings for Win32, DXGI, D3D11, WinRT capture and WASAPI; keep unsafe COM/resource lifetime code in small modules. Use `libloading` for driver DLL entry points and a small generated C ABI binding or C/C++ bridge for NVENC/AMF/VPL. Pin tested SDK headers and negotiate runtime versions; do not require the full vendor SDK on the user's machine. Existing `bytes`, `quinn`, Opus and tracing/telemetry infrastructure can remain, with added frame-stage and deadline metadata. This is a design choice; exact crate versions should be chosen against the workspace lockfile during implementation. [Microsoft windows-rs](https://github.com/microsoft/windows-rs), [Intel VPL](https://github.com/intel/libvpl).

The normal path should be:

`interactive capture worker → DDA/WGC borrowed D3D11 texture → owned shared GPU surface → explicit colour/scale shader → vendor encoder texture → completion event/output worker → small deadline-aware datagrams + adaptive FEC → client depacketize/decode → native GPU surface → timed presentation`.

Represent a frame with adapter LUID, texture/ownership token, source time, colour descriptor, dimensions, orientation and epoch. Keep capture/convert/encode resource ownership explicit. Preserve only the freshest unsubmitted frame. Keep encoder completion and network work off the capture thread. Reconfigure bitrate without recreating the encoder when supported; use epoch changes and an IDR for incompatible resolution/profile changes. Track device loss and rebuild without leaking old shared handles.

Select H.264 as the compatibility floor, HEVC Main/Main10 for M1/M2 and HDR-capable clients, AV1 only when both endpoints qualify, and a separate 4:4:4 preference gated on actual hardware decoding. Prefer the output adapter's encoder on hybrids unless measured evidence favors a transfer. Add a Sunna-owned virtual display/audio package for complete headless operation; the basic physical-display capture path needs neither driver. Keep the privileged service/session handoff separate from ordinary capture and do not inherit Parsec/Voicemeeter/Steam dependencies.

## 10. Order to build

1. **Measurement and protocol contracts first:** clock/epoch IDs, stage timestamps, queue depths, colour/depth/chroma metadata, deterministic test pattern and optical methodology. Preserve current recovery behavior until its replacement is validated.
2. **DDA + D3D11 + NVENC SDR baseline:** use Valkyris only for owner-authorized runtime work later. GPU conversion, preallocated surfaces, bounded work, no lookahead/B-frame reordering. Complete mode-change and idle/restart handling before aggressive tuning.
3. **AMF, Intel VPL and software qualification:** build the same texture/metadata abstraction and test on actual AMD/Intel/hybrid machines. Start this before expanding the supported-product claim beyond NVIDIA.
4. **WGC and capture compatibility:** high-refresh runtime detection, full-screen/MPO/frame-generation matrix, cursor, DPI, session transitions. Select backends by observed success and freshness, with diagnostics.
5. **Client fast paths and audio:** remove Linux decoder readback, validate macOS/Android native presentation, WASAPI desktop/process loopback, Opus packet timing and A/V drift. Host-only benchmarks are insufficient.
6. **Headless driver integration and HDR/4:4:4:** bring up the signed Sunna-owned output/audio devices and end-to-end colour tests. Keep this a parallel product dependency in the project schedule, even if driver deployment arrives after the first media prototype.
7. **p99 and loss optimization:** deadline-aware repair, NVENC RFI, AMF/Intel LTR evaluation, adaptive bitrate/FEC, measured GPU headroom and power behavior.
8. **Subframe experiments last:** partial encode output, early packet/FEC blocks and compatible client decode. Retain only optimizations that win the optical/quality comparison without disproportionate complexity or CPU load.

## 11. Biggest risks

- **Insufficient hardware coverage:** one headless Ada PC cannot establish universal capture, colour, latency or driver behavior.
- **Client bottlenecks:** the Airs' display timing, unsupported 4:4:4/AV1 hardware profiles, Android thermal limits and Linux readback can erase host gains.
- **Headless lifecycle:** signed virtual display/audio drivers, HDR EDID/mode support, reboot/session transitions and render-adapter selection are real shipped components, not environment assumptions.
- **GPU contention and tails:** HAGS, D3D11 locks, capture reinitialization, near-full VRAM and game load can dominate p99 while averages look good.
- **Colour fidelity:** transfer/range mistakes and hidden tone mapping can defeat the “better-looking” goal even with 10-bit output.
- **Loss semantics:** dropping stale compressed frames without reference recovery creates corruption or longer freezes; a transport improvement must respect codec dependencies.
- **Misleading benchmarks:** overlay-only latency, comparing different output topology, ignoring repair bitrate, or measuring a local cursor can produce a false victory.

## 12. Open questions

1. What are the exact Android and Linux client GPUs, refresh/HDR modes and hardware decode profiles? What external displays are available for 120/144/240 Hz optical tests?
2. Which capture backend has the best source-present-to-frame-age p50/p99 on each requested Windows build, including the owner's driver and physical versus virtual outputs?
3. Does a Sunna virtual display deliver HDR and the requested refresh on each vendor without avoidable cross-adapter transfers? Can direct IddCx surfaces improve latency enough to justify the coupling?
4. Which P1/P2/P3, AMF usage and Intel low-power settings win at **matched quality** under gaming load, and what are the real 4K144 service tails?
5. Which client profiles support hardware 4:4:4 and 10-bit presentation? Is a two-stream text mode worth its complexity where native 4:4:4 is unavailable?
6. What exactly do current Parsec Encode/Decode overlay intervals contain, and which features are available in the owner's entitlement/version? Treat unresolved internals as UNVERIFIED.
7. Do NVIDIA subframe and AMD slice/tile modes emit useful bytes early on the actual driver, and can the selected clients overlap enough decode work to improve photons rather than only packet timestamps?
8. How much can presentation-deadline repair and reference recovery improve Sunna's existing 50–150 ms give-up behavior without increasing visible corruption or bitrate spikes?
9. Can process-loopback capture work acceptably for the target games when no render endpoint exists, or must the virtual speaker always be installed? What are actual engine/Opus/output latencies and drift?
10. What driver/service deployment and recovery design will Sunna own for lock/UAC/headless use on a clean PC, independently of all software installed on Valkyris?
11. What predeclared hardware/workload cells will define the public “beats Parsec” claim, and which measured regressions will block it?
