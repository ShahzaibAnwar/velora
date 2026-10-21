//! Engine integration tests against a controllable local HTTP server.
//!
//! Each test maps to a scenario from the project brief (numbered in the test name).

mod common;

use std::io;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::*;
use velora_engine::download::checkpoint;
use velora_engine::{AddRequest, ChecksumState, DownloadStatus, EngineEvent, StartMode};

fn add(url: &str) -> AddRequest {
    AddRequest { url: url.to_string(), start: StartMode::Now, ..Default::default() }
}

fn read(path: std::path::PathBuf) -> Vec<u8> {
    std::fs::read(path).expect("downloaded file exists")
}

fn leftovers(dir: &std::path::Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains(".velora"))
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn s01_normal_download() {
    let server = TestServer::start().await;
    let spec = file_spec(3 * 1024 * 1024 + 123);
    let data = spec.data.clone();
    let url = server.add("files/normal.bin", spec);
    let h = harness().await;

    let rec = h.engine.add(add(&url)).unwrap();
    let rec = wait_finished(&h.engine, &rec.id).await;
    assert_eq!(rec.status, DownloadStatus::Completed, "{:?}", rec.error);
    assert_eq!(rec.file_name, "normal.bin");
    assert!(rec.resumable);
    assert_eq!(rec.total_size, Some(data.len() as u64));
    assert_eq!(read(rec.target_path()), *data);
    assert!(leftovers(h.dir.path()).is_empty(), "partial files left behind");
    let events = h.events.lock().unwrap();
    assert!(events.iter().any(|e| matches!(e, EngineEvent::Completed { .. })));
}

#[tokio::test(flavor = "multi_thread")]
async fn s02_very_large_download_segmented() {
    let server = TestServer::start().await;
    let spec = file_spec(48 * 1024 * 1024 + 7);
    let data = spec.data.clone();
    let url = server.add("big.iso", spec);
    let h = harness().await;

    let rec = h.engine.add(AddRequest { connections: Some(8), ..add(&url) }).unwrap();
    let rec = wait_finished(&h.engine, &rec.id).await;
    assert_eq!(rec.status, DownloadStatus::Completed, "{:?}", rec.error);
    let got = read(rec.target_path());
    assert_eq!(got.len(), data.len());
    assert!(got == *data, "content mismatch");
}

#[tokio::test(flavor = "multi_thread")]
async fn s03_unknown_content_length() {
    let server = TestServer::start().await;
    let spec = Spec { chunked: true, ranges: false, ..file_spec(700_000) };
    let data = spec.data.clone();
    let url = server.add("stream.dat", spec);
    let h = harness().await;

    let rec = h.engine.add(add(&url)).unwrap();
    let rec = wait_finished(&h.engine, &rec.id).await;
    assert_eq!(rec.status, DownloadStatus::Completed, "{:?}", rec.error);
    assert!(!rec.resumable);
    assert_eq!(read(rec.target_path()), *data);
}

#[tokio::test(flavor = "multi_thread")]
async fn s04_range_supported_uses_multiple_connections() {
    let server = TestServer::start().await;
    let spec = file_spec(8 * 1024 * 1024);
    let data = spec.data.clone();
    let url = server.add("multi.bin", spec);
    let h = harness().await;

    let rec = h.engine.add(AddRequest { connections: Some(4), ..add(&url) }).unwrap();
    let rec = wait_finished(&h.engine, &rec.id).await;
    assert_eq!(rec.status, DownloadStatus::Completed, "{:?}", rec.error);
    assert_eq!(read(rec.target_path()), *data);
    // One probe plus at least four segment requests.
    assert!(server.counters("multi.bin").ranged.load(Ordering::SeqCst) >= 5);
}

#[tokio::test(flavor = "multi_thread")]
async fn s05_server_ignores_range() {
    let server = TestServer::start().await;
    let spec = Spec { ranges: false, ..file_spec(2 * 1024 * 1024) };
    let data = spec.data.clone();
    let url = server.add("noranges.bin", spec);
    let h = harness().await;

    let rec = h.engine.add(add(&url)).unwrap();
    let rec = wait_finished(&h.engine, &rec.id).await;
    assert_eq!(rec.status, DownloadStatus::Completed, "{:?}", rec.error);
    assert!(!rec.resumable, "must not claim resume without a verified 206");
    assert_eq!(read(rec.target_path()), *data);
}

#[tokio::test(flavor = "multi_thread")]
async fn s06_incorrect_content_range_is_never_stitched() {
    let server = TestServer::start().await;
    let spec = Spec { wrong_range: true, ..file_spec(3 * 1024 * 1024) };
    let url = server.add("wrong.bin", spec);
    let h = harness().await;

    let rec = h.engine.add(add(&url)).unwrap();
    let rec = wait_finished(&h.engine, &rec.id).await;
    assert_eq!(rec.status, DownloadStatus::Failed);
    assert_eq!(rec.error.as_ref().unwrap().code, "bad_range");
    assert!(!h.dir.path().join("wrong.bin").exists(), "no completed file may exist");
}

#[tokio::test(flavor = "multi_thread")]
async fn s07_connection_interruption_recovers() {
    let server = TestServer::start().await;
    let spec = Spec { drop_after: Some(200 * 1024), drop_times: 3, ..file_spec(4 * 1024 * 1024) };
    let data = spec.data.clone();
    let url = server.add("drop.bin", spec);
    let h = harness().await;

    let rec = h.engine.add(AddRequest { connections: Some(2), ..add(&url) }).unwrap();
    let rec = wait_finished(&h.engine, &rec.id).await;
    assert_eq!(rec.status, DownloadStatus::Completed, "{:?}", rec.error);
    assert_eq!(read(rec.target_path()), *data);
}

#[tokio::test(flavor = "multi_thread")]
async fn s08_resume_after_restart() {
    let server = TestServer::start().await;
    let size = 4 * 1024 * 1024;
    let spec = Spec { chunk_delay: Some(Duration::from_millis(15)), ..file_spec(size) };
    let data = spec.data.clone();
    let url = server.add("resume.bin", spec);
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("velora.db");
    let downloads = dir.path().join("dl");
    std::fs::create_dir_all(&downloads).unwrap();

    let (engine, _) = open_engine(Some(&db), &downloads, None).await;
    let rec = engine.add(AddRequest { connections: Some(4), ..add(&url) }).unwrap();
    wait_progress(&engine, &rec.id, 1024 * 1024).await;
    engine.shutdown().await;
    let saved = engine.get(&rec.id).unwrap();
    assert_eq!(saved.status, DownloadStatus::Queued, "running downloads are re-queued on shutdown");
    drop(engine);
    let sent_before = server.counters("resume.bin").bytes_sent.load(Ordering::SeqCst);

    // Speed the server up and start a fresh engine on the same database.
    server.update("resume.bin", |s| s.chunk_delay = None);
    let (engine, _) = open_engine(Some(&db), &downloads, None).await;
    let reopened = engine.get(&rec.id).unwrap();
    assert!(reopened.downloaded > 0, "checkpointed progress survives the restart");
    let rec = wait_finished(&engine, &rec.id).await;
    assert_eq!(rec.status, DownloadStatus::Completed, "{:?}", rec.error);
    assert_eq!(read(rec.target_path()), *data);
    let total_sent = server.counters("resume.bin").bytes_sent.load(Ordering::SeqCst);
    assert!(
        total_sent < size as u64 + sent_before,
        "resume must not download everything again (sent {total_sent}, before restart {sent_before})"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn s09_etag_change_blocks_resume_until_restart() {
    let server = TestServer::start().await;
    let spec = Spec { chunk_delay: Some(Duration::from_millis(15)), ..file_spec(3 * 1024 * 1024) };
    let url = server.add("changing.bin", spec);
    let h = harness().await;

    let rec = h.engine.add(add(&url)).unwrap();
    wait_progress(&h.engine, &rec.id, 512 * 1024).await;
    h.engine.pause(&rec.id).unwrap();
    wait_status(&h.engine, &rec.id, DownloadStatus::Paused, Duration::from_secs(10)).await;

    let new_data = Arc::new(pattern(3 * 1024 * 1024, 0x5A));
    server.update("changing.bin", |s| {
        s.data = new_data.clone();
        s.etag = Some("\"v2\"".into());
        s.chunk_delay = None;
    });
    h.engine.resume(&rec.id).unwrap();
    let failed = wait_finished(&h.engine, &rec.id).await;
    assert_eq!(failed.status, DownloadStatus::Failed);
    assert_eq!(failed.error.as_ref().unwrap().code, "remote_changed");

    h.engine.restart(&rec.id).unwrap();
    let done = wait_status(&h.engine, &rec.id, DownloadStatus::Completed, Duration::from_secs(30)).await;
    assert_eq!(read(done.target_path()), *new_data, "only the new version's bytes may be in the file");
}

#[tokio::test(flavor = "multi_thread")]
async fn s10_redirects() {
    let server = TestServer::start().await;
    let spec = file_spec(1024 * 1024);
    let data = spec.data.clone();
    let target = server.add("real/target.zip", spec);
    let url = server.add("go", Spec { redirect_to: Some(target.clone()), ..Default::default() });
    let h = harness().await;

    let rec = h.engine.add(add(&url)).unwrap();
    let rec = wait_finished(&h.engine, &rec.id).await;
    assert_eq!(rec.status, DownloadStatus::Completed, "{:?}", rec.error);
    assert_eq!(rec.final_url.as_deref(), Some(target.as_str()));
    assert_eq!(rec.file_name, "target.zip");
    assert_eq!(read(rec.target_path()), *data);
}

#[tokio::test(flavor = "multi_thread")]
async fn s11_temporary_503() {
    let server = TestServer::start().await;
    let spec = Spec { fail_first: 2, fail_status: 503, ..file_spec(600_000) };
    let data = spec.data.clone();
    let url = server.add("busy.bin", spec);
    let h = harness().await;

    let rec = h.engine.add(add(&url)).unwrap();
    let rec = wait_finished(&h.engine, &rec.id).await;
    assert_eq!(rec.status, DownloadStatus::Completed, "{:?}", rec.error);
    assert_eq!(read(rec.target_path()), *data);
}

#[tokio::test(flavor = "multi_thread")]
async fn s12_rate_limited_429_respects_retry_after() {
    let server = TestServer::start().await;
    let spec = Spec { fail_first: 1, fail_status: 429, retry_after: Some(2), ..file_spec(300_000) };
    let url = server.add("limited.bin", spec);
    let h = harness().await;

    let started = Instant::now();
    let rec = h.engine.add(add(&url)).unwrap();
    let rec = wait_finished(&h.engine, &rec.id).await;
    assert_eq!(rec.status, DownloadStatus::Completed, "{:?}", rec.error);
    assert!(started.elapsed() >= Duration::from_millis(1900), "Retry-After: 2 was not honoured");
}

#[tokio::test(flavor = "multi_thread")]
async fn s13_timeout_then_recovery() {
    let server = TestServer::start().await;
    // read_timeout is 5 s in tests; stall the first request longer than that.
    let spec = Spec { stall_first: Some(Duration::from_secs(7)), ..file_spec(400_000) };
    let data = spec.data.clone();
    let url = server.add("stall.bin", spec);
    let h = harness().await;

    let started = Instant::now();
    let rec = h.engine.add(add(&url)).unwrap();
    let rec = wait_finished(&h.engine, &rec.id).await;
    assert_eq!(rec.status, DownloadStatus::Completed, "{:?}", rec.error);
    assert_eq!(read(rec.target_path()), *data);
    assert!(started.elapsed() >= Duration::from_secs(5), "the read timeout should have fired");
    // Probe, the stalled request, and the retry.
    assert!(server.counters("stall.bin").requests.load(Ordering::SeqCst) >= 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn s14_disk_full_fails_cleanly() {
    let server = TestServer::start().await;
    let url = server.add("huge.bin", file_spec(2 * 1024 * 1024));
    let dir = tempfile::tempdir().unwrap();
    let fault: velora_engine::download::WriteFault =
        Arc::new(|offset| if offset >= 512 * 1024 { Err(io::Error::from(io::ErrorKind::StorageFull)) } else { Ok(()) });
    let (engine, _) = open_engine(None, dir.path(), Some(fault)).await;

    let rec = engine.add(add(&url)).unwrap();
    let rec = wait_finished(&engine, &rec.id).await;
    assert_eq!(rec.status, DownloadStatus::Failed);
    assert_eq!(rec.error.as_ref().unwrap().code, "disk_full");
    assert!(!dir.path().join("huge.bin").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn s15_duplicate_filenames_and_urls() {
    let server = TestServer::start().await;
    let spec = Spec { chunk_delay: Some(Duration::from_millis(20)), ..file_spec(1024 * 1024) };
    let data = spec.data.clone();
    let url = server.add("dup/report.pdf", spec);
    let h = harness().await;
    std::fs::write(h.dir.path().join("report.pdf"), b"existing user file").unwrap();

    let check = h.engine.check_add(&url, None, None, None).unwrap();
    assert!(check.file_exists);
    assert_eq!(check.suggested_name, "report (1).pdf");

    let first = h.engine.add(add(&url)).unwrap();
    let check = h.engine.check_add(&url, None, None, None).unwrap();
    assert_eq!(check.duplicate_of.as_ref().map(|r| r.id.as_str()), Some(first.id.as_str()));

    let second = h.engine.add(add(&url)).unwrap();
    let a = wait_finished(&h.engine, &first.id).await;
    let b = wait_finished(&h.engine, &second.id).await;
    assert_eq!(a.status, DownloadStatus::Completed);
    assert_eq!(b.status, DownloadStatus::Completed);
    assert_ne!(a.file_name, b.file_name);
    assert_eq!(std::fs::read(h.dir.path().join("report.pdf")).unwrap(), b"existing user file", "existing file untouched");
    assert_eq!(read(a.target_path()), *data);
    assert_eq!(read(b.target_path()), *data);
}

#[tokio::test(flavor = "multi_thread")]
async fn s16_cancellation_removes_partial_data() {
    let server = TestServer::start().await;
    let spec = Spec { chunk_delay: Some(Duration::from_millis(20)), ..file_spec(4 * 1024 * 1024) };
    let url = server.add("cancel.bin", spec);
    let h = harness().await;

    let rec = h.engine.add(add(&url)).unwrap();
    wait_progress(&h.engine, &rec.id, 256 * 1024).await;
    h.engine.cancel(&rec.id).unwrap();
    let rec = wait_status(&h.engine, &rec.id, DownloadStatus::Cancelled, Duration::from_secs(10)).await;
    assert_eq!(rec.downloaded, 0);
    // Give the OS a moment to release handles, then check no partial files remain.
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(leftovers(h.dir.path()).is_empty(), "{:?}", leftovers(h.dir.path()));
    assert!(!h.dir.path().join("cancel.bin").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn s17_queue_persists_and_runs_in_order() {
    let server = TestServer::start().await;
    let urls: Vec<String> = (0..3).map(|i| server.add(&format!("q{i}.bin"), file_spec(200_000 + i))).collect();
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("velora.db");
    let dl = dir.path().join("dl");
    std::fs::create_dir_all(&dl).unwrap();

    let (engine, _) = open_engine(Some(&db), &dl, None).await;
    engine.pause_all();
    let ids: Vec<String> =
        urls.iter().map(|u| engine.add(AddRequest { url: u.clone(), start: StartMode::Queue, ..Default::default() }).unwrap().id).collect();
    // Run the last one first.
    let order = vec![ids[2].clone(), ids[0].clone(), ids[1].clone()];
    engine.reorder(&order).unwrap();
    engine.shutdown().await;
    drop(engine);

    let (engine, _) = open_engine(Some(&db), &dl, None).await;
    for id in &ids {
        assert_eq!(engine.get(id).unwrap().status, DownloadStatus::Queued);
    }
    let mut s = engine.settings();
    s.max_active = 1;
    engine.update_settings(s).unwrap();
    engine.resume_all();
    let mut finished = Vec::new();
    for id in &ids {
        finished.push(wait_status(&engine, id, DownloadStatus::Completed, Duration::from_secs(30)).await);
    }
    finished.sort_by_key(|r| r.completed_at);
    let completed_order: Vec<String> = finished.into_iter().map(|r| r.id).collect();
    assert_eq!(completed_order, order);
}

#[tokio::test(flavor = "multi_thread")]
async fn s18_checksum_verification() {
    let server = TestServer::start().await;
    let spec = file_spec(500_000);
    let expected = {
        use sha2::{Digest, Sha256};
        Sha256::digest(&*spec.data).iter().map(|b| format!("{b:02x}")).collect::<String>()
    };
    let url = server.add("sum.bin", spec);
    let h = harness().await;

    let good = h.engine.add(AddRequest { checksum: Some(expected.to_uppercase()), ..add(&url) }).unwrap();
    let good = wait_finished(&h.engine, &good.id).await;
    assert_eq!(good.status, DownloadStatus::Completed);
    assert_eq!(good.checksum_state, ChecksumState::Verified);

    let bad = h.engine.add(AddRequest { checksum: Some("0".repeat(64)), ..add(&url) }).unwrap();
    let bad = wait_finished(&h.engine, &bad.id).await;
    assert_eq!(bad.checksum_state, ChecksumState::Mismatch);

    assert!(h.engine.add(AddRequest { checksum: Some("xyz".into()), ..add(&url) }).is_err());
    let actual = h.engine.verify_checksum(&good.id, None).await.unwrap();
    assert_eq!(actual, expected);
}

#[tokio::test(flavor = "multi_thread")]
async fn s19_corrupt_partial_files_restart_safely() {
    let server = TestServer::start().await;
    let spec = Spec { chunk_delay: Some(Duration::from_millis(15)), ..file_spec(3 * 1024 * 1024) };
    let data = spec.data.clone();
    let url = server.add("corrupt.bin", spec);
    let h = harness().await;

    let rec = h.engine.add(add(&url)).unwrap();
    wait_progress(&h.engine, &rec.id, 512 * 1024).await;
    h.engine.pause(&rec.id).unwrap();
    let paused = wait_status(&h.engine, &rec.id, DownloadStatus::Paused, Duration::from_secs(10)).await;
    let ck = checkpoint::checkpoint_path(&paused.target_path());
    assert!(ck.exists());
    std::fs::write(&ck, b"{ this is not json").unwrap();
    // Also damage the partial data: a corrupt checkpoint means none of it may be trusted.
    let part = checkpoint::part_path(&paused.target_path());
    std::fs::write(&part, vec![0xFFu8; 1000]).unwrap();

    server.update("corrupt.bin", |s| s.chunk_delay = None);
    h.engine.resume(&rec.id).unwrap();
    let rec = wait_finished(&h.engine, &rec.id).await;
    assert_eq!(rec.status, DownloadStatus::Completed, "{:?}", rec.error);
    assert_eq!(read(rec.target_path()), *data);
}

#[tokio::test(flavor = "multi_thread")]
async fn s20_invalid_or_inaccessible_destination() {
    let server = TestServer::start().await;
    let url = server.add("x.bin", file_spec(10_000));
    let h = harness().await;

    let err = h.engine.add(AddRequest { dest_dir: Some("relative\\folder".into()), ..add(&url) }).unwrap_err();
    assert_eq!(err.code, "path");

    // A file sits where the destination folder should be.
    let blocker = h.dir.path().join("not-a-folder");
    std::fs::write(&blocker, b"x").unwrap();
    let rec = h.engine.add(AddRequest { dest_dir: Some(blocker.join("sub").to_string_lossy().into_owned()), ..add(&url) }).unwrap();
    let rec = wait_finished(&h.engine, &rec.id).await;
    assert_eq!(rec.status, DownloadStatus::Failed);

    assert!(h.engine.add(AddRequest { url: "file:///C:/Windows/win.ini".into(), ..Default::default() }).is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn s21_global_speed_limit_throttles_transfer() {
    let server = TestServer::start().await;
    let spec = file_spec(600 * 1024);
    let url = server.add("slow.bin", spec);
    let h = harness().await;
    let mut s = h.engine.settings();
    s.global_speed_limit = 200 * 1024;
    h.engine.update_settings(s).unwrap();

    let started = Instant::now();
    let rec = h.engine.add(add(&url)).unwrap();
    let rec = wait_finished(&h.engine, &rec.id).await;
    assert_eq!(rec.status, DownloadStatus::Completed, "{:?}", rec.error);
    // 600 KiB at 200 KiB/s takes about 3 s (minus the initial burst allowance).
    assert!(started.elapsed() >= Duration::from_millis(2300), "took {:?}", started.elapsed());
}

#[tokio::test(flavor = "multi_thread")]
async fn s22_pause_resume_mid_download_is_byte_exact() {
    let server = TestServer::start().await;
    let spec = Spec { chunk_delay: Some(Duration::from_millis(40)), ..file_spec(6 * 1024 * 1024 + 99) };
    let data = spec.data.clone();
    let url = server.add("pr.bin", spec);
    let h = harness().await;

    let rec = h.engine.add(AddRequest { connections: Some(6), ..add(&url) }).unwrap();
    for _ in 0..3 {
        wait_for(&h.engine, &rec.id, Duration::from_secs(20), |r| r.status == DownloadStatus::Downloading).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        h.engine.pause(&rec.id).unwrap();
        let paused = wait_for(&h.engine, &rec.id, Duration::from_secs(10), |r| {
            matches!(r.status, DownloadStatus::Paused | DownloadStatus::Completed)
        })
        .await;
        assert_eq!(paused.status, DownloadStatus::Paused, "download finished before it could be paused; slow the server down");
        h.engine.resume(&rec.id).unwrap();
    }
    let rec = wait_finished(&h.engine, &rec.id).await;
    assert_eq!(rec.status, DownloadStatus::Completed, "{:?}", rec.error);
    assert_eq!(read(rec.target_path()), *data);
}

#[tokio::test(flavor = "multi_thread")]
async fn s23_permanent_404_does_not_retry() {
    let server = TestServer::start().await;
    let h = harness().await;
    let rec = h.engine.add(add(&format!("{}/missing.zip", server.base))).unwrap();
    let rec = wait_finished(&h.engine, &rec.id).await;
    assert_eq!(rec.status, DownloadStatus::Failed);
    assert_eq!(rec.error.as_ref().unwrap().code, "not_found");
}

#[tokio::test(flavor = "multi_thread")]
async fn s25_download_again_never_overwrites_the_finished_file() {
    let server = TestServer::start().await;
    let spec = file_spec(300_000);
    let data = spec.data.clone();
    let url = server.add("again.bin", spec);
    let h = harness().await;

    let rec = h.engine.add(add(&url)).unwrap();
    let first = wait_status(&h.engine, &rec.id, DownloadStatus::Completed, Duration::from_secs(20)).await;
    std::fs::write(first.target_path(), b"user edited this file").unwrap();

    h.engine.restart(&rec.id).unwrap();
    let again =
        wait_for(&h.engine, &rec.id, Duration::from_secs(20), |r| r.status == DownloadStatus::Completed && r.file_name != first.file_name)
            .await;
    assert_eq!(std::fs::read(first.target_path()).unwrap(), b"user edited this file");
    assert_eq!(read(again.target_path()), *data);
}

#[tokio::test(flavor = "multi_thread")]
async fn s24_remove_running_download_cleans_up() {
    let server = TestServer::start().await;
    let spec = Spec { chunk_delay: Some(Duration::from_millis(20)), ..file_spec(2 * 1024 * 1024) };
    let url = server.add("rm.bin", spec);
    let h = harness().await;
    let rec = h.engine.add(add(&url)).unwrap();
    wait_progress(&h.engine, &rec.id, 128 * 1024).await;
    h.engine.remove(std::slice::from_ref(&rec.id)).unwrap();
    assert!(h.engine.get(&rec.id).is_none());
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(leftovers(h.dir.path()).is_empty(), "{:?}", leftovers(h.dir.path()));
}
