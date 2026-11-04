//! Smoke-style *text* graph: percentile spread, median and loss per second.
//! This is independent of terminal libraries and works on pipes or tiny terminals.

use crate::stats::percentile;
use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct Bucket {
    pub second: u64,
    pub sent: u32,
    pub timeouts: u32,
    pub replies: Vec<f64>,
}

impl Bucket {
    pub fn new(second: u64) -> Self {
        Self {
            second,
            sent: 0,
            timeouts: 0,
            replies: Vec::new(),
        }
    }

    pub fn median(&self) -> f64 {
        percentile(self.replies.clone(), 0.5)
    }

    pub fn p95(&self) -> f64 {
        percentile(self.replies.clone(), 0.95)
    }

    pub fn p10(&self) -> f64 {
        percentile(self.replies.clone(), 0.1)
    }

    pub fn p90(&self) -> f64 {
        percentile(self.replies.clone(), 0.9)
    }

    pub fn pending(&self) -> usize {
        (self.sent as usize).saturating_sub(self.replies.len() + self.timeouts as usize)
    }

    pub fn loss(&self) -> f64 {
        if self.sent == 0 {
            0.0
        } else {
            100.0 * self.timeouts as f64 / self.sent as f64
        }
    }
}

pub struct History {
    buckets: VecDeque<Bucket>,
    capacity: usize,
}

impl History {
    pub fn new(capacity: usize) -> Self {
        Self {
            buckets: VecDeque::new(),
            capacity: capacity.clamp(5, 3600) + 4,
        }
    }

    fn get_mut(&mut self, second: u64) -> Option<&mut Bucket> {
        if self
            .buckets
            .front()
            .is_some_and(|first| second < first.second)
        {
            return None;
        }
        if self.buckets.is_empty() {
            self.buckets.push_back(Bucket::new(second));
        }
        while self.buckets.back().is_some_and(|last| last.second < second) {
            let next = self.buckets.back().map_or(second, |b| b.second + 1);
            self.buckets.push_back(Bucket::new(next));
            while self.buckets.len() > self.capacity {
                self.buckets.pop_front();
            }
        }
        self.buckets.iter_mut().find(|b| b.second == second)
    }

    pub fn on_send(&mut self, second: u64) {
        if let Some(bucket) = self.get_mut(second) {
            bucket.sent += 1;
        }
    }

    pub fn on_reply(&mut self, second: u64, ms: f64) {
        if let Some(bucket) = self.get_mut(second) {
            bucket.replies.push(ms);
        }
    }

    pub fn on_timeout(&mut self, second: u64) {
        if let Some(bucket) = self.get_mut(second) {
            bucket.timeouts += 1;
        }
    }

    fn axis_max(&self) -> f64 {
        self.buckets
            .iter()
            .filter(|b| !b.replies.is_empty())
            .map(Bucket::p95)
            .fold(1.0_f64, f64::max)
            * 1.5
    }

    pub fn line(&self, second: u64, ascii: bool) -> String {
        let bucket = self.buckets.iter().find(|b| b.second == second);
        match bucket {
            Some(b) => format_bucket(b, self.axis_max(), ascii),
            None => format!("t+{second:04}s  |{}|  no probes", " ".repeat(28)),
        }
    }

    pub fn recent(&self, max_lines: usize, ascii: bool) -> String {
        let max = self.axis_max();
        self.buckets
            .iter()
            .rev()
            .take(max_lines)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(|b| format_bucket(b, max, ascii))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn scale(ms: f64, axis: f64) -> usize {
    if axis <= 0.0 {
        return 0;
    }
    ((ms / axis * 27.0).round() as usize).min(27)
}

fn format_bucket(b: &Bucket, max_ms: f64, ascii: bool) -> String {
    let mut cells = [' '; 28];
    if b.replies.is_empty() {
        cells[0] = if b.timeouts > 0 {
            'X'
        } else if b.pending() > 0 {
            '?'
        } else {
            '.'
        };
        return format!(
            "t+{:04}s  |{}|  {:>2}/{:<2}  loss {:>5.1}%  pending {}  no RTT",
            b.second,
            cells.iter().collect::<String>(),
            b.replies.len(),
            b.sent,
            b.loss(),
            b.pending()
        );
    }
    let lo = scale(b.p10(), max_ms);
    let hi = scale(b.p90(), max_ms);
    for cell in cells.iter_mut().take(hi + 1).skip(lo) {
        *cell = if ascii { ':' } else { '▒' };
    }
    let median = scale(b.median(), max_ms);
    cells[median] = if ascii { '*' } else { '●' };
    format!(
        "t+{:04}s  |{}|  {:>2}/{:<2}  loss {:>5.1}%  pending {}  p50 {:>7.3}  p95 {:>7.3} ms",
        b.second,
        cells.iter().collect::<String>(),
        b.replies.len(),
        b.sent,
        b.loss(),
        b.pending(),
        b.median(),
        b.p95()
    )
}

/// Compact per-hop SmokePing-style time series (oldest left, newest right).
/// A lost probe is 'X', a successful RTT is a graded block; data is bounded by the caller.
pub fn sparkline(samples: &VecDeque<Option<f64>>, width: usize, ascii: bool) -> String {
    let width = width.min(96);
    let peak = samples
        .iter()
        .filter_map(|sample| *sample)
        .filter(|ms| ms.is_finite() && *ms >= 0.0)
        .fold(1.0_f64, f64::max);
    let grades: &[char] = if ascii {
        &['_', '.', '-', ':', '=', '+', '*', '#']
    } else {
        &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█']
    };
    let mut output = String::with_capacity(width * 3);
    for sample in samples.iter().skip(samples.len().saturating_sub(width)) {
        match sample {
            Some(ms) if ms.is_finite() && *ms >= 0.0 => {
                let level = ((ms / peak * 7.0).round() as usize).min(7);
                output.push(grades[level]);
            }
            _ => output.push('X'),
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_missing_and_spread_buckets() {
        let mut h = History::new(5);
        h.on_send(0);
        h.on_send(0);
        h.on_reply(0, 10.0);
        assert!(h.line(0, true).contains("pending 1"));
        h.on_timeout(0);
        assert!(h.line(0, true).contains("loss  50.0%"));
        assert!(h.line(0, true).contains('*'));
        h.on_send(1);
        assert!(h.line(1, true).contains("no RTT"));
        assert!(h.line(2, true).contains("no probes"));
    }

    #[test]
    fn hop_smoke_is_bounded_and_shows_loss() {
        let mut series = VecDeque::new();
        for i in 0..100 {
            series.push_back(if i == 99 { None } else { Some(f64::from(i)) });
        }
        assert_eq!(sparkline(&series, 8, true).chars().count(), 8);
        assert!(sparkline(&series, 8, true).ends_with('X'));
        assert!(sparkline(&series, 8, false).contains('▇'));
        assert_eq!(sparkline(&VecDeque::new(), 10, false), "");
    }

    #[test]
    fn history_is_capped_even_if_time_advances() {
        let mut h = History::new(5);
        for t in 0..100 {
            h.on_send(t);
        }
        assert!(h.buckets.len() <= 9);
        assert!(h.buckets.front().unwrap().second > 0);
        assert!(h.recent(5, true).lines().count() <= 5);
        // Old buckets are not reintroduced by delayed responses.
        h.on_reply(0, 2.0);
        assert!(h.buckets.front().unwrap().second > 0);
    }
}
