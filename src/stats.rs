//! Online statistics (bounded sample reservoir for percentiles) and deterministic tests.

use std::collections::VecDeque;

const SAMPLE_LIMIT: usize = 2048;

#[derive(Clone, Debug)]
pub struct Stats {
    pub sent: u64,
    pub received: u64,
    pub duplicates: u64,
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    m2: f64,
    prev: Option<f64>,
    jitter_sum: f64,
    jitter_pairs: u64,
    samples: VecDeque<f64>,
    keep_samples: bool,
}

impl Default for Stats {
    fn default() -> Self {
        Self {
            sent: 0,
            received: 0,
            duplicates: 0,
            min: 0.0,
            max: 0.0,
            mean: 0.0,
            m2: 0.0,
            prev: None,
            jitter_sum: 0.0,
            jitter_pairs: 0,
            samples: VecDeque::new(),
            keep_samples: true,
        }
    }
}

impl Stats {
    /// Sweeps with many hosts need min/avg/max, not a 2048-sample history per IP.
    pub fn for_sweep() -> Self {
        Self {
            keep_samples: false,
            ..Self::default()
        }
    }

    pub fn on_send(&mut self) {
        self.sent += 1;
    }

    pub fn on_reply(&mut self, ms: f64) {
        if !ms.is_finite() || ms < 0.0 {
            return;
        }
        self.received += 1;
        if self.received == 1 {
            self.min = ms;
            self.max = ms;
        } else {
            self.min = self.min.min(ms);
            self.max = self.max.max(ms);
        }
        let delta = ms - self.mean;
        self.mean += delta / self.received as f64;
        self.m2 += delta * (ms - self.mean);
        if let Some(prev) = self.prev {
            self.jitter_sum += (ms - prev).abs();
            self.jitter_pairs += 1;
        }
        self.prev = Some(ms);
        if self.keep_samples {
            if self.samples.len() == SAMPLE_LIMIT {
                self.samples.pop_front();
            }
            self.samples.push_back(ms);
        }
    }

    pub fn loss_percent(&self) -> f64 {
        if self.sent == 0 {
            0.0
        } else {
            100.0 * self.sent.saturating_sub(self.received) as f64 / self.sent as f64
        }
    }

    pub fn mdev(&self) -> f64 {
        if self.received == 0 {
            0.0
        } else {
            (self.m2 / self.received as f64).sqrt()
        }
    }

    pub fn jitter(&self) -> f64 {
        if self.jitter_pairs == 0 {
            0.0
        } else {
            self.jitter_sum / self.jitter_pairs as f64
        }
    }

    pub fn percentile(&self, fraction: f64) -> f64 {
        percentile(self.samples.iter().copied().collect(), fraction)
    }
}

pub fn percentile(mut samples: Vec<f64>, fraction: f64) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    samples.sort_by(f64::total_cmp);
    let p = fraction.clamp(0.0, 1.0) * (samples.len() - 1) as f64;
    let lower = p.floor() as usize;
    let upper = p.ceil() as usize;
    let weight = p - lower as f64;
    samples[lower] * (1.0 - weight) + samples[upper] * weight
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_samples_are_well_defined() {
        let s = Stats::default();
        assert_eq!(s.loss_percent(), 0.0);
        assert_eq!(s.mdev(), 0.0);
        assert_eq!(s.percentile(0.95), 0.0);
    }

    #[test]
    fn loss_welford_jitter_and_percentile() {
        let mut s = Stats::default();
        for _ in 0..4 {
            s.on_send();
        }
        for n in [1.0, 2.0, 3.0] {
            s.on_reply(n);
        }
        assert_eq!(s.received, 3);
        assert!((s.loss_percent() - 25.0).abs() < 1e-9);
        assert!((s.mean - 2.0).abs() < 1e-9);
        assert!((s.mdev() - (2.0_f64 / 3.0).sqrt()).abs() < 1e-9);
        assert_eq!(s.jitter(), 1.0);
        assert_eq!(s.percentile(0.5), 2.0);
        assert!((s.percentile(0.95) - 2.9).abs() < 1e-9);
    }

    #[test]
    fn range_statistics_never_allocate_percentile_history() {
        let mut s = Stats::for_sweep();
        for n in 0..10_000 {
            s.on_send();
            s.on_reply(n as f64);
        }
        assert_eq!(s.received, 10_000);
        assert!(s.samples.is_empty());
        assert!((s.mean - 4_999.5).abs() < 1e-9);
    }

    #[test]
    fn percentiles_remain_memory_bounded() {
        let mut s = Stats::default();
        for n in 0..10_000 {
            s.on_send();
            s.on_reply(n as f64);
        }
        assert_eq!(s.samples.len(), SAMPLE_LIMIT);
        assert_eq!(s.received, 10_000);
    }
}
