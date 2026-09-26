//! Lossless tiles for small screen changes, shared by capture backends.

use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use sunna_proto::tiles::TileBatch;

/// Fast lane only for small changes: at most this share of the frame.
/// Anything bigger is left to the video encoder.
const TILE_MAX_AREA_FRACTION: f64 = 0.04;
/// And at most this many rectangles after merging.
const TILE_MAX_RECTS: usize = 32;
/// Change detection granularity, in pixels. macOS's own dirty rects are
/// far too coarse for this (typing in VS Code reported 50-100% of the screen
/// changed), so the tile worker diffs frames itself in blocks this size.
const TILE_BLOCK: usize = 32;
/// A block that keeps changing is animation (video, spinners), which the
/// video encoder handles far better than lossless tiles: a small YouTube
/// window was costing up to ~800 KB/s of tiles on top of the video stream.
/// Each block keeps an exponentially decaying change count (half-life
/// below); above the threshold it's animated and left to the video.
/// Steady-state counts: typing at 10 keys/s ~4, 15 keys/s ~6, 30 fps ~11.
const TILE_HEAT_HALF_LIFE_SECS: f32 = 0.25;
const TILE_ANIMATED_HEAT: f32 = 7.0;
/// Compressed bytes allowed per batch; beyond this the video is cheaper.
const TILE_MAX_BATCH_BYTES: usize = 256 * 1024;

/// Why captures did or didn't take the fast lane, logged every 2 s so the
/// thresholds can be tuned from real sessions.
#[derive(Default)]
struct TileDecisions {
    since: Option<std::time::Instant>,
    captures: u32,
    sent: u32,
    no_rects: u32,
    no_change: u32,
    too_large: u32,
    animated: u32,
    diff_us: Vec<u64>,
    /// Changed-area fractions (percent) and rect counts seen, for the log.
    area_pct: Vec<f64>,
    rect_counts: Vec<usize>,
}

impl TileDecisions {
    fn note(&mut self, outcome: TileOutcome, area_pct: f64, rects: usize, diff_us: u64) {
        let now = std::time::Instant::now();
        let since = *self.since.get_or_insert(now);
        self.captures += 1;
        match outcome {
            TileOutcome::Sent => self.sent += 1,
            #[cfg(target_os = "macos")]
            TileOutcome::NoRects => self.no_rects += 1,
            TileOutcome::NoChange => self.no_change += 1,
            TileOutcome::TooLarge => self.too_large += 1,
            TileOutcome::Animated => self.animated += 1,
        }
        if diff_us > 0 {
            self.diff_us.push(diff_us);
        }
        if rects > 0 {
            self.area_pct.push(area_pct);
            self.rect_counts.push(rects);
        }
        if now.duration_since(since) >= Duration::from_secs(2) {
            let median = |values: &mut Vec<f64>| -> f64 {
                if values.is_empty() {
                    return 0.0;
                }
                values.sort_by(|a, b| a.total_cmp(b));
                values[values.len() / 2]
            };
            let mut counts: Vec<f64> = self.rect_counts.iter().map(|&count| count as f64).collect();
            let mut diff_ms: Vec<f64> = self.diff_us.iter().map(|&us| us as f64 / 1000.0).collect();
            tracing::info!(
                captures = self.captures,
                sent = self.sent,
                no_rects = self.no_rects,
                no_change = self.no_change,
                too_large = self.too_large,
                animated = self.animated,
                diff_ms_median = format!("{:.2}", median(&mut diff_ms)),
                area_pct_median = format!("{:.2}", median(&mut self.area_pct)),
                area_pct_max = format!("{:.2}", self.area_pct.iter().cloned().fold(0.0, f64::max)),
                tiles_median = median(&mut counts),
                tiles_max = self.rect_counts.iter().copied().max().unwrap_or(0),
                "fast lane decisions"
            );
            *self = TileDecisions::default();
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum TileOutcome {
    Sent,
    /// macOS reported nothing changed.
    #[cfg(target_os = "macos")]
    NoRects,
    /// The captured pixels are identical.
    NoChange,
    TooLarge,
    /// Everything that changed is animating; left to the video.
    Animated,
}

static TILE_DECISIONS: Mutex<Option<TileDecisions>> = Mutex::new(None);

pub(crate) fn note_tile_decision(outcome: TileOutcome, area_pct: f64, rects: usize, diff_us: u64) {
    TILE_DECISIONS
        .lock()
        .unwrap()
        .get_or_insert_with(TileDecisions::default)
        .note(outcome, area_pct, rects, diff_us);
}

#[derive(Default)]
pub(crate) struct TileDiffer {
    previous: Vec<u8>,
    size: (usize, usize),
    // Per block: decaying change count and when it was last updated.
    heat: Vec<(f32, Instant)>,
}

impl TileDiffer {
    #[cfg(any(target_os = "linux", test))]
    pub(crate) fn diff(
        &mut self,
        base: &[u8],
        stride: usize,
        width: usize,
        height: usize,
        capture_ts_us: u64,
    ) -> Option<TileBatch> {
        let started = Instant::now();
        let changed = self.copy_changes(base, stride, width, height)?;
        self.encode_changes(changed, capture_ts_us, started)
    }

    /// Copy and compare while the capture's pixels are readable. Surface
    /// backends can release their read lock before encoding the tiles.
    pub(crate) fn copy_changes(
        &mut self,
        base: &[u8],
        stride: usize,
        width: usize,
        height: usize,
    ) -> Option<Vec<bool>> {
        let row_bytes = width * 4;
        if self.size != (width, height) {
            // First frame (or a size change): take a full copy, send nothing.
            self.size = (width, height);
            self.previous = vec![0u8; row_bytes * height];
            self.heat = vec![
                (0.0, std::time::Instant::now());
                width.div_ceil(TILE_BLOCK) * height.div_ceil(TILE_BLOCK)
            ];
            for row in 0..height {
                let src = &base[row * stride..row * stride + row_bytes];
                self.previous[row * row_bytes..(row + 1) * row_bytes].copy_from_slice(src);
            }
            return None;
        }
        // Compare the whole frame block by block: macOS's region only
        // says *that* something changed (its coordinates are coarse, and
        // trusting them risks missing changes), and a full compare costs a
        // few ms on this worker thread. Changed rows are copied into
        // `previous` as we go, so it mirrors the last capture we looked at.
        let (x0, y0, x1, y1) = (0, 0, width, height);
        let (bx0, by0) = (x0 / TILE_BLOCK, y0 / TILE_BLOCK);
        let (bx1, by1) = (x1.div_ceil(TILE_BLOCK), y1.div_ceil(TILE_BLOCK));
        let blocks_x = width.div_ceil(TILE_BLOCK);
        let mut changed = vec![false; blocks_x * height.div_ceil(TILE_BLOCK)];
        for by in by0..by1 {
            let row_start = by * TILE_BLOCK;
            let row_end = (row_start + TILE_BLOCK).min(height);
            for bx in bx0..bx1 {
                let col_start = bx * TILE_BLOCK * 4;
                let col_end = ((bx + 1) * TILE_BLOCK).min(width) * 4;
                let mut differs = false;
                for row in row_start..row_end {
                    let src = &base[row * stride + col_start..row * stride + col_end];
                    let dst =
                        &mut self.previous[row * row_bytes + col_start..row * row_bytes + col_end];
                    if src != dst {
                        dst.copy_from_slice(src);
                        differs = true;
                    }
                }
                if differs {
                    changed[by * blocks_x + bx] = true;
                }
            }
        }
        Some(changed)
    }

    pub(crate) fn encode_changes(
        &mut self,
        changed: Vec<bool>,
        capture_ts_us: u64,
        started: Instant,
    ) -> Option<TileBatch> {
        self.encode_changes_at(changed, capture_ts_us, started, Instant::now())
    }

    fn encode_changes_at(
        &mut self,
        mut changed: Vec<bool>,
        capture_ts_us: u64,
        started: Instant,
        now: Instant,
    ) -> Option<TileBatch> {
        let (width, height) = self.size;
        let row_bytes = width * 4;
        let (bx0, by0) = (0, 0);
        let (bx1, by1) = (width.div_ceil(TILE_BLOCK), height.div_ceil(TILE_BLOCK));
        let blocks_x = bx1;
        let changed_count = changed.iter().filter(|&&changed| changed).count();
        let diff_us = started.elapsed().as_micros() as u64;

        if changed_count == 0 {
            note_tile_decision(TileOutcome::NoChange, 0.0, 0, diff_us);
            return None;
        }
        // Update each changed block's heat; drop animated blocks from the
        // tile set (they stay with the video).
        let mut animated = 0usize;
        for (index, block_changed) in changed.iter_mut().enumerate() {
            if !*block_changed {
                continue;
            }
            let (count, last) = self.heat[index];
            let elapsed = now.duration_since(last).as_secs_f32();
            let decayed = count * 0.5f32.powf(elapsed / TILE_HEAT_HALF_LIFE_SECS);
            self.heat[index] = (decayed + 1.0, now);
            if decayed + 1.0 > TILE_ANIMATED_HEAT {
                *block_changed = false;
                animated += 1;
            }
        }
        let changed_count = changed_count - animated;
        if changed_count == 0 {
            note_tile_decision(TileOutcome::Animated, 0.0, 0, diff_us);
            return None;
        }
        let area = changed_count * TILE_BLOCK * TILE_BLOCK;
        let area_pct = area as f64 * 100.0 / (width * height).max(1) as f64;
        if area_pct > TILE_MAX_AREA_FRACTION * 100.0 {
            note_tile_decision(TileOutcome::TooLarge, area_pct, changed_count, diff_us);
            return None;
        }
        // Merge changed blocks into rectangles: horizontal runs per block
        // row, then runs stacked with identical spans in consecutive rows.
        // (Rects are in block units: x0, x1 exclusive, y0, y1 exclusive.)
        let mut rects: Vec<(usize, usize, usize, usize)> = Vec::new();
        let mut open: Vec<(usize, usize, usize, usize)> = Vec::new();
        for by in by0..by1 {
            let mut runs = Vec::new();
            let mut bx = bx0;
            while bx < bx1 {
                if !changed[by * blocks_x + bx] {
                    bx += 1;
                    continue;
                }
                let run_start = bx;
                while bx < bx1 && changed[by * blocks_x + bx] {
                    bx += 1;
                }
                runs.push((run_start, bx));
            }
            let mut next_open = Vec::with_capacity(runs.len());
            for (start, end) in runs {
                if let Some(index) = open
                    .iter()
                    .position(|&(x0, x1, _, y1)| x0 == start && x1 == end && y1 == by)
                {
                    let (x0, x1, y0, _) = open.swap_remove(index);
                    next_open.push((x0, x1, y0, by + 1));
                } else {
                    next_open.push((start, end, by, by + 1));
                }
            }
            rects.append(&mut open); // runs that didn't continue are done
            open = next_open;
        }
        rects.append(&mut open);
        if rects.len() > TILE_MAX_RECTS {
            note_tile_decision(TileOutcome::TooLarge, area_pct, rects.len(), diff_us);
            return None;
        }
        // Cut tiles from `previous` (which now holds this capture's pixels).
        let mut tiles = Vec::with_capacity(rects.len());
        for (bx_start, bx_end, by_start, by_end) in rects {
            let px0 = bx_start * TILE_BLOCK;
            let px1 = (bx_end * TILE_BLOCK).min(width);
            let py0 = by_start * TILE_BLOCK;
            let py1 = (by_end * TILE_BLOCK).min(height);
            let (tile_w, tile_h) = (px1 - px0, py1 - py0);
            let mut pixels = Vec::with_capacity(tile_w * tile_h * 4);
            for row in py0..py1 {
                pixels.extend_from_slice(
                    &self.previous[row * row_bytes + px0 * 4..row * row_bytes + px1 * 4],
                );
            }
            // Opaque: the capture's alpha byte isn't meaningful.
            for alpha in pixels.iter_mut().skip(3).step_by(4) {
                *alpha = 255;
            }
            if let Ok(qoi) = qoi::encode_to_vec(&pixels, tile_w as u32, tile_h as u32) {
                tiles.push(sunna_proto::tiles::Tile {
                    x: px0 as u32,
                    y: py0 as u32,
                    width: tile_w as u32,
                    height: tile_h as u32,
                    qoi,
                });
            }
        }
        if tiles.iter().map(|tile| tile.qoi.len()).sum::<usize>() > TILE_MAX_BATCH_BYTES {
            note_tile_decision(TileOutcome::TooLarge, area_pct, tiles.len(), diff_us);
            return None;
        }
        note_tile_decision(TileOutcome::Sent, area_pct, tiles.len(), diff_us);
        Some(sunna_proto::tiles::TileBatch {
            capture_ts_us,
            stream_width: width as u32,
            stream_height: height as u32,
            tiles,
        })
    }
}

/// One pending capture; a newer job replaces it when the worker falls behind.
pub(crate) struct TileQueue<J> {
    state: Mutex<TileState<J>>,
    ready: Condvar,
}

struct TileState<J> {
    job: Option<J>,
    closed: bool,
}

impl<J> TileQueue<J> {
    pub(crate) fn submit(&self, job: J) {
        let mut state = self.state.lock().unwrap();
        if !state.closed {
            state.job = Some(job);
            self.ready.notify_one();
        }
    }
}

pub(crate) struct TileWorker<J> {
    pub(crate) queue: Arc<TileQueue<J>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl<J: Send + 'static> TileWorker<J> {
    pub(crate) fn start(
        sink: crate::TileSink,
        mut process: impl FnMut(&mut TileDiffer, J) -> Option<TileBatch> + Send + 'static,
    ) -> std::io::Result<Self> {
        let queue = Arc::new(TileQueue {
            state: Mutex::new(TileState {
                job: None,
                closed: false,
            }),
            ready: Condvar::new(),
        });
        let worker_queue = Arc::clone(&queue);
        let thread = std::thread::Builder::new()
            .name("sunna-tiles".into())
            .spawn(move || {
                let mut differ = TileDiffer::default();
                loop {
                    let job = {
                        let mut state = worker_queue.state.lock().unwrap();
                        loop {
                            if state.closed {
                                return;
                            }
                            if let Some(job) = state.job.take() {
                                break job;
                            }
                            state = worker_queue.ready.wait(state).unwrap();
                        }
                    };
                    if let Some(batch) = process(&mut differ, job) {
                        sink(batch);
                    }
                }
            })?;
        Ok(Self {
            queue,
            thread: Some(thread),
        })
    }
}

impl<J> Drop for TileWorker<J> {
    fn drop(&mut self) {
        {
            let mut state = self.queue.state.lock().unwrap();
            state.job = None;
            state.closed = true;
            self.queue.ready.notify_all();
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_unchanged_and_resized_frames_send_nothing() {
        let mut differ = TileDiffer::default();
        let pixels = vec![0; 512 * 512 * 4];
        assert!(differ.diff(&pixels, 512 * 4, 512, 512, 1).is_none());
        assert!(differ.diff(&pixels, 512 * 4, 512, 512, 2).is_none());
        assert!(differ.diff(&pixels, 256 * 4, 256, 512, 3).is_none());
    }

    #[test]
    fn small_changes_merge_and_roundtrip_with_stride_and_opaque_alpha() {
        let (width, height, stride) = (513, 513, 513 * 4 + 16);
        let mut pixels = vec![0; stride * height];
        let mut differ = TileDiffer::default();
        assert!(differ.diff(&pixels, stride, width, height, 1).is_none());
        // Four blocks merge horizontally and vertically; the corner clips
        // to the frame size. Padding must never appear in the tiles.
        for (x, y) in [(33, 65), (65, 65), (33, 97), (65, 97), (512, 512)] {
            let offset = y * stride + x * 4;
            pixels[offset..offset + 4].copy_from_slice(&[7, 23, 199, 0]);
        }
        for row in pixels.chunks_exact_mut(stride) {
            row[width * 4..].fill(123);
        }
        let batch = differ.diff(&pixels, stride, width, height, 42).unwrap();
        assert_eq!(
            (batch.capture_ts_us, batch.stream_width, batch.stream_height),
            (42, 513, 513)
        );
        assert_eq!(batch.tiles.len(), 2);
        assert_eq!(
            batch
                .tiles
                .iter()
                .map(|t| (t.x, t.y, t.width, t.height))
                .collect::<Vec<_>>(),
            vec![(32, 64, 64, 64), (512, 512, 1, 1)]
        );
        for tile in batch.tiles {
            let (header, decoded) = qoi::decode_to_vec(&tile.qoi).unwrap();
            assert_eq!((header.width, header.height), (tile.width, tile.height));
            let mut expected = Vec::new();
            for y in tile.y as usize..(tile.y + tile.height) as usize {
                for x in tile.x as usize..(tile.x + tile.width) as usize {
                    expected.extend_from_slice(&pixels[y * stride + x * 4..y * stride + x * 4 + 3]);
                    expected.push(255);
                }
            }
            assert_eq!(decoded, expected);
        }
        assert!(differ.diff(&pixels, stride, width, height, 43).is_none());
    }

    #[test]
    fn large_changes_send_nothing_but_update_the_previous_copy() {
        let mut differ = TileDiffer::default();
        let mut pixels = vec![0; 512 * 512 * 4];
        differ.diff(&pixels, 512 * 4, 512, 512, 1);
        pixels.fill(42);
        assert!(differ.diff(&pixels, 512 * 4, 512, 512, 2).is_none());
        pixels[0] = 43;
        let batch = differ.diff(&pixels, 512 * 4, 512, 512, 3).unwrap();
        assert_eq!(batch.tiles.len(), 1);
        assert_eq!((batch.tiles[0].width, batch.tiles[0].height), (32, 32));
    }

    #[test]
    fn animated_blocks_are_dropped_then_cool_down() {
        let mut differ = TileDiffer::default();
        let mut pixels = vec![0; 512 * 512 * 4];
        differ.diff(&pixels, 512 * 4, 512, 512, 1);
        let start = Instant::now();
        for i in 1..=20 {
            pixels[0] = i;
            let changed = differ.copy_changes(&pixels, 512 * 4, 512, 512).unwrap();
            let batch = differ.encode_changes_at(
                changed,
                i as u64,
                start,
                start + Duration::from_millis(i as u64 * 16),
            );
            if i <= 7 {
                assert!(batch.is_some());
            }
            if i >= 10 {
                assert!(batch.is_none());
            }
        }
        pixels[0] = 21;
        let changed = differ.copy_changes(&pixels, 512 * 4, 512, 512).unwrap();
        assert!(differ
            .encode_changes_at(changed, 21, start, start + Duration::from_secs(3))
            .is_some());
    }

    #[test]
    fn too_many_rectangles_are_dropped() {
        let mut differ = TileDiffer::default();
        let mut pixels = vec![0; 2048 * 2048 * 4];
        differ.diff(&pixels, 2048 * 4, 2048, 2048, 1);
        for i in 0..33 {
            let (x, y) = ((i % 16) * 64, (i / 16) * 64);
            pixels[(y * 2048 + x) * 4] = 1;
        }
        assert!(differ.diff(&pixels, 2048 * 4, 2048, 2048, 2).is_none());
    }

    #[test]
    fn oversized_qoi_batch_is_dropped() {
        let mut differ = TileDiffer::default();
        let mut pixels = vec![0; 2048 * 2048 * 4];
        differ.diff(&pixels, 2048 * 4, 2048, 2048, 1);
        let mut random = 12345u32;
        for y in 0..256 {
            for x in 0..288 {
                random ^= random << 13;
                random ^= random >> 17;
                random ^= random << 5;
                let offset = (y * 2048 + x) * 4;
                pixels[offset..offset + 4].copy_from_slice(&random.to_le_bytes());
            }
        }
        assert!(differ.diff(&pixels, 2048 * 4, 2048, 2048, 2).is_none());
    }

    #[test]
    fn worker_keeps_only_the_newest_pending_job_and_stops_on_drop() {
        let (entered, received) = std::sync::mpsc::channel();
        let (release, resume) = std::sync::mpsc::channel();
        let worker = TileWorker::start(Arc::new(|_| {}), move |_, job: u32| {
            entered.send(job).unwrap();
            if job == 1 {
                resume.recv().unwrap();
            }
            None
        })
        .unwrap();
        worker.queue.submit(1);
        assert_eq!(received.recv_timeout(Duration::from_secs(2)).unwrap(), 1);
        worker.queue.submit(2);
        worker.queue.submit(3);
        release.send(()).unwrap();
        assert_eq!(received.recv_timeout(Duration::from_secs(2)).unwrap(), 3);
        drop(worker);
        assert!(received.recv().is_err());
    }
}
