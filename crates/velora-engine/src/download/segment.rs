//! Byte-range segments and the work-stealing splitter.
//!
//! A segment owns the inclusive range `[start, end]`. Its owner claims bytes from `pos` upwards; a
//! thief may only *lower* `end`, and only to a point well beyond `pos`. Both happen under the same
//! mutex, so two connections can never claim the same byte.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::model::SegmentView;

/// Smallest segment created when splitting a file up front.
pub const MIN_SEGMENT: u64 = 1024 * 1024;
/// A running segment is only split when at least twice this much remains.
pub const MIN_SPLIT: u64 = 512 * 1024;

#[derive(Debug)]
struct Range {
    /// Next byte to claim.
    pos: u64,
    /// Last byte (inclusive) this segment is responsible for.
    end: u64,
}

#[derive(Debug)]
pub struct Segment {
    pub start: u64,
    range: Mutex<Range>,
    /// Absolute offset up to which data has been handed to the OS (exclusive).
    written_to: AtomicU64,
    active: AtomicBool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentState {
    pub start: u64,
    pub end: u64,
    /// Bytes from `start` that are safely written.
    pub written: u64,
}

impl Segment {
    pub fn new(start: u64, end: u64, written: u64) -> Self {
        let pos = start + written;
        Self { start, range: Mutex::new(Range { pos, end }), written_to: AtomicU64::new(pos), active: AtomicBool::new(false) }
    }

    /// Claim up to `n` bytes. Returns the absolute offset and how many bytes were granted (0 = done).
    pub fn claim(&self, n: usize) -> (u64, usize) {
        let mut r = self.range.lock().unwrap();
        if r.pos > r.end {
            return (r.pos, 0);
        }
        let take = (r.end + 1 - r.pos).min(n as u64);
        let offset = r.pos;
        r.pos += take;
        (offset, take as usize)
    }

    /// The range still to be requested, or `None` when every byte has been claimed.
    pub fn pending(&self) -> Option<(u64, u64)> {
        let r = self.range.lock().unwrap();
        (r.pos <= r.end).then_some((r.pos, r.end))
    }

    pub fn end(&self) -> u64 {
        self.range.lock().unwrap().end
    }

    pub fn claimed(&self) -> u64 {
        self.range.lock().unwrap().pos - self.start
    }

    pub fn mark_written(&self, upto: u64) {
        self.written_to.fetch_max(upto, Ordering::AcqRel);
    }

    pub fn written(&self) -> u64 {
        self.written_to.load(Ordering::Acquire) - self.start
    }

    pub fn is_done(&self) -> bool {
        let end = self.end();
        self.written_to.load(Ordering::Acquire) > end
    }

    pub fn set_active(&self, v: bool) {
        self.active.store(v, Ordering::Release);
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }

    pub fn state(&self) -> SegmentState {
        let end = self.end();
        SegmentState { start: self.start, end, written: self.written().min(end + 1 - self.start) }
    }

    pub fn view(&self) -> SegmentView {
        let r = self.range.lock().unwrap();
        SegmentView {
            start: self.start,
            end: r.end,
            downloaded: (r.pos - self.start).min(r.end + 1 - self.start),
            active: self.is_active(),
        }
    }
}

#[derive(Default)]
pub struct SegmentTable {
    segs: Mutex<Vec<Arc<Segment>>>,
}

impl SegmentTable {
    pub fn set(&self, segs: Vec<Arc<Segment>>) {
        *self.segs.lock().unwrap() = segs;
    }

    pub fn is_empty(&self) -> bool {
        self.segs.lock().unwrap().is_empty()
    }

    /// Hand out the next piece of work: an unowned unfinished segment if there is one, otherwise the
    /// second half of the largest running segment. Returns `None` when there is nothing worth taking.
    pub fn next_work(&self) -> Option<Arc<Segment>> {
        let mut segs = self.segs.lock().unwrap();
        if let Some(s) = segs.iter().find(|s| !s.is_active() && s.pending().is_some()) {
            s.set_active(true);
            return Some(s.clone());
        }
        let (victim, remaining) = segs
            .iter()
            .filter(|s| s.is_active())
            .filter_map(|s| s.pending().map(|(pos, end)| (s.clone(), end + 1 - pos)))
            .max_by_key(|(_, rem)| *rem)?;
        if remaining < 2 * MIN_SPLIT {
            return None;
        }
        let new_seg = {
            let mut r = victim.range.lock().unwrap();
            // Re-check under the lock: the owner may have advanced since we looked.
            let remaining = r.end.checked_add(1)?.checked_sub(r.pos)?;
            if remaining < 2 * MIN_SPLIT {
                return None;
            }
            let mid = r.pos + remaining / 2;
            let old_end = r.end;
            r.end = mid - 1;
            Arc::new(Segment::new(mid, old_end, 0))
        };
        new_seg.set_active(true);
        segs.push(new_seg.clone());
        Some(new_seg)
    }

    /// Bytes received and claimed (includes data still in write buffers).
    pub fn downloaded(&self) -> u64 {
        self.segs.lock().unwrap().iter().map(|s| s.view().downloaded).sum()
    }

    pub fn all_done(&self) -> bool {
        self.segs.lock().unwrap().iter().all(|s| s.is_done())
    }

    /// Durable state, sorted by offset, for checkpoints.
    pub fn states(&self) -> Vec<SegmentState> {
        let mut v: Vec<_> = self.segs.lock().unwrap().iter().map(|s| s.state()).collect();
        v.sort_by_key(|s| s.start);
        v
    }

    pub fn views(&self) -> Vec<SegmentView> {
        let mut v: Vec<_> = self.segs.lock().unwrap().iter().map(|s| s.view()).collect();
        v.sort_by_key(|s| s.start);
        v
    }
}

/// Split `size` bytes into at most `connections` initial segments of at least [`MIN_SEGMENT`].
pub fn initial_segments(size: u64, connections: u8) -> Vec<Arc<Segment>> {
    let n = (size / MIN_SEGMENT).clamp(1, connections.max(1) as u64);
    let len = size / n;
    (0..n)
        .map(|i| {
            let start = i * len;
            let end = if i == n - 1 { size - 1 } else { start + len - 1 };
            Arc::new(Segment::new(start, end, 0))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_split_covers_file() {
        let segs = initial_segments(10 * MIN_SEGMENT + 7, 4);
        assert_eq!(segs.len(), 4);
        assert_eq!(segs[0].start, 0);
        for w in segs.windows(2) {
            assert_eq!(w[0].end() + 1, w[1].start);
        }
        assert_eq!(segs[3].end(), 10 * MIN_SEGMENT + 6);
        assert_eq!(initial_segments(100, 8).len(), 1);
    }

    #[test]
    fn claims_never_overlap_after_steal() {
        let table = SegmentTable::default();
        table.set(vec![Arc::new(Segment::new(0, 8 * MIN_SPLIT - 1, 0))]);
        let a = table.next_work().unwrap();
        let (off, take) = a.claim(1000);
        assert_eq!((off, take), (0, 1000));
        let b = table.next_work().expect("large remainder should be split");
        assert!(b.start > 1000);
        assert_eq!(a.end() + 1, b.start);
        // Owner A can never claim into B's range.
        let mut total = 1000usize;
        loop {
            let (_, t) = a.claim(64 * 1024);
            if t == 0 {
                break;
            }
            total += t;
        }
        assert_eq!(total as u64, b.start);
    }

    #[test]
    fn no_split_when_small() {
        let table = SegmentTable::default();
        table.set(vec![Arc::new(Segment::new(0, MIN_SPLIT, 0))]);
        let _a = table.next_work().unwrap();
        assert!(table.next_work().is_none());
    }
}
