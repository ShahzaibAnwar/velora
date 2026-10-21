//! Live, in-memory state of a running download, shared between its workers and the engine.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use super::segment::SegmentTable;
use crate::model::{ProgressUpdate, SegmentView};

pub const HISTORY_LEN: usize = 60;

#[derive(Default)]
pub struct LiveState {
    pub table: SegmentTable,
    single_mode: AtomicBool,
    single_bytes: AtomicU64,
    total: AtomicU64,
    total_known: AtomicBool,
    /// Bytes received from the network during this run (for speed).
    received: AtomicU64,
    connections: AtomicU32,
    meter: Mutex<SpeedMeter>,
}

impl LiveState {
    pub fn set_total(&self, total: Option<u64>) {
        self.total.store(total.unwrap_or(0), Ordering::Release);
        self.total_known.store(total.is_some(), Ordering::Release);
    }

    pub fn total(&self) -> Option<u64> {
        self.total_known.load(Ordering::Acquire).then(|| self.total.load(Ordering::Acquire))
    }

    pub fn set_single(&self, single: bool) {
        self.single_mode.store(single, Ordering::Release);
    }

    pub fn set_single_bytes(&self, n: u64) {
        self.single_bytes.store(n, Ordering::Release);
    }

    pub fn add_received(&self, n: u64) {
        self.received.fetch_add(n, Ordering::Relaxed);
    }

    pub fn conn_opened(&self) {
        self.connections.fetch_add(1, Ordering::Relaxed);
    }

    pub fn conn_closed(&self) {
        self.connections.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn downloaded(&self) -> u64 {
        if self.single_mode.load(Ordering::Acquire) {
            self.single_bytes.load(Ordering::Acquire)
        } else {
            self.table.downloaded()
        }
    }

    pub fn segments(&self) -> Vec<SegmentView> {
        if self.single_mode.load(Ordering::Acquire) {
            match self.total() {
                Some(total) if total > 0 => vec![SegmentView {
                    start: 0,
                    end: total - 1,
                    downloaded: self.downloaded(),
                    active: self.connections.load(Ordering::Relaxed) > 0,
                }],
                _ => Vec::new(),
            }
        } else {
            self.table.views()
        }
    }

    /// Sample the counters; call on a regular tick. Returns a progress snapshot.
    pub fn sample(&self, id: &str, now: Instant) -> ProgressUpdate {
        let received = self.received.load(Ordering::Relaxed);
        let speed = self.meter.lock().unwrap().tick(received, now);
        self.snapshot(id, speed)
    }

    /// A progress snapshot using the most recent speed, without advancing the meter.
    pub fn sample_peek(&self, id: &str) -> ProgressUpdate {
        let speed = self.meter.lock().unwrap().smoothed as u64;
        self.snapshot(id, speed)
    }

    fn snapshot(&self, id: &str, speed: u64) -> ProgressUpdate {
        let downloaded = self.downloaded();
        let total = self.total();
        let eta_secs = match (total, speed) {
            (Some(t), s) if s > 0 && t > downloaded => Some((t - downloaded).div_ceil(s)),
            _ => None,
        };
        ProgressUpdate {
            id: id.to_string(),
            downloaded,
            total_size: total,
            speed,
            eta_secs,
            connections: self.connections.load(Ordering::Relaxed),
        }
    }

    pub fn speed_history(&self) -> Vec<u64> {
        self.meter.lock().unwrap().history.iter().copied().collect()
    }
}

struct SpeedMeter {
    last_received: u64,
    last_at: Option<Instant>,
    smoothed: f64,
    history: VecDeque<u64>,
    window_start: Option<Instant>,
    window_bytes: u64,
}

impl Default for SpeedMeter {
    fn default() -> Self {
        Self {
            last_received: 0,
            last_at: None,
            smoothed: 0.0,
            history: VecDeque::with_capacity(HISTORY_LEN),
            window_start: None,
            window_bytes: 0,
        }
    }
}

impl SpeedMeter {
    /// Exponentially smoothed bytes/s. One history sample is recorded per elapsed second.
    fn tick(&mut self, received: u64, now: Instant) -> u64 {
        let delta = received.saturating_sub(self.last_received);
        self.last_received = received;
        if let Some(prev) = self.last_at {
            let dt = now.duration_since(prev).as_secs_f64();
            if dt > 0.0 {
                let instant = delta as f64 / dt;
                self.smoothed = if self.smoothed == 0.0 { instant } else { self.smoothed * 0.6 + instant * 0.4 };
                if self.smoothed < 1.0 {
                    self.smoothed = 0.0;
                }
            }
        }
        self.last_at = Some(now);

        let start = *self.window_start.get_or_insert(now);
        self.window_bytes += delta;
        let elapsed = now.duration_since(start).as_secs_f64();
        if elapsed >= 1.0 {
            if self.history.len() == HISTORY_LEN {
                self.history.pop_front();
            }
            self.history.push_back((self.window_bytes as f64 / elapsed) as u64);
            self.window_start = Some(now);
            self.window_bytes = 0;
        }
        self.smoothed as u64
    }
}
