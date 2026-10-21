//! A controllable local HTTP server and helpers for engine integration tests.
#![allow(dead_code)]

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::extract::{Path as AxPath, State};
use axum::http::{header, HeaderMap, Response, StatusCode};
use axum::routing::get;
use axum::Router;
use futures_util::stream::{self, StreamExt};
use velora_engine::{DownloadRecord, DownloadStatus, Engine, EngineConfig, EngineEvent, Settings};

/// How the server should treat requests for one path.
#[derive(Clone, Default)]
pub struct Spec {
    pub data: Arc<Vec<u8>>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    /// Honour `Range` headers.
    pub ranges: bool,
    /// Omit Content-Length (chunked transfer encoding).
    pub chunked: bool,
    /// Answer the first `fail_first` requests with `fail_status`.
    pub fail_first: u32,
    pub fail_status: u16,
    pub retry_after: Option<u32>,
    /// For the first `drop_times` body requests, abort the connection after this many bytes.
    pub drop_after: Option<usize>,
    pub drop_times: u32,
    /// Answer ranged requests (other than the 0-0 probe) with a shifted Content-Range.
    pub wrong_range: bool,
    /// Stall the first data (non-probe) response body for this long.
    pub stall_first: Option<Duration>,
    pub redirect_to: Option<String>,
    /// Delay between 16 KiB chunks, to make downloads slow enough to interrupt.
    pub chunk_delay: Option<Duration>,
}

#[derive(Default)]
pub struct Counters {
    pub requests: AtomicU32,
    pub ranged: AtomicU32,
    pub body_requests: AtomicU32,
    pub bytes_sent: AtomicU64,
}

#[derive(Clone, Default)]
pub struct ServerState {
    specs: Arc<Mutex<HashMap<String, Spec>>>,
    counters: Arc<Mutex<HashMap<String, Arc<Counters>>>>,
}

pub struct TestServer {
    pub base: String,
    state: ServerState,
}

impl TestServer {
    pub async fn start() -> TestServer {
        let state = ServerState::default();
        let app = Router::new().route("/{*path}", get(handler)).with_state(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        TestServer { base: format!("http://{addr}"), state }
    }

    pub fn add(&self, path: &str, spec: Spec) -> String {
        self.state.specs.lock().unwrap().insert(path.to_string(), spec);
        self.state.counters.lock().unwrap().insert(path.to_string(), Arc::new(Counters::default()));
        format!("{}/{}", self.base, path)
    }

    pub fn update(&self, path: &str, f: impl FnOnce(&mut Spec)) {
        f(self.state.specs.lock().unwrap().get_mut(path).unwrap());
    }

    pub fn counters(&self, path: &str) -> Arc<Counters> {
        self.state.counters.lock().unwrap().get(path).unwrap().clone()
    }
}

pub fn pattern(len: usize, seed: u8) -> Vec<u8> {
    // Position-dependent bytes so any misplaced range shows up as corruption.
    (0..len).map(|i| ((i as u64).wrapping_mul(2654435761) >> 7) as u8 ^ seed).collect()
}

pub fn file_spec(len: usize) -> Spec {
    Spec { data: Arc::new(pattern(len, 0)), etag: Some("\"v1\"".into()), ranges: true, ..Default::default() }
}

fn parse_range(h: &str, len: u64) -> Option<(u64, u64)> {
    let spec = h.strip_prefix("bytes=")?;
    let (a, b) = spec.split_once('-')?;
    let start: u64 = a.parse().ok()?;
    let end: u64 = if b.is_empty() { len.checked_sub(1)? } else { b.parse::<u64>().ok()?.min(len.checked_sub(1)?) };
    (start <= end).then_some((start, end))
}

async fn handler(State(state): State<ServerState>, AxPath(path): AxPath<String>, headers: HeaderMap) -> Response<Body> {
    let Some(spec) = state.specs.lock().unwrap().get(&path).cloned() else {
        return Response::builder().status(404).body(Body::empty()).unwrap();
    };
    let counters = state.counters.lock().unwrap().get(&path).unwrap().clone();
    let n = counters.requests.fetch_add(1, Ordering::SeqCst);

    if let Some(to) = &spec.redirect_to {
        return Response::builder().status(302).header(header::LOCATION, to.as_str()).body(Body::empty()).unwrap();
    }
    if n < spec.fail_first {
        let mut b = Response::builder().status(spec.fail_status);
        if let Some(ra) = spec.retry_after {
            b = b.header(header::RETRY_AFTER, ra.to_string());
        }
        return b.body(Body::empty()).unwrap();
    }

    let len = spec.data.len() as u64;
    let mut range = None;
    if spec.ranges {
        if let Some(r) = headers.get(header::RANGE).and_then(|v| v.to_str().ok()) {
            let if_range_ok = match headers.get(header::IF_RANGE).and_then(|v| v.to_str().ok()) {
                None => true,
                Some(v) => Some(v) == spec.etag.as_deref() || Some(v) == spec.last_modified.as_deref(),
            };
            if if_range_ok {
                range = parse_range(r, len);
                if range.is_none() {
                    return Response::builder()
                        .status(416)
                        .header(header::CONTENT_RANGE, format!("bytes */{len}"))
                        .body(Body::empty())
                        .unwrap();
                }
            }
        }
    }

    let (status, start, end) = match range {
        Some((s, e)) => (StatusCode::PARTIAL_CONTENT, s, e),
        None if len == 0 => (StatusCode::OK, 0, 0),
        None => (StatusCode::OK, 0, len - 1),
    };
    let body_len = if len == 0 { 0 } else { end - start + 1 };
    if range.is_some() {
        counters.ranged.fetch_add(1, Ordering::SeqCst);
    }
    let is_probe = range == Some((0, 0));
    if !is_probe {
        counters.body_requests.fetch_add(1, Ordering::SeqCst);
    }

    let mut b = Response::builder().status(status).header(header::ACCEPT_RANGES, if spec.ranges { "bytes" } else { "none" });
    if let Some(e) = &spec.etag {
        b = b.header(header::ETAG, e.as_str());
    }
    if let Some(lm) = &spec.last_modified {
        b = b.header(header::LAST_MODIFIED, lm.as_str());
    }
    if range.is_some() {
        let shown_start = if spec.wrong_range && !is_probe { start + 1 } else { start };
        let shown_end = if spec.wrong_range && !is_probe { (end + 1).min(len - 1) } else { end };
        b = b.header(header::CONTENT_RANGE, format!("bytes {shown_start}-{shown_end}/{len}"));
    }
    if !spec.chunked {
        b = b.header(header::CONTENT_LENGTH, body_len.to_string());
    }

    let body_index = counters.body_requests.load(Ordering::SeqCst);
    let drop_after = spec.drop_after.filter(|_| !is_probe && body_index <= spec.drop_times);
    let stall = spec.stall_first.filter(|_| !is_probe && body_index == 1);
    let data = spec.data.clone();
    let delay = spec.chunk_delay;
    let sent = counters.clone();

    let chunks: Vec<(u64, u64)> = {
        let mut v = Vec::new();
        let mut p = start;
        while len > 0 && p <= end {
            let e = (p + 16 * 1024 - 1).min(end);
            v.push((p, e));
            p = e + 1;
        }
        v
    };
    let stream = stream::iter(chunks.into_iter().enumerate()).then(move |(i, (s, e))| {
        let data = data.clone();
        let sent = sent.clone();
        async move {
            if i == 0 {
                if let Some(st) = stall {
                    tokio::time::sleep(st).await;
                }
            }
            if let Some(d) = delay {
                tokio::time::sleep(d).await;
            }
            if let Some(limit) = drop_after {
                if (s - start) as usize >= limit {
                    return Err(std::io::Error::other("simulated connection drop"));
                }
            }
            sent.bytes_sent.fetch_add(e - s + 1, Ordering::SeqCst);
            Ok::<Bytes, std::io::Error>(Bytes::copy_from_slice(&data[s as usize..=e as usize]))
        }
    });
    // Stop the stream after the first error so hyper aborts the connection.
    let stream = stream.scan(false, |failed, item| {
        let out = if *failed { None } else { Some(item) };
        if let Some(Err(_)) = &out {
            *failed = true;
        }
        futures_util::future::ready(out)
    });
    b.body(Body::from_stream(stream)).unwrap()
}

// -- Engine helpers ------------------------------------------------------------------------------

pub struct Harness {
    pub engine: Engine,
    pub dir: tempfile::TempDir,
    pub events: Arc<Mutex<Vec<EngineEvent>>>,
}

pub fn test_settings(dir: &Path) -> Settings {
    Settings {
        default_dir: dir.to_string_lossy().into_owned(),
        max_retries: 3,
        retry_max_delay_secs: 1,
        connect_timeout_secs: 5,
        read_timeout_secs: 5,
        overwrite_policy: velora_engine::settings::OverwritePolicy::Rename,
        ..Settings::default()
    }
}

pub async fn open_engine(
    db: Option<&Path>,
    dir: &Path,
    fault: Option<velora_engine::download::WriteFault>,
) -> (Engine, Arc<Mutex<Vec<EngineEvent>>>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let mut config = EngineConfig::new(db.map(Path::to_path_buf), dir.to_path_buf(), Arc::new(move |e| sink.lock().unwrap().push(e)));
    config.tick = Duration::from_millis(100);
    config.write_fault = fault;
    let engine = Engine::open(config).await.unwrap();
    let mut s = test_settings(dir);
    let existing = engine.settings();
    s.max_active = existing.max_active;
    engine.update_settings(s).unwrap();
    (engine, events)
}

pub async fn harness() -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = open_engine(None, dir.path(), None).await;
    Harness { engine, dir, events }
}

pub async fn wait_for(engine: &Engine, id: &str, timeout: Duration, pred: impl Fn(&DownloadRecord) -> bool) -> DownloadRecord {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let rec = engine.get(id).expect("record exists");
        if pred(&rec) {
            return rec;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("timed out waiting; last state: {:?} error={:?} downloaded={}", rec.status, rec.error, rec.downloaded);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

pub async fn wait_status(engine: &Engine, id: &str, status: DownloadStatus, timeout: Duration) -> DownloadRecord {
    wait_for(engine, id, timeout, |r| r.status == status).await
}

pub async fn wait_finished(engine: &Engine, id: &str) -> DownloadRecord {
    wait_for(engine, id, Duration::from_secs(60), |r| r.status.is_finished()).await
}

/// Wait until a running download has at least `bytes` on record (via live progress).
pub async fn wait_progress(engine: &Engine, id: &str, bytes: u64) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(d) = engine.details(id) {
            if d.progress.as_ref().is_some_and(|p| p.downloaded >= bytes) {
                return;
            }
        }
        assert!(tokio::time::Instant::now() < deadline, "no progress");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
