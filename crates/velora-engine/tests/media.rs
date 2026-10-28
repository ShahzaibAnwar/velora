//! X post resolver against a local fake of the public embed endpoint.

use axum::extract::Query;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{json, Value};
use std::collections::HashMap;
use url::Url;
use velora_engine::media::x;

async fn start() -> String {
    async fn handler(Query(q): Query<HashMap<String, String>>) -> Result<Json<Value>, StatusCode> {
        // The resolver must send the widget's token for the requested ID.
        let id = q.get("id").cloned().unwrap_or_default();
        if q.get("token") != Some(&x::token(&id)) {
            return Err(StatusCode::BAD_REQUEST);
        }
        match id.as_str() {
            "100" => Ok(Json(json!({
                "__typename": "Tweet",
                "id_str": "100",
                "text": "A short   clip\nwith a caption",
                "user": { "screen_name": "someone" },
                "mediaDetails": [{
                    "type": "video",
                    "video_info": { "variants": [
                        { "content_type": "application/x-mpegURL", "url": "https://video.twimg.com/amplify_video/1/pl/a.m3u8" },
                        { "content_type": "video/mp4", "bitrate": 256000, "url": "https://video.twimg.com/amplify_video/1/vid/avc1/480x270/s.mp4?tag=14" },
                        { "content_type": "video/mp4", "bitrate": 2176000, "url": "https://video.twimg.com/amplify_video/1/vid/avc1/1280x720/l.mp4?tag=14" },
                        { "content_type": "video/mp4", "bitrate": 9999999, "url": "https://evil.example/vid/avc1/1920x1080/x.mp4" }
                    ]}
                }]
            }))),
            "200" => Ok(Json(json!({ "__typename": "Tweet", "id_str": "200", "text": "no video", "mediaDetails": [{ "type": "photo" }] }))),
            "300" => Ok(Json(json!({ "__typename": "TweetTombstone" }))),
            _ => Err(StatusCode::NOT_FOUND),
        }
    }
    let app = Router::new().route("/tweet-result", get(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}/tweet-result")
}

fn page(id: &str) -> Url {
    Url::parse(&format!("https://x.com/someone/status/{id}?s=20")).unwrap()
}

#[tokio::test]
async fn resolves_best_mp4_first_and_ignores_foreign_hosts() {
    let endpoint = start().await;
    let client = reqwest::Client::new();
    let media = x::resolve(&client, &endpoint, &page("100")).await.unwrap();
    assert_eq!(media.source, "X");
    assert_eq!(media.author.as_deref(), Some("someone"));
    assert_eq!(media.title, "A short clip with a caption");
    let labels: Vec<_> = media.variants.iter().map(|v| v.label.as_str()).collect();
    assert_eq!(labels, ["1280×720", "480×270"], "HLS playlists and non-X hosts are excluded, best first");
    assert_eq!(media.variants[0].file_name, "someone_100_720p.mp4");
    assert!(media.variants[0].url.starts_with("https://video.twimg.com/"));
}

#[tokio::test]
async fn explains_posts_without_video_or_unavailable() {
    let endpoint = start().await;
    let client = reqwest::Client::new();
    assert_eq!(x::resolve(&client, &endpoint, &page("200")).await.unwrap_err().code, "no_video");
    assert_eq!(x::resolve(&client, &endpoint, &page("300")).await.unwrap_err().code, "not_found");
    assert_eq!(x::resolve(&client, &endpoint, &page("404")).await.unwrap_err().code, "not_found");
}

/// Hits the real X endpoint and downloads the video. Opt-in: `cargo test -p velora-engine --test media -- --ignored`.
/// Set VELORA_LIVE_X_POST to test another public post.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs internet and a public X post"]
async fn live_x_post_resolves_and_downloads() {
    use std::sync::Arc;
    use velora_engine::{AddRequest, DownloadStatus, Engine, EngineConfig, StartMode};

    let post = std::env::var("VELORA_LIVE_X_POST").unwrap_or_else(|_| "https://x.com/SMunirMaan/status/2112847019228414346".into());
    let dir = tempfile::tempdir().unwrap();
    let engine = Engine::open(EngineConfig::new(None, dir.path().to_path_buf(), Arc::new(|_| {}))).await.unwrap();
    let media = engine.resolve_media(&post).await.unwrap().expect("an X post is recognised");
    println!("resolved {} variant(s): {:?}", media.variants.len(), media.variants.iter().map(|v| &v.label).collect::<Vec<_>>());
    let best = &media.variants[0];
    let rec = engine
        .add(AddRequest {
            url: best.url.clone(),
            file_name: Some(best.file_name.clone()),
            referrer: Some(media.page_url.clone()),
            start: StartMode::Now,
            ..Default::default()
        })
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    let rec = loop {
        let r = engine.get(&rec.id).unwrap();
        if r.status.is_finished() || std::time::Instant::now() > deadline {
            break r;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    };
    assert_eq!(rec.status, DownloadStatus::Completed, "{:?}", rec.error);
    let bytes = std::fs::read(rec.target_path()).unwrap();
    println!("downloaded {} ({} bytes, resumable: {})", rec.file_name, bytes.len(), rec.resumable);
    assert_eq!(&bytes[4..8], b"ftyp", "the file is an MP4");
}

async fn wait_live(
    engine: &velora_engine::Engine,
    id: &str,
    secs: u64,
    pred: impl Fn(&velora_engine::DownloadRecord) -> bool,
) -> velora_engine::DownloadRecord {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(secs);
    loop {
        let r = engine.get(id).unwrap();
        if pred(&r) || r.status == velora_engine::DownloadStatus::Failed || std::time::Instant::now() > deadline {
            return r;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
}

/// Installs yt-dlp into a temp data folder, resolves a video page and downloads the best choice
/// (merging needs ffmpeg on PATH outside Windows). Opt-in; set VELORA_LIVE_VIDEO for another page.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs internet"]
async fn live_ytdlp_resolves_and_downloads() {
    use std::sync::Arc;
    use velora_engine::{AddRequest, DownloadKind, DownloadStatus, Engine, EngineConfig, StartMode};

    // A public HLS test stream: exercises yt-dlp, the variant list and the ffmpeg remux.
    let page = std::env::var("VELORA_LIVE_VIDEO").unwrap_or_else(|_| "https://test-streams.mux.dev/x36xhzz/x36xhzz.m3u8".into());
    let dir = tempfile::tempdir().unwrap();
    let config = EngineConfig::new(None, dir.path().join("dl"), Arc::new(|_| {})).with_data_dir(dir.path().join("data"));
    let engine = Engine::open(config).await.unwrap();
    let started = std::time::Instant::now();
    let media = engine.resolve_media(&page).await.unwrap().expect("yt-dlp recognises the page");
    println!(
        "{} from {} in {:.1}s: {:?}",
        media.title,
        media.source,
        started.elapsed().as_secs_f64(),
        media.variants.iter().map(|v| (&v.label, v.size)).collect::<Vec<_>>()
    );
    if std::env::var_os("VELORA_LIVE_RESOLVE_ONLY").is_some() {
        return;
    }
    let pick = media.variants.iter().rev().find(|v| v.height.is_some_and(|h| h <= 360)).unwrap_or(&media.variants[0]);
    let rec = engine
        .add(AddRequest {
            url: pick.url.clone(),
            file_name: Some(pick.file_name.clone()),
            media_format: pick.format.clone(),
            start: StartMode::Paused,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(rec.kind, DownloadKind::Media);
    // A downloaded file named like yt-dlp's config must never be read as options.
    std::fs::create_dir_all(&rec.dest_dir).unwrap();
    let planted = std::path::Path::new(&rec.dest_dir).join("yt-dlp.conf");
    std::fs::write(&planted, "-o planted.%(ext)s\n").unwrap();
    engine.resume(&rec.id).unwrap();
    let rec = wait_live(&engine, &rec.id, 300, |r| r.status == DownloadStatus::Completed).await;
    assert_eq!(rec.status, DownloadStatus::Completed, "{:?}", rec.error);
    let len = std::fs::metadata(rec.target_path()).unwrap().len();
    println!("downloaded {} ({len} bytes)", rec.file_name);
    assert!(len > 10_000);
    assert_eq!(rec.file_name, pick.file_name, "the planted config was ignored");
    std::fs::remove_file(&planted).unwrap();
    let leftovers: Vec<_> = std::fs::read_dir(&rec.dest_dir).unwrap().flatten().map(|e| e.file_name()).collect();
    assert_eq!(leftovers.len(), 1, "only the finished file remains: {leftovers:?}");
}

/// Measures plain reqwest throughput against a large GitHub release asset for 15 s, with the
/// engine's own client settings and with reqwest defaults. Opt-in diagnostic.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs internet; diagnostic"]
async fn live_http_throughput() {
    use futures_util::StreamExt;
    let url = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-lgpl.zip";
    for (label, client) in [
        ("engine client", velora_engine::net::build_client(&velora_engine::Settings::default()).unwrap()),
        ("reqwest default", reqwest::Client::new()),
    ] {
        let start = std::time::Instant::now();
        let resp = client.get(url).send().await.unwrap();
        let version = resp.version();
        let mut stream = resp.bytes_stream();
        let mut total = 0u64;
        let mut chunks = 0u64;
        while start.elapsed() < std::time::Duration::from_secs(15) {
            match tokio::time::timeout(std::time::Duration::from_secs(5), stream.next()).await {
                Ok(Some(Ok(c))) => {
                    total += c.len() as u64;
                    chunks += 1;
                }
                _ => break,
            }
        }
        let secs = start.elapsed().as_secs_f64();
        println!("{label}: {version:?} {:.0} KB/s ({total} bytes in {chunks} chunks, {secs:.1}s)", total as f64 / secs / 1024.0);
    }
    // The same stream written to disk, as the helper installer does.
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let mut file = std::fs::File::create(dir.path().join("probe.zip.download")).unwrap();
    let client = velora_engine::net::build_client(&velora_engine::Settings::default()).unwrap();
    let start = std::time::Instant::now();
    let mut stream = client.get(url).send().await.unwrap().bytes_stream();
    let mut total = 0u64;
    while start.elapsed() < std::time::Duration::from_secs(15) {
        match tokio::time::timeout(std::time::Duration::from_secs(5), stream.next()).await {
            Ok(Some(Ok(c))) => {
                file.write_all(&c).unwrap();
                total += c.len() as u64;
            }
            _ => break,
        }
    }
    println!("engine client + file writes: {:.0} KB/s", total as f64 / start.elapsed().as_secs_f64() / 1024.0);
}

/// Installs yt-dlp and ffmpeg into an empty folder from their GitHub releases (checksums verified,
/// ffmpeg unpacked from its zip) and runs both. Opt-in; run with neither program on PATH.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs internet; downloads about 100 MB"]
async fn live_tools_install_and_run() {
    use velora_engine::tools::Tools;

    let dir = tempfile::tempdir().unwrap();
    let tools = Tools::new(Some(dir.path().to_path_buf()));
    let client = reqwest::Client::new();
    let ytdlp = tools.ensure_ytdlp(&client).await.unwrap();
    assert!(ytdlp.starts_with(dir.path()), "installed, not taken from PATH: {}", ytdlp.display());
    let out = std::process::Command::new(&ytdlp).arg("--version").output().unwrap();
    println!("yt-dlp {}", String::from_utf8_lossy(&out.stdout).trim());
    assert!(out.status.success());
    if cfg!(windows) {
        let ffmpeg_dir = tools.ensure_ffmpeg(&client).await.unwrap();
        assert!(ffmpeg_dir.starts_with(dir.path()), "installed, not taken from PATH: {}", ffmpeg_dir.display());
        let out = std::process::Command::new(ffmpeg_dir.join("ffmpeg.exe")).arg("-version").output().unwrap();
        println!("{}", String::from_utf8_lossy(&out.stdout).lines().next().unwrap_or_default());
        assert!(out.status.success());
        assert!(ffmpeg_dir.join("ffprobe.exe").is_file());
    }
}

/// Resolves a real torrent, downloads part of it, pauses, resumes from the pieces on disk, then
/// cancels and checks the data is removed. Opt-in; VELORA_LIVE_TORRENT picks another torrent.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs internet and peers"]
async fn live_torrent_pause_resume_cancel() {
    use std::sync::Arc;
    use velora_engine::{AddRequest, DownloadKind, DownloadStatus, Engine, EngineConfig, StartMode};

    let link = std::env::var("VELORA_LIVE_TORRENT")
        .unwrap_or_else(|_| "https://cdimage.debian.org/debian-cd/current/amd64/bt-cd/debian-13.7.0-amd64-netinst.iso.torrent".into());
    let dir = tempfile::tempdir().unwrap();
    let config = EngineConfig::new(None, dir.path().join("dl"), Arc::new(|_| {})).with_data_dir(dir.path().join("data"));
    let engine = Engine::open(config).await.unwrap();
    let info = engine.probe(&link, None).await.unwrap();
    println!("probe: {} · {:?} bytes · {:?}", info.file_name, info.size, info.mime);
    let rec = engine
        .add(AddRequest { url: link.clone(), file_name: Some(info.file_name.clone()), start: StartMode::Now, ..Default::default() })
        .unwrap();
    assert_eq!(rec.kind, DownloadKind::Torrent);

    let rec = wait_live(&engine, &rec.id, 180, |r| r.downloaded > 8 * 1024 * 1024).await;
    assert!(rec.downloaded > 8 * 1024 * 1024, "no progress: {:?} {:?}", rec.status, rec.error);
    println!("downloading {} into {}: {} bytes so far", rec.file_name, rec.dest_dir, rec.downloaded);
    let target = rec.target_path();

    engine.pause(&rec.id).unwrap();
    let paused = wait_live(&engine, &rec.id, 20, |r| r.status == DownloadStatus::Paused).await;
    assert_eq!(paused.status, DownloadStatus::Paused);
    assert!(target.exists());

    engine.resume(&rec.id).unwrap();
    let resumed = wait_live(&engine, &rec.id, 120, |r| r.status == DownloadStatus::Downloading && r.downloaded >= paused.downloaded).await;
    assert_eq!(resumed.status, DownloadStatus::Downloading, "{:?}", resumed.error);
    println!("resumed at {} bytes (paused at {})", resumed.downloaded, paused.downloaded);

    engine.cancel(&rec.id).unwrap();
    let cancelled = wait_live(&engine, &rec.id, 20, |r| r.status == DownloadStatus::Cancelled).await;
    assert_eq!(cancelled.status, DownloadStatus::Cancelled);
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    assert!(!target.exists(), "cancel removes the partial data");
}

#[tokio::test]
async fn drm_streaming_services_are_refused_clearly() {
    let client = reqwest::Client::new();
    for url in ["https://www.netflix.com/watch/1", "https://www.primevideo.com/detail/x", "https://www.disneyplus.com/video/1"] {
        let err = velora_engine::media::resolve(&client, &Url::parse(url).unwrap()).await.unwrap_err();
        assert_eq!(err.code, "unsupported_site", "{url}");
        assert!(err.message.contains("DRM"), "{url}: {}", err.message);
    }
    // Video sites handled by yt-dlp are not refused, and lookalike hosts are not caught by the rule.
    for url in [
        "https://www.youtube.com/watch?v=abc",
        "https://www.instagram.com/reels/x/",
        "https://notnetflix.com/x",
        "https://example.com/netflix.com/a.zip",
    ] {
        assert!(velora_engine::media::unsupported_site(&Url::parse(url).unwrap()).is_none(), "{url}");
    }
}

#[test]
fn video_sites_and_streams_are_routed_to_ytdlp() {
    use velora_engine::media::ytdlp::{is_known_site, is_stream_manifest, is_stream_mime};
    assert!(is_known_site(&Url::parse("https://www.youtube.com/watch?v=abc").unwrap()));
    assert!(is_known_site(&Url::parse("https://youtu.be/abc").unwrap()));
    assert!(is_known_site(&Url::parse("https://m.facebook.com/watch/?v=1").unwrap()));
    assert!(!is_known_site(&Url::parse("https://notyoutube.com/x").unwrap()));
    assert!(is_stream_manifest(&Url::parse("https://cdn.example.com/live/index.M3U8?t=1").unwrap()));
    assert!(is_stream_manifest(&Url::parse("https://cdn.example.com/v/manifest.mpd").unwrap()));
    assert!(!is_stream_manifest(&Url::parse("https://cdn.example.com/v/file.mp4").unwrap()));
    assert!(is_stream_mime("application/vnd.apple.mpegurl; charset=utf-8"));
    assert!(!is_stream_mime("text/html"));
}

#[tokio::test]
async fn magnet_links_are_added_as_torrents() {
    use std::sync::Arc;
    use velora_engine::{AddRequest, DownloadKind, Engine, EngineConfig, StartMode};

    let dir = tempfile::tempdir().unwrap();
    let engine = Engine::open(EngineConfig::new(None, dir.path().to_path_buf(), Arc::new(|_| {}))).await.unwrap();
    let magnet = "magnet:?xt=urn:btih:dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c&dn=Big+Buck+Bunny";
    assert!(engine.resolve_media(magnet).await.unwrap().is_none());
    let check = engine.check_add(magnet, None, None, None).unwrap();
    assert_eq!(check.file_name, "Big Buck Bunny");
    let rec = engine.add(AddRequest { url: magnet.into(), start: StartMode::Paused, ..Default::default() }).unwrap();
    assert_eq!(rec.kind, DownloadKind::Torrent);
    assert_eq!(rec.url, magnet);
    assert!(engine.check_add(magnet, None, None, None).unwrap().duplicate_of.is_some());
    assert!(engine.add(AddRequest { url: "magnet:?xt=urn:btih:zz".into(), ..Default::default() }).is_err());

    let media = engine
        .add(AddRequest {
            url: "https://www.youtube.com/watch?v=abc".into(),
            file_name: Some("Clip (720p).mp4".into()),
            media_format: Some("bv*[height=720]+ba/b[height=720]".into()),
            start: StartMode::Paused,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(media.kind, DownloadKind::Media);
    assert_eq!(media.file_name, "Clip (720p).mp4");
}

#[tokio::test]
async fn ordinary_urls_are_not_resolved() {
    let client = reqwest::Client::new();
    let r = velora_engine::media::resolve(&client, &Url::parse("https://example.com/file.zip").unwrap()).await.unwrap();
    assert!(r.is_none());
}
