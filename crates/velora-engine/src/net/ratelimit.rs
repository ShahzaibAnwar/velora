//! Token-bucket bandwidth limiter shared by all connections it governs.
//!
//! Workers call [`RateLimiter::acquire`] for every chunk *before* reading the next one, so a limited
//! download stops pulling from the socket and TCP flow control slows the sender. That is real
//! throttling, not a cosmetic cap on the displayed speed.

use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct RateLimiter {
    state: Mutex<State>,
}

struct State {
    /// Bytes per second; 0 = unlimited.
    rate: u64,
    /// May go negative: a debt the caller sleeps off.
    tokens: f64,
    last: Instant,
}

impl RateLimiter {
    pub fn new(rate: u64) -> Self {
        Self { state: Mutex::new(State { rate, tokens: 0.0, last: Instant::now() }) }
    }

    pub fn rate(&self) -> u64 {
        self.state.lock().unwrap().rate
    }

    pub fn set_rate(&self, rate: u64) {
        let mut s = self.state.lock().unwrap();
        if s.rate != rate {
            s.rate = rate;
            s.tokens = 0.0;
            s.last = Instant::now();
        }
    }

    /// How long the caller must wait after consuming `n` bytes.
    fn reserve(&self, n: usize) -> Option<Duration> {
        let mut s = self.state.lock().unwrap();
        if s.rate == 0 {
            return None;
        }
        let now = Instant::now();
        let rate = s.rate as f64;
        // Allow a burst of at most 1/4 s worth of data so short pauses don't turn into spikes.
        let burst = (rate / 4.0).max(16.0 * 1024.0);
        s.tokens = (s.tokens + now.duration_since(s.last).as_secs_f64() * rate).min(burst);
        s.last = now;
        s.tokens -= n as f64;
        (s.tokens < 0.0).then(|| Duration::from_secs_f64(-s.tokens / rate))
    }

    pub async fn acquire(&self, n: usize) {
        if let Some(wait) = self.reserve(n) {
            tokio::time::sleep(wait).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unlimited_never_waits() {
        let l = RateLimiter::new(0);
        assert!(l.reserve(10_000_000).is_none());
    }

    #[test]
    fn debt_produces_proportional_wait() {
        let l = RateLimiter::new(100_000);
        // The bucket starts empty, so taking 125 KB at 100 KB/s creates ~1.25 s of debt.
        let wait = l.reserve(125_000).unwrap();
        assert!(wait > Duration::from_millis(900) && wait <= Duration::from_millis(1250), "{wait:?}");
    }
}
