# 01 — RDP Graphics Pipeline (RDPGFX) via FreeRDP / xrdp / specs

Scope: how RDP GFX achieves sharp text + low bandwidth; what Sunna should steal.
Citations: FILE:LINE relative to /home/sunny/research-repos/<repo>; specs as [MS-XXX] §x.y, p.NN (page markers in ../specs/*.txt).

## 1. RDPGFX PDU model

Snapshot: FreeRDP @ aae2c8a (2026-09-25). Spec: [MS-RDPEGFX] v20260511.

**Transport/framing.** GFX runs on DVC `"Microsoft::Windows::RDS::Graphics"`, which the spec says is designed for a *non-lossy* channel ([MS-RDPEGFX] §2.1, p.14). All S->C messages are wrapped in `RDP_SEGMENTED_DATA` (descriptor SINGLE/MULTIPART, segments of `RDP8_BULK_ENCODED_DATA`) compressed with RDP 8.0 bulk compression = LZ77 + static Huffman ("ZGFX"; FreeRDP `libfreerdp/codec/zgfx.c`) ([MS-RDPEGFX] §2.2.5.1 p.72, §3.1.9.1 p.79). C->S messages are unwrapped. So *everything*, including already-entropy-coded bitmaps, goes through a generic LZ layer — which mostly helps uncompressed/planar/SolidFill/command streams. Spec also says continuous network characteristics detection SHOULD be enabled ([MS-RDPBCGR] §1.3.9, 2.2.14).

**Header** `RDPGFX_HEADER {u16 cmdId; u16 flags=0; u32 pduLength (incl. 8-byte header)}` ([MS-RDPEGFX] §2.2.1.5, p.16-18). cmdIds:
| id | PDU | dir | key fields |
|---|---|---|---|
|0x01|WIRE_TO_SURFACE_1|S->C|surfaceId u16, codecId u16, pixelFormat u8 (XRGB_8888=0x20 / ARGB_8888=0x21), destRect RECT16 (bounding rect for AVC*), bitmapDataLength u32, bitmapData (§2.2.2.1 p.19-21)|
|0x02|WIRE_TO_SURFACE_2|S->C|surfaceId, codecId (only CAPROGRESSIVE 0x0009), **codecContextId u32** (persistent progressive context), pixelFormat, bitmapData (§2.2.2.2 p.21)|
|0x03|DELETE_ENCODING_CONTEXT|S->C|surfaceId, codecContextId — frees progressive tile state (§2.2.2.3 p.22)|
|0x04|SOLIDFILL|S->C|surfaceId, fillPixel COLOR32, fillRectCount u16, fillRects[] (§2.2.2.4 p.22)|
|0x05|SURFACE_TO_SURFACE|S->C|surfaceIdSrc, surfaceIdDest, rectSrc, destPtsCount, destPts[] — blit within/between surfaces = scroll / window move for ~20 bytes (§2.2.2.5 p.23)|
|0x06|SURFACE_TO_CACHE|S->C|surfaceId, **cacheKey u64**, cacheSlot u16, rectSrc (§2.2.2.6 p.24)|
|0x07|CACHE_TO_SURFACE|S->C|cacheSlot, surfaceId, destPtsCount, destPts[] (§2.2.2.7 p.24)|
|0x08|EVICT_CACHE_ENTRY|S->C|cacheSlot (§2.2.2.8 p.25)|
|0x09/0x0A|CREATE/DELETE_SURFACE|S->C|surfaceId, width, height, pixelFormat (§2.2.2.9-10 p.25-26)|
|0x0B|START_FRAME|S->C|timestamp (packed ms:10/s:6/min:6/h:10 bits UTC), frameId u32 (§2.2.2.11 p.26)|
|0x0C|END_FRAME|S->C|frameId (§2.2.2.12 p.27)|
|0x0D|FRAME_ACKNOWLEDGE|C->S|**queueDepth u32**, frameId, totalFramesDecoded (§2.2.2.13 p.27-28)|
|0x0E|RESET_GRAPHICS|S->C|width,height (max 32766), monitorCount (<=16), TS_MONITOR_DEF[]; PDU is always exactly 340 bytes (§2.2.2.14 p.28)|
|0x0F|MAP_SURFACE_TO_OUTPUT|S->C|surfaceId, outputOriginX/Y u32 (§2.2.2.15 p.29)|
|0x10|CACHE_IMPORT_OFFER|C->S|cacheEntriesCount (<5462) × {cacheKey u64, bitmapLength u32} (§2.2.2.16 p.30)|
|0x11|CACHE_IMPORT_REPLY|S->C|importedEntriesCount, cacheSlots[] u16 (first N offered entries imported) (§2.2.2.17 p.31)|
|0x12/0x13|CAPS_ADVERTISE/CONFIRM|both|array of RDPGFX_CAPSET{version,capsDataLength,capsData}; server picks one (§2.2.2.18-19 p.32)|
|0x15|MAP_SURFACE_TO_WINDOW|S->C|surfaceId, windowId u64, mappedWidth/Height (RAIL/RemoteApp) (§2.2.2.20)|
|0x16|QOE_FRAME_ACKNOWLEDGE|C->S|frameId, timestamp (client ms when START_FRAME decode began), timeDiffSE u16 (Start->End decode ms), timeDiffEDR u16 (End->render done ms); informational only; only with caps 10/10.2+ (§2.2.2.21 p.33-34)|
|0x17/0x18|MAP_SURFACE_TO_SCALED_OUTPUT/WINDOW|S->C|as 0x0F/0x15 + targetWidth/targetHeight — client scales the surface (server renders at lower res, e.g. HiDPI) (§2.2.2.22-23 p.34-35)|

**Surfaces model.** Server creates N offscreen surfaces (client-side bitmaps), maps them onto the "Graphics Output Buffer" (one per session desktop) or to RAIL windows. All codec output lands in a surface; SolidFill/SurfaceToSurface/CacheToSurface are pure client-side compositing ops. Frames = the atomic present unit: client presents the output buffer on END_FRAME (FreeRDP: gdi `EndFrame` -> `UpdateSurfaces`, see §5).

**Frame acks = flow control.** Server keeps "Unacknowledged Frames" list ([MS-RDPEGFX] §3.2.1.2 p.84). On ack it removes frameId; if `queueDepth` in 1..0xFFFFFFFE the server "SHOULD use this value to determine how far the client is lagging ... and attempt to throttle the graphics frame rate accordingly"; `SUSPEND_FRAME_ACKNOWLEDGEMENT=0xFFFFFFFF` → server clears the list and "MUST NOT wait or block on unacknowledged frames" ([MS-RDPEGFX] §3.2.5.13 p.86). `QUEUE_DEPTH_UNAVAILABLE=0`. Client acks the most recently processed frame (§3.3.5.13 p.99). FreeRDP client: always sends `queueDepth = QUEUE_DEPTH_UNAVAILABLE` (never a real byte count), or SUSPEND once on first frame if `FreeRDP_GfxSuspendFrameAck` (channels/rdpgfx/client/rdpgfx_main.c:1356-1379); QoE ack optional via `FreeRDP_GfxSendQoeAck` for caps >=10 (rdpgfx_main.c:1390-1410). Windows server behaviour: widely reported to cap in-flight unacked frames (UNVERIFIED exact number; commonly cited ~ a few frames) — i.e. a window-based, decode-speed-aware limiter.

**Caps versions & flags** ([MS-RDPEGFX] §2.2.1.6 p.18-19, §2.2.3.x p.36-43; FreeRDP include/freerdp/channels/rdpgfx.h:108-163):
| capset | version | flags allowed / meaning |
|---|---|---|
|8.0|0x00080004|THINCLIENT 0x1 (16 MB cache AND must use plain RemoteFX instead of Progressive), SMALL_CACHE 0x2 (16 MB). Neither → 100 MB cache|
|8.1|0x00080105|+ AVC420_ENABLED 0x10 (H.264 4:2:0 allowed in WTS1). Valid combos: THIN / SMALL / SMALL\|AVC420 / SMALL\|AVC420\|THIN|
|10.0|0x000A0002|SMALL_CACHE, AVC_DISABLED 0x20. **If AVC_DISABLED not set, client MUST support AVC444** (§2.2.3.3 p.38)|
|10.1|0x000A0100|16 reserved bytes, no flags|
|10.2|0x000A0200|same as 10.0 (enables QoE ack)|
|10.3|0x000A0301|AVC_DISABLED, AVC_THINCLIENT 0x40 ("client prefers AVC444"; must not be combined with AVC_DISABLED); implies 16 MB cache|
|10.4|0x000A0400|SMALL_CACHE, AVC_DISABLED, AVC_THINCLIENT; AVC enabled now also means client can take **AVC420 in the same frame as other codecs** (mixed-codec frames) (§2.2.3.7 p.40-41)|
|10.5|0x000A0502|same flags|
|10.6|0x000A0600 (FreeRDP also sends errata value 0x000A0601 "106_ERR")|same flags (rdpgfx.h:119-125)|
|10.7|0x000A0701|+ SCALEDMAP_DISABLE 0x80 (no MAP_SURFACE_TO_SCALED_*)|
|11.1/11.2/11.3|0x000B0101/0x000B0200/0x000B0300|undocumented, "used by Azure"; SCP_DISABLE 0x100 (rdpgfx.h:128-153), UNVERIFIED semantics|
|FRDP_1|0x00010000|FreeRDP-private: AV1_I444_SUPPORTED 0x10000000, AV1_I444_DISABLED 0x20000000; codecId RDPGFX_CODECID_AV1=0x0001 (rdpgfx.h:108-195). Only with WITH_GFX_AV1|
FreeRDP client advertises all of them (rdpgfx_main.c:267-556); for ≥10.3 it sets AVC_THINCLIENT whenever AVC is enabled (rdpgfx_main.c:424-425) and drops SMALL_CACHE for 10.3 (435). Server re-receiving CapsAdvertise after a ≥10.3 confirm MUST reset protocol state (§3.2.5.18 p.87) — used for client-driven codec renegotiation.

**Bitmap cache limits.** Slots are 1-based, variable-sized; total bytes ≤100 MB (≤25,600 slots) or ≤16 MB (≤4,096 slots) under THINCLIENT/SMALL_CACHE/10.3 ([MS-RDPEGFX] §3.3.1.4 p.95). FreeRDP: `MaxCacheSlots = GfxSmallCache ? 4096 : 25600` (rdpgfx_main.c:2626-2627). Server owns the slot allocation (Bitmap Cache Map, §3.2.1.1 p.84); cacheKey is a 64-bit content hash so a *persistent* disk cache can be offered back on reconnect (CACHE_IMPORT_OFFER, <5462 entries; RDPGFX_CACHE_ENTRY_MAX_COUNT=5462 at rdpgfx.h:363; FreeRDP save/load at rdpgfx_main.c:841-1070).

**Codec IDs** (§2.2.2.1 p.20; rdpgfx.h:191-205): UNCOMPRESSED 0x0000, (AV1 0x0001 FreeRDP-private), CAVIDEO (RemoteFX) 0x0003, CLEARCODEC 0x0008, CAPROGRESSIVE 0x0009 (WTS2 only), PLANAR 0x000A, AVC420 0x000B, ALPHA 0x000C, CAPROGRESSIVE_V2 0x000D (FreeRDP define; not in spec), AVC444 0x000E, AVC444V2 0x000F. **Interleaved is NOT a GFX codec** — it is the legacy [MS-RDPBCGR] bitmap-update/pointer RLE (FreeRDP interleaved.c used by legacy BitmapUpdate and pointer decode).

**Cursor is out of band.** GFX carries no cursor. Pointer shapes go over the core RDP channel as Pointer Update PDUs ([MS-RDPBCGR] §2.2.9.1.1.4: TS_COLORPOINTERATTRIBUTE / TS_POINTERATTRIBUTE / TS_LARGEPOINTERATTRIBUTE up to 384×384 / cached pointer index / system pointer / TS_POINTERPOSATTRIBUTE) and are drawn locally by the client — zero cursor latency, cursor never enters the video (FreeRDP client: libfreerdp/core/update.c pointer handlers, UNVERIFIED line numbers).

## 2. Codecs — exact mechanisms

All paths below relative to /home/sunny/research-repos/FreeRDP unless noted.

### 2.1 Uncompressed (0x0000)
Raw XRGB/ARGB rows, left→right, top→bottom ([MS-RDPEGFX] §2.2.2.1 p.20). Only the ZGFX bulk LZ layer compresses it. Useful for tiny rects / cursor-sized updates.

### 2.2 Planar (0x000A) — lossless-ish, the "screen text" codec of RDP 6/8
Defined in [MS-RDPEGDI] §2.2.2.5.1/§3.1.9 (not in local spec set). `formatHeader` byte: bits 0-2 **CLL** (Color Loss Level), bit3 **CS** (chroma subsampling), bit4 **RLE**, bit5 **NA** (no alpha) (libfreerdp/codec/planar.c:58-67; include/freerdp/codec/planar.h:32-35).
- CLL=0 → planes are A,R,G,B (lossless). CLL=1..7 → AYCoCg; Co/Cg are right-shifted by CLL (lossy chroma quantization) and with CS=1 Co/Cg planes are 2×2 subsampled (CS requires YCoCg: planar.c:790-793; YCoCg→RGB decode planar.c:1051,1162).
- Per plane: optional scan-line **delta** (row y − row y−1, mapped to sign-magnitude byte: `s2c = d>=0 ? d<<1 : ((-d)<<1)-1`) then byte **RLE** with control byte `[0-3] nRunLength, [4-7] cRawBytes`, nRunLength 1/2 meaning run = cRawBytes+16/+32 (planar.c:99-110,196-219; delta encode at planar.c `freerdp_bitmap_planar_delta_encode_plane`).
- Net: excellent on flat UI/text (vertical coherence → zeros → long runs), exact when CLL=0. No entropy coder beyond RLE (ZGFX mops up).

### 2.3 Interleaved — NOT a GFX codec
Legacy RDP 5 bitmap RLE ([MS-RDPBCGR] §2.2.9.1.1.3.1.2.4) used for legacy BitmapUpdate and pointer shapes (libfreerdp/codec/interleaved.c). Not in the RDPGFX codecId table.

### 2.4 ClearCodec (0x0008) — the dedicated text/UI codec (decode-only in FreeRDP)
Stream: `flags u8 | seqNumber u8 | glyphIndex u16 (opt) | compositePayload` ([MS-RDPEGFX] §2.2.4.1 p.43-44). Flags: `GLYPH_INDEX 0x01` (only for bitmaps ≤1024 px area), `GLYPH_HIT 0x02` (payload absent, reuse Glyph Storage[glyphIndex]), `CACHE_RESET 0x04` (reset both V-bar cursors). seqNumber increments per ClearCodec message mod 256 (clear.c:1127-1138 checks). glyphIndex 0..3999.
Composite payload = 3 layers, each optional, decoded in order and overpainting ([MS-RDPEGFX] §2.2.4.1.1 p.44-45, §3.3.8.1 p.101-102):
1. **Residual layer**: whole-bitmap BGR runs (`CLEARCODEC_RGB_RUN_SEGMENT`: 3-byte pixel + runLengthFactor1 u8 / 0xFF→u16 / 0xFFFF→u32) — paints backgrounds/gradients-free fills (p.45-46).
2. **Bands layer**: horizontal bands (max height **52 px**) with `xStart,xEnd,yStart,yEnd` (inclusive) + background BGR; one **V-Bar** (1-px-wide column of band height) per x. Each V-Bar is one of: `VBAR_CACHE_HIT` (15-bit index, top bit 1) into **V-Bar Storage (32,768 entries)**; `SHORT_VBAR_CACHE_HIT` (14-bit index, top bits 01, + yOn u8) into **Short V-Bar Storage (16,384)**; `SHORT_VBAR_CACHE_MISS` (yOn 8 bits, yOff 6 bits, top bits 00) + raw BGR pixels for rows [yOn,yOff), rest of column = band bg. Each decoded V-Bar is pushed into V-Bar storage at the ring cursor (§2.2.4.1.1.2.x p.46-50; §3.3.1.10-13 p.96; clear.c:38-39,606-880). This is the **text trick**: anti-aliased glyph stems produce a small vocabulary of pixel columns; repeated letters hit the column caches, so text costs ~2 bytes/column after warm-up, and it is lossless.
3. **Subcodec layer**: rects each with `xStart,yStart,width,height,subCodecId`: 0 = raw BGR, 1 = **NSCodec** ([MS-RDPNSC]: AYCoCg, ColorLossLevel 1..7, ChromaSubsamplingLevel, per-plane RLE — [MS-RDPNSC] §2.2.x p.~9-10, lines 355-411), 2 = **RLEX** (palette ≤127 entries, segments of stopIndex/suiteDepth "run then suite" encoding — suites encode monotonically increasing palette indices, i.e. AA text ramps) (§2.2.4.1.1.3 p.50-53; clear.c:455-560).
- **Glyph cache**: whole small bitmap (≤1024 px², e.g. a rendered word/glyph run) stored at glyphIndex in Decompressor Glyph Storage (4,000 slots, §3.3.1.9 p.96) and replayed with GLYPH_HIT → 3-4 byte message (clear.c:929-1050).
- FreeRDP has **no ClearCodec encoder** (`clear_compress` = "TODO: not implemented!", clear.c:1273-1281); Windows servers use it for text-heavy regions (see §3).

### 2.5 RemoteFX / CAVIDEO (0x0003) ([MS-RDPRFX])
64×64 tiles anchored at screen (0,0) (§3.1.8.1.1 p.33); optional tile differencing = send only changed tiles, not residuals (p.33); RGB→YCbCr (Y level-shifted by −128) (§3.1.8.1.3 p.33); 3-level 2-D **LeGall 5/3 DWT** with mirrored boundaries → 10 subbands (§3.1.8.1.4 p.34); scalar quant per subband with 4-bit factors (scale = 2^(f−1)... per Fig.8; FreeRDP default `{6,6,6,6,7,7,8,8,8,9}` for LL3,LH3,HL3,HH3,LH2,HL2,HH2,LH1,HL1,HH1 order per TS_RFX_CODEC_QUANT, rfx.c:71); linearization HL1..HH3,LL3 with LL3 DPCM (p.36); **RLGR1 or RLGR3** entropy (adaptive run-length / Golomb-Rice; CLW_ENTROPY_RLGR1=0x01, RLGR3=0x04) (§3.1.8.1.7 p.37). All intra, no motion. Never lossless: TS_RFX_CODEC_QUANT fields "MUST have a value in the range of 6 to 15" ([MS-RDPRFX] §2.2.2.1.5, line 803), so even the finest quant discards bits; RFX's sharpness ceiling is fixed.

### 2.6 RemoteFX Progressive (CAPROGRESSIVE 0x0009, via WIRE_TO_SURFACE_2 + codecContextId)
Adds on top of RemoteFX ([MS-RDPEGFX] §3.1.8.1 p.75-79, §3.2.8.1 p.88-94):
- **Reduce-Extrapolate DWT** (region flag `RFX_DWT_REDUCE_EXTRAPOLATE 0x01`): extrapolate a 65th sample → level-1 bands 33 low / 31 high, then 17/16, 9/8; removes visible 64-px tile seams of original RFX (§3.2.8.1.2.2 p.89-90).
- **Sub-band diffing**: send either DwtQ (original) or `Diff = DwtQ − Ref` in the coefficient domain, where Ref = what the *decoder* currently holds (not previous frame) (§3.1.8.1.2/§3.1.8.1.4 p.76-77). Tile flag `RFX_TILE_DIFFERENCE 0x01`.
- **Extra (progressive) quantization**: `ProgQ = SB >> BitPos(quality, component, band, chunk)`; BitPos=15 at 0%, 0 at 100%; LL3 rounds toward −∞ (§3.1.8.1.3 p.76-77). Tables `RFX_PROGRESSIVE_CODEC_QUANT {quality u8, yQuantValues, cbQuantValues, crQuantValues}` = per-band bit shifts per "progressive quality" level; region carries `numQuant`, `numProgQuant`, tiles index them via `progressiveQuality` (0xFF = full quality, 100%) (§2.2.4.2.1.5 p.56-60).
- **Wire**: blocks `RFX_PROGRESSIVE_TILE_SIMPLE` (0xCCC5, non-progressive, RLGR1), `TILE_FIRST` (0xCCC6: flags, quantIdxY/Cb/Cr, xIdx, yIdx, progressiveQuality, y/cb/cr/tail lengths) and `TILE_UPGRADE` (0xCCC7: quantIdx*, progressiveQuality, ySrlLen, yRawLen, cbSrlLen, cbRawLen, crSrlLen, crRawLen + data) (§2.2.4.2.1.5.3-5 p.61-66).
- **First pass**: RLGR1 over all 10 bands of ProgQ (K=1, KR=1 start). DTS = ProgQ·PQF, DRS = SB − DTS (§3.2.8.1.5.1 p.93).
- **Upgrade passes**: `UpgradeQ = DRS / PQF(TargetC)`. For each coefficient: if the decoder's accumulated value (DAS) is 0 → coefficient goes into the **SRL** stream (zero-run coding with KP adaptation 8→±, then unary magnitude + sign — magnitude bounded by nBits); if DAS≠0 → only the **raw refinement bits** (sign already known) go into the RAW stream (§3.2.8.1.5.2 p.93-94; SRL §3.1.8.1.5 p.77-78). This is JPEG2000/SPIHT-style bitplane refinement.
- **Concretely on the wire**: frame N sends TILE_FIRST at e.g. 25% quality for changed tiles (small, blurry); when the region stays static, frames N+1, N+2… carry TILE_UPGRADE for the same tiles to 50%, 75%, 100% (spec worked example: "Encode Frame #1 at 25%/50%, Frame #2 at 25%/50%/100%", §4.1.2 p.121-128). Full = progressiveQuality 0xFF, i.e. final quant = the base RemoteFX quant (still lossy unless base quant is minimal).
- FreeRDP server side emits only `TILE_SIMPLE` via RLGR1 (`progressive_compress` → `rfx_encode_message` → `rfx_write_message_progressive_simple`, progressive.c:2520-2650, mode=RLGR1 at progressive.c:2624) — **no actual progressive refinement in FreeRDP's encoder**. Decoder supports all (progressive.c:957-1640).

### 2.7 AVC420 (0x000B)
`RFX_AVC420_BITMAP_STREAM = RFX_AVC420_METABLOCK + Annex-B H.264 (High profile allowed)`. Bitstream dims 16-aligned; decoded frame is **cropped by regionRects mask** (§2.2.4.4 p.67-68). Metablock: `numRegionRects u32, regionRects[] RECT16, quantQualityVals[] {qpVal u8 = qp:6 | r:1 | p:1, qualityVal u8 0..100}` (§2.2.4.4.1-2 p.68-69). **Metablock is informational** — "SHOULD NOT be used by the client when decoding" (p.68); its real role: (a) region mask = only these rects are copied to the surface (so the H.264 frame is a full-surface-size canvas but only damaged rects are applied — unchanged areas are protected from codec noise / drift), (b) per-region QP + `p` flag lets the server signal "this region is a progressive (low-QP refresh) pass". Color: **full-range BT.709** ([MS-RDPEGFX] §3.3.8.3.1 p.106). FreeRDP encoder: damage detection per **64×64 tile on YUV** buffers vs previous frame (`detect_changes`/`diff_tile`, libfreerdp/codec/h264.c:244-322), uniform `qp = h264->QP`, `qualityVal = 100 − QP` for all rects (h264.c:209-243).

### 2.8 AVC444 (0x000E) and AVC444v2 (0x000F) — 4:4:4 via two 4:2:0 pictures
`RFX_AVC444[V2]_BITMAP_STREAM = avc420EncodedBitstreamInfo u32 (cbAvc420EncodedBitstream1:30 | LC:2) + stream1 + [stream2]` (§2.2.4.5-6 p.69-72). **LC**: 0 = stream1 YUV420 (main/luma view) + stream2 Chroma420 (aux view); 1 = main only (aux "sent in subsequent frames if required"); 2 = stream1 is aux only, "MUST be combined with the decoded AVC stream from previous frames"; 3 invalid. **Both views MUST be encoded by the same encoder and decoded by a single decoder as one stream** (p.69, p.71) — i.e. main and aux pictures are interleaved in ONE H.264 sequence (FreeRDP does exactly this: one `subsystem->Compress` for both, h264.c:487-508; one decoder `log_decompress` for both, h264.c:598-628).
Packing (reconstructed from FreeRDP combine code, libfreerdp/primitives/prim_YUV.c):
- **Main view (both v1/v2)**: Y = Y444 (B1); U,V = **2×2 box average** of U444/V444 (B2,B3) (encoder avg: prim_YUV.c ~1736-1741 `Uavg = (U1e+U2e+U1o+U2o)/4`; decoder upsample by replication prim_YUV.c:36-95). Main view alone = a normal, decodable 4:2:0 picture.
- **Aux v1** (`general_ChromaV1ToYUV444`, prim_YUV.c:97-171): aux **Y plane** carries the odd rows of U444 and V444 at full width, interleaved in 8-line blocks per 16-line macroblock row (rows y%16<8 → U444 row 2k+1, else V444 row 2k+1) (B4/B5); aux U/V planes carry U444/V444 at (odd x, even y) (B6/B7). Macroblock-local interleave keeps each 16×16 aux MB spatially co-located with the main MB.
- **Aux v2** (`general_ChromaV2ToYUV444`, prim_YUV.c:173-223): aux Y plane left half = U444 odd columns (all rows), right half = V444 odd columns (B4/B5); aux U plane left/right quarter = U444/V444 at (x≡0 mod 4, odd y) (B6/B7); aux V plane left/right quarter = U444/V444 at (x≡2 mod 4, odd y) (B8/B9). Frame-level (not MB-level) layout — better spatial coherence for the encoder than v1's 8-line interleave.
- **Even-even sample reconstruction** (the only sample not transmitted directly): `U[2x,2y] = 4·avg − U[2x+1,2y] − U[2x,2y+1] − U[2x+1,2y+1]`, used only if it differs from the averaged value by ≥30 (`CONDITIONAL_CLIP`, prim_internal.h:215-226; applied prim_YUV.c:355-365, 439-443) — spec's optional "cutoff threshold of 30" (§3.3.8.3.2 p.109, §3.3.8.3.3 p.112). Rationale: quantization noise in the aux stream would otherwise amplify ×4.
- **Composition rule**: luma-subframe rects → convert from main only (as 4:2:0); chroma-subframe rects → use Y/U/V "from the last corresponding rectangle in a luma subframe" + current chroma (p.109). So LC=1 then LC=2 = **send sharp-luma now, add chroma fidelity later** (a progressive chroma upgrade path).
- FreeRDP encoder: `avc444_compress` splits to main/aux YUV, runs 64×64 change detection separately for luma-view and chroma-view (h264.c:461-466), picks LC 0/1/2 by which changed (h264.c:469-485). Known issue: because aux and main pictures alternate in one encoder, each picture's reference is the other view type unless the encoder uses long-term refs → poor inter prediction / big aux frames (UNVERIFIED for Windows encoder; FreeRDP just uses default ref structure).

### 2.9 Alpha codec (0x000C)
`alphaSig u16 = 0x414C ("AL") | compressed u16 | data`: raw alpha bytes, or `CLEARCODEC_ALPHA_RLE_SEGMENT` (alpha u8 + runLengthFactor1/2/3 like ClearCodec) ([MS-RDPEGFX] §2.2.4.3 p.66-67). Sent *after* a color codec on ARGB surfaces (RAIL windows with transparency).

### 2.10 CAPROGRESSIVE_V2 0x000D
Defined in FreeRDP (include/freerdp/channels/rdpgfx.h:202) but not in the spec codec table (UNVERIFIED purpose; appears unused).

### 2.11 AV1 (FreeRDP-private, not MS)
`RDPGFX_CODECID_AV1 = 0x0001`, negotiated only via FreeRDP-private `RDPGFX_CAPVERSION_FRDP_1 = 0x00010000` with flags `AV1_I444_SUPPORTED 0x10000000` / `AV1_I444_DISABLED 0x20000000` (rdpgfx.h:108-195; client caps rdpgfx_main.c:536-556). libfreerdp/codec/av1.c (917 lines, © 2026 Thincast): encode via **libaom**, decode via **dav1d** (av1.c:29-66), reuses the AVC420 metablock format (`allocate_h264_metablock`, av1.c:83; gdi/gfx.c:434-485), native **4:4:4 (AV1 High profile I444)** option — i.e. FreeRDP's modern replacement for the AVC444 two-picture hack. Shadow server can emit it (server/shadow/shadow_client.c:1275-1286). Windows servers do not know it.

## 3. How servers pick a codec per region

### 3.1 Windows (RDS / AVD / W365) — "mixed mode"
From Microsoft's AVD doc "Graphics encoding over the Remote Desktop Protocol" (learn.microsoft.com/en-us/azure/virtual-desktop/graphics-encoding, updated 2025-10):
- Pipeline per frame: (1) **video detection** → video regions go to AVC/H.264; (2) non-video: "image processors determine if there are **delta changes, motion is detected, or if content is available in the cache**" → emitted as SurfaceToSurface / CacheToSurface / nothing; (3) remaining pixels go to an **image classifier: text vs image**; (4) text → "a custom codec that's optimized for text" (this is ClearCodec — the only text-specific codec in [MS-RDPEGFX]; the doc never names it, so the mapping is an inference, UNVERIFIED but strongly implied); images → AVC/H.264 (default when client supports it) or RemoteFX (Progressive).
- Quote: "On average, approximately 80% of the graphics data for a remote session is text." "Mixed-mode is only available with software encoding." With a GPU (hardware encode) or "full-screen video" profile, *everything* is H.264 or HEVC and "it performs worse than mixed-mode encoding when the screen content is largely text based".
- 4:4:4: default chroma is 4:2:0; 4:4:4 requires enabling full-screen AVC and GPO "Prioritize H.264/AVC 444 graphics mode for Remote Desktop Connections" (policy `TS_SERVER_AVC444_MODE_PREFERRED`, registry `AVC444ModePreferred`) + Image Quality High. AVC444 was introduced in RDP 10 / Win10 / WS2016 specifically because "text shows a halo effect with typical implementations of AVC/H.264" (MS RDS blog 2016, quoted in FreeRDP issue #3490; original techcommunity post body not retrievable).
- HEVC/H.265: doc claims HEVC full-screen with GPU hosts, "25-50% data compression compared to AVC". There is **no HEVC codecId in [MS-RDPEGFX] v20260511** → must ride on the undocumented 11.x capsets (0x000B01xx, "used by Azure", rdpgfx.h:128-130) (UNVERIFIED).
- RDP 8.1 introduced "AVC mixed mode" = AVC for images, text by the proprietary codec (search-result summary of MS blog; UNVERIFIED primary text).
- Caps interplay: pre-10.4 capsets only guarantee AVC as a whole frame codec; 10.4+ explicitly allows **AVC420 in the same frame as other codecs** ([MS-RDPEGFX] §2.2.3.7 p.40-41) — that's the protocol hook for per-region mixing inside one END_FRAME.
- AVC420 region mask semantics make mixing cheap: H.264 is encoded for the whole surface-sized canvas but only `regionRects` are blitted; ClearCodec/Planar rects are separate WTS1 PDUs overwriting text areas in the same frame.
- "Quality improves when idle": Progressive RFX TILE_UPGRADE passes, and the AVC420 qpVal `p` bit (region "progressively encoded"), and AVC444 LC=1-then-LC=2 (luma now, chroma later) are the three wire mechanisms. Windows' exact scheduling policy is undocumented (UNVERIFIED).

### 3.2 FreeRDP shadow server (server/shadow/shadow_client.c)
**One codec per session, no per-region classification.** `shadow_client_send_surface_gfx` (shadow_client.c:1695-1800) priority: AV1 (if FRDP_1 caps) → AVC444v2/AVC444 → AVC420 → RemoteFX (CAVIDEO) → Progressive → Planar → Uncompressed (codec ids set at 1286, 1410, 1483, 1550, 1598, 1640). Each update = StartFrame + one WTS + EndFrame. Damage for H.264 = 64×64 YUV tile compare (libfreerdp/codec/h264.c:279-322). H.264 defaults: VBR, 10 Mbit/s, 30 fps, QP 0 (=encoder-chosen) (shadow_server.c:1170-1173).
Flow control: `inFlight = frameId − lastAckFrameId` (0 if SUSPEND); if inFlight>1 → `fps = maxFps/(inFlight+1)`, else fps += 2 per frame up to maxFps (shadow_encoder.c:36-80). queueDepth stored but unused beyond SUSPEND (shadow_client.c:848-861).

### 3.3 xrdp (xrdp/xrdp_encoder.c, xrdp/gfx.toml)
Also one codec per session, configured order `[codec] order = ["H.264","RFX"]` (gfx.toml). H.264 via x264 (default `preset=ultrafast tune=zerolatency profile=main`, per-connection-type VBV profiles lan/wan/broadband/satellite/modem chosen from RDP connection type/autodetect) or OpenH264. **AVC420 only** (AVC444 defined in xrdp_egfx.h but not encoded). Metablock hard-codes `qp=23, quality=100` for all rects (xrdp_encoder.c:733-734) — purely cosmetic. Supports SolidFill and SurfaceToSurface passthrough from the X module (xrdp_encoder.c:1198-1255, 1388-1488). Flow control: `frames_in_flight` default 2 (range 1..16) (xrdp_encoder.c:45-48); module is only told to produce the next frame when `frame_id_client + fif > frame_id_server` (xrdp/xrdp_mm.c:1337-1350) — a strict ack window. `max_compressed_bytes` default 3 MiB/frame (xrdp_encoder.c:50).

### 3.4 gnome-remote-desktop (brief; covered elsewhere)
Codec enum → RDPGFX: CAPROGRESSIVE (RFX progressive, simple tiles), AVC420, AVC444v2 (src/grd-rdp-dvc-graphics-pipeline.c:530-540). Prefers AVC444v2 when client advertises it (hardware encode path) else AVC420 else RFX-progressive (…pipeline.c:181-188, 266-267). Implements LC=0/1/2 (…pipeline.c:605-660) — i.e. can send luma-only and chroma-only updates.

### 3.5 What "progressive refinement" is on the wire (summary)
| mechanism | frame N (change) | frames N+1.. (static) | end state |
|---|---|---|---|
|RFX Progressive| WTS2 TILE_FIRST at progressiveQuality q0 (coarse bitplanes) | WTS2 TILE_UPGRADE (SRL for newly-significant coeffs, RAW refinement bits for already-significant) toward 0xFF | base RemoteFX quant (lossy) |
|AVC420| WTS1 AVC420, regionRects=damaged, qp high, p=0 | (Windows) re-encode same rects at lower QP with p=1 (UNVERIFIED exact policy) | near-lossless H.264 |
|AVC444| LC=1 (luma+4:2:0 chroma) | LC=2 (aux chroma) for those rects | full 4:4:4 |
|Text| ClearCodec (lossless, cached) — no refinement needed | GLYPH_HIT / VBAR hits on repaint | exact |

## 4. Caching, flow control, transports

**Bitmap cache (GFX level).** SurfaceToCache(surfaceId, cacheKey u64, cacheSlot, rectSrc) copies surface pixels (post-decode, so any codec) into a slot; CacheToSurface(cacheSlot, surfaceId, destPts[]) replays to N points; Evict frees. Limits: 100 MB / 25,600 slots, or 16 MB / 4,096 slots ([MS-RDPEGFX] §3.3.1.4 p.95). Server allocates slots and tracks the map (§3.2.1.1 p.84) — client is dumb storage. Persistent across sessions via CacheImportOffer (≤5461 entries {cacheKey, bitmapLength}) / Reply (slot assignments) (§2.2.2.16-17 p.30-31). Use cases: re-showing a previously seen window/tab, UI chrome, scroll-back. Windows uses it after its "content available in the cache" detector (AVD doc).
**SurfaceToSurface**: scroll/window-move = copy rectSrc to destPts on client, then only the newly exposed strip is encoded. Windows' "motion detected" image processor feeds this (AVD doc). xrdp exposes it (xrdp_encoder.c:1225). FreeRDP shadow does not detect motion.
**ClearCodec internal caches**: Glyph Storage 4,000 bitmaps (≤1024 px² each), V-Bar 32,768 columns, Short V-Bar 16,384 (§3.3.1.9-13 p.96) — sub-bitmap, content-addressed-ish reuse specific to text.
**Bulk compression**: ZGFX (RDP 8.0 LZ77+Huffman) over every S→C GFX PDU (§3.1.9.1 p.79; libfreerdp/codec/zgfx.c) — history window gives cross-PDU redundancy removal for free on command streams & planar.

**Flow control via frame acks.** Implicit, window-based: server tracks unacked frameIds; client acks each EndFrame after decode; `queueDepth` (bytes buffered, not yet processed) lets the server see *decoder* backlog, distinct from network backlog; server "SHOULD ... throttle the graphics frame rate accordingly" ([MS-RDPEGFX] §3.2.5.13 p.86). Because frames are only acked **after decode**, the ack RTT = network RTT + decode time → the window auto-shrinks the frame rate when either the network or the client is slow. Implementations: xrdp hard window 2 frames (xrdp_mm.c:1337-1350); FreeRDP shadow fps = maxFps/(inflight+1) (shadow_encoder.c:59-80). Clients can opt out (SUSPEND 0xFFFFFFFF) — then the server must assume the client decodes faster than it receives. QoE ack adds client-side decode (timeDiffSE) and render (timeDiffEDR) timings (§2.2.2.21 p.33-34). Separate from this, **bandwidth is adapted by network autodetect** ([MS-RDPBCGR] §2.2.14: RTT Measure Request/Response, Bandwidth Measure Start/Payload/Stop, Network Characteristics Result {baseRTT, bandwidth, averageRTT}; continuous detection during session) — the GFX spec says it SHOULD be enabled (§2.1 p.14). (BCGR PDU type values from memory: RTT request 0x1001/0x0001, BW start 0x1014/0x0014, BW stop 0x002B/0x0429, result 0x08C0/0x0840 — UNVERIFIED.)

**Transports.**
- GFX is a DVC and requires a **non-lossy** channel (§2.1 p.14), so it can move to **reliable** UDP only; never lossy.
- [MS-RDPEUDP] (v1): two modes — reliable (RDP-UDP-R, persistent retransmits) and lossy/unreliable (RDP-UDP-L, `RDPUDP_FLAG_SYNLOSSY 0x0200`: no retransmits, **ordering preserved**, receiver out-of-order timer) (§1.3 p.7; §2.2.2.1 p.13; §3.1.5.1.x p.18). **FEC exists in the protocol for both**: an FEC packet = linear combination over **GF(256)** of up to 255 source packets (block size ≤255, or none), FEC packets never acked or retransmitted, receiver may ignore them; "Upon receiving notification of a packet loss, the sender retransmits the lost datagram" (§1.3.2.2 p.9-10; §3.1.1.6 p.20-27). Congestion control MUST exist, NewReno-variant acceptable; loss → RDPUDP_FLAG_CN → sender cuts rate, marks RDPUDP_FLAG_CWR (§3.1.1.8 p.27).
- [MS-RDPEUDP2] (RDPUDP_PROTOCOL_VERSION_3, negotiated in the v1 SYN): "only supports reliable UDP mode ... This transport does not include an FEC layer" (§3.1.1.1 p.17); adds DelayAckInfo payload (delayed-ack parameters) etc. So modern Windows RDP-over-UDP = reliable, retransmission-based, no FEC.
- [MS-RDPEMT]: side-band multitransport connections (reliable UDP secured with TLS, lossy UDP with DTLS) bootstrapped from the TCP connection by Initiate Multitransport Request (requestId + security cookie) → Tunnel Create Request/Response on the UDP flow; DVCs (incl. GFX) are then moved onto it, with **Soft-Sync** ([MS-RDPEDYC] §3.1.5.3) allowing channels to migrate between TCP and UDP mid-session (§1.3 p.6-7). In practice the lossy tunnel is used for audio/UDP-friendly channels (UNVERIFIED); graphics stays reliable.
- Implication: RDP graphics never tolerates loss — every frame is a reliable in-order byte stream. Latency under loss comes from retransmission + in-order delivery (head-of-line), mitigated only by congestion control and the frame-ack window.

## 5. FreeRDP client decode/compose path + known issues
(pending)
## 6. Critical assessment for Sunna
(pending)
