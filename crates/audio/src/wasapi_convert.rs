//! Packet conversion kept independent of WASAPI so it can be tested on every host.

use std::collections::VecDeque;

use crate::{FRAME_LEN, SAMPLE_RATE};

#[derive(Clone, Copy, Debug)]
pub(crate) struct Format {
    pub channels: usize,
    pub rate: u32,
    pub bits: u16,
    pub float: bool,
    pub block_align: usize,
    pub channel_mask: u32,
}

impl Format {
    pub fn validate(self) -> anyhow::Result<Self> {
        anyhow::ensure!(
            self.channels > 0 && self.rate > 0,
            "WASAPI returned an empty audio format"
        );
        anyhow::ensure!(
            if self.float {
                matches!(self.bits, 32 | 64)
            } else {
                matches!(self.bits, 8 | 16 | 24 | 32)
            },
            "unsupported WASAPI sample encoding: {} bits, float={}",
            self.bits,
            self.float
        );
        anyhow::ensure!(
            self.block_align >= self.channels * (self.bits as usize / 8),
            "invalid WASAPI block alignment"
        );
        Ok(self)
    }

    fn sample(self, bytes: &[u8]) -> f32 {
        let value = match (self.float, self.bits) {
            (true, 32) => f32::from_le_bytes(bytes[..4].try_into().unwrap()),
            (true, 64) => f64::from_le_bytes(bytes[..8].try_into().unwrap()) as f32,
            (false, 8) => (bytes[0] as f32 - 128.0) / 128.0,
            (false, 16) => i16::from_le_bytes(bytes[..2].try_into().unwrap()) as f32 / 32768.0,
            (false, 24) => {
                (i32::from_le_bytes([0, bytes[0], bytes[1], bytes[2]]) >> 8) as f32 / 8388608.0
            }
            (false, 32) => i32::from_le_bytes(bytes[..4].try_into().unwrap()) as f32 / 2147483648.0,
            _ => unreachable!("validated format"),
        };
        if value.is_finite() {
            value
        } else {
            0.0
        }
    }
}

pub(crate) fn float_to_i16(value: f32) -> i16 {
    if value.is_nan() {
        0
    } else {
        (value.clamp(-1.0, 1.0) * 32768.0).round() as i16
    }
}

/// WAVEFORMATEXTENSIBLE channels follow ascending speaker-mask bits. Preserve
/// front stereo, mix center/surround at -3 dB and LFE at -6 dB; clip only after
/// mixing. Unknown layouts alternate extra channels between left and right.
fn channel_weights(channels: usize, mask: u32) -> Vec<[f32; 2]> {
    if channels == 1 {
        return vec![[1.0, 1.0]];
    }
    let mut speakers = (0..32).filter(|bit| mask & (1 << bit) != 0);
    (0..channels)
        .map(|channel| {
            if mask == 0 || mask.count_ones() as usize != channels {
                return match channel {
                    0 => [1.0, 0.0],
                    1 => [0.0, 1.0],
                    n if n % 2 == 0 => [0.5, 0.0],
                    _ => [0.0, 0.5],
                };
            }
            let surround = std::f32::consts::FRAC_1_SQRT_2;
            match speakers.next().unwrap() {
                0 => [1.0, 0.0], // front left
                1 => [0.0, 1.0], // front right
                3 => [0.5, 0.5], // LFE
                4 | 6 | 9 | 12 | 15 => [surround, 0.0],
                5 | 7 | 10 | 14 | 17 => [0.0, surround],
                _ => [surround, surround], // center and height-center channels
            }
        })
        .collect()
}

pub(crate) struct Converter {
    pub format: Format,
    weights: Vec<[f32; 2]>,
    previous: Option<[f32; 2]>,
    next: f64,
    samples: VecDeque<i16>,
}

impl Converter {
    pub fn new(format: Format) -> Self {
        Self {
            weights: channel_weights(format.channels, format.channel_mask),
            format,
            previous: None,
            next: 0.0,
            samples: VecDeque::new(),
        }
    }

    pub fn reset(&mut self) {
        self.previous = None;
        self.next = 0.0;
        self.samples.clear();
    }

    pub fn push(&mut self, data: Option<&[u8]>, frames: usize) {
        let width = self.format.bits as usize / 8;
        for frame in 0..frames {
            let mut stereo = [0.0; 2];
            if let Some(data) = data {
                let offset = frame * self.format.block_align;
                for (channel, weights) in self.weights.iter().enumerate() {
                    let value = self.format.sample(&data[offset + channel * width..]);
                    stereo[0] += value * weights[0];
                    stereo[1] += value * weights[1];
                }
            }
            self.resample(stereo);
        }
        // A stalled encoder should resume near live sound, not play a backlog.
        let excess = self.samples.len().saturating_sub(FRAME_LEN * 10);
        self.samples.drain(..excess);
    }

    fn resample(&mut self, current: [f32; 2]) {
        let step = f64::from(self.format.rate) / f64::from(SAMPLE_RATE);
        if let Some(previous) = self.previous {
            // Linear interpolation carries its phase across packet boundaries.
            // This fallback favors low latency; Windows' SRC is preferred and
            // supplies better antialiasing when it accepts AUTOCONVERTPCM.
            while self.next <= 1.0 {
                for channel in 0..2 {
                    let value = previous[channel]
                        + (current[channel] - previous[channel]) * self.next as f32;
                    self.samples.push_back(float_to_i16(value));
                }
                self.next += step;
            }
            self.next -= 1.0;
        } else {
            self.samples.extend(current.map(float_to_i16));
            self.next = step;
        }
        self.previous = Some(current);
    }

    /// Called once per real-time 10 ms tick, including when WASAPI sent nothing.
    pub fn frame(&mut self, output: &mut [i16; FRAME_LEN]) {
        let available = self.samples.len().min(FRAME_LEN);
        for (slot, value) in output.iter_mut().zip(self.samples.drain(..available)) {
            *slot = value;
        }
        output[available..].fill(0);
        if available == 0 {
            // An empty tick is an idle-device gap. Keep interpolation history
            // for partial ticks: fractional-rate packets can be one sample short.
            self.previous = None;
            self.next = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn format(channels: usize, rate: u32, mask: u32) -> Format {
        Format {
            channels,
            rate,
            bits: 32,
            float: true,
            block_align: channels * 4,
            channel_mask: mask,
        }
        .validate()
        .unwrap()
    }
    fn bytes(samples: &[f32]) -> Vec<u8> {
        samples.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    #[test]
    fn clamp_and_pcm_encodings() {
        assert_eq!(float_to_i16(-2.0), i16::MIN);
        assert_eq!(float_to_i16(2.0), i16::MAX);
        assert_eq!(float_to_i16(f32::NAN), 0);
        assert_eq!(float_to_i16(0.5), 16384);
        for (bits, data) in [
            (8, vec![0]),
            (16, vec![0, 128]),
            (24, vec![0, 0, 128]),
            (32, vec![0, 0, 0, 128]),
        ] {
            let fmt = Format {
                bits,
                float: false,
                block_align: bits as usize / 8,
                ..format(1, 48000, 0)
            }
            .validate()
            .unwrap();
            assert_eq!(fmt.sample(&data), -1.0);
        }
        assert!(Format {
            bits: 20,
            ..format(1, 48000, 0)
        }
        .validate()
        .is_err());
        assert!(Format {
            block_align: 1,
            ..format(2, 48000, 0)
        }
        .validate()
        .is_err());
    }

    #[test]
    fn downmix_mono_stereo_and_surround() {
        let mut mono = Converter::new(format(1, 48000, 0));
        mono.push(Some(&bytes(&[0.25])), 1);
        assert_eq!(mono.samples, [8192, 8192]);
        let mut stereo = Converter::new(format(2, 48000, 3));
        stereo.push(Some(&bytes(&[0.25, -0.5])), 1);
        assert_eq!(stereo.samples, [8192, -16384]);
        let mut surround = Converter::new(format(6, 48000, 0x3f));
        surround.push(Some(&bytes(&[0.0, 0.0, 0.5, 0.0, 0.25, 0.0])), 1);
        assert_eq!(
            surround.samples[0],
            float_to_i16(0.75 * std::f32::consts::FRAC_1_SQRT_2)
        );
        assert_eq!(
            surround.samples[1],
            float_to_i16(0.5 * std::f32::consts::FRAC_1_SQRT_2)
        );
    }

    #[test]
    fn resampling_is_continuous_across_packets() {
        for rate in [24000, 44100, 48000, 96000] {
            let samples: Vec<_> = (0..1000).map(|n| (n as f32 * 0.01).sin() * 0.25).collect();
            let mut whole = Converter::new(format(1, rate, 0));
            whole.push(Some(&bytes(&samples)), samples.len());
            let mut split = Converter::new(format(1, rate, 0));
            for chunk in samples.chunks(37) {
                split.push(Some(&bytes(chunk)), chunk.len());
            }
            assert_eq!(whole.samples, split.samples, "rate {rate}");
            let expected =
                ((samples.len() - 1) as f64 * 48000.0 / rate as f64).floor() as usize + 1;
            assert!((whole.samples.len() as isize / 2 - expected as isize).abs() <= 1);
        }
        let mut up = Converter::new(format(1, 24000, 0));
        up.push(Some(&bytes(&[0.0, 0.5, 1.0])), 3);
        assert_eq!(
            up.samples,
            [0, 0, 8192, 8192, 16384, 16384, 24576, 24576, 32767, 32767]
        );
    }

    #[test]
    fn fractional_rate_packets_do_not_reset_interpolation_each_tick() {
        let mut converter = Converter::new(format(1, 44100, 0));
        let packet = bytes(&vec![0.25; 441]);
        let mut output = [0; FRAME_LEN];
        converter.push(Some(&packet), 441);
        converter.frame(&mut output);
        for _ in 0..100 {
            converter.push(Some(&packet), 441);
            converter.frame(&mut output);
            assert_eq!(output, [8192; FRAME_LEN]);
        }
    }

    #[test]
    fn ten_ms_frames_silent_packets_and_idle_padding() {
        let mut converter = Converter::new(format(2, 48000, 3));
        converter.push(
            Some(&bytes(&vec![0.25; FRAME_LEN + 100])),
            (FRAME_LEN + 100) / 2,
        );
        let mut output = [1; FRAME_LEN];
        converter.frame(&mut output);
        assert_eq!(output, [8192; FRAME_LEN]);
        converter.frame(&mut output);
        assert_eq!(&output[..100], &[8192; 100]);
        assert!(output[100..].iter().all(|x| *x == 0));
        converter.push(None, 480);
        converter.frame(&mut output);
        assert_eq!(output, [0; FRAME_LEN]);
        converter.frame(&mut output);
        assert_eq!(output, [0; FRAME_LEN]);
        converter.push(Some(&bytes(&vec![0.5; FRAME_LEN])), 480);
        converter.reset();
        converter.frame(&mut output);
        assert_eq!(output, [0; FRAME_LEN]);
    }
}
