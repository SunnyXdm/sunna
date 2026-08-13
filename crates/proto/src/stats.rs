//! Minimal latency statistics. Percentiles over collected microsecond samples.

#[derive(Debug, Clone, PartialEq)]
pub struct Percentiles {
    pub count: usize,
    pub mean_us: f64,
    pub p50_us: u64,
    pub p95_us: u64,
    pub p99_us: u64,
    pub max_us: u64,
}

impl Percentiles {
    pub fn from_samples(mut samples: Vec<u64>) -> Option<Self> {
        if samples.is_empty() {
            return None;
        }
        samples.sort_unstable();
        let count = samples.len();
        let sum: u128 = samples.iter().map(|&sample| sample as u128).sum();
        let at = |quantile: f64| -> u64 {
            let index = ((count as f64 - 1.0) * quantile).round() as usize;
            samples[index]
        };
        Some(Self {
            count,
            mean_us: sum as f64 / count as f64,
            p50_us: at(0.50),
            p95_us: at(0.95),
            p99_us: at(0.99),
            max_us: samples[count - 1],
        })
    }
}

impl std::fmt::Display for Percentiles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "n={} mean={:.2}ms p50={:.2}ms p95={:.2}ms p99={:.2}ms max={:.2}ms",
            self.count,
            self.mean_us / 1000.0,
            self.p50_us as f64 / 1000.0,
            self.p95_us as f64 / 1000.0,
            self.p99_us as f64 / 1000.0,
            self.max_us as f64 / 1000.0,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_basic() {
        let stats = Percentiles::from_samples((1..=100).collect()).unwrap();
        assert_eq!(stats.count, 100);
        // Index = round((count-1) * q): round(49.5) = 50 → the value 51.
        assert_eq!(stats.p50_us, 51);
        assert_eq!(stats.p95_us, 95);
        assert_eq!(stats.max_us, 100);
        assert!(Percentiles::from_samples(vec![]).is_none());
    }
}
