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

#[tokio::test]
async fn protected_or_sign_in_sites_are_refused_clearly() {
    let client = reqwest::Client::new();
    for (url, site) in [
        ("https://www.instagram.com/reels/Det_GaduY-V/", "Instagram"),
        ("https://www.youtube.com/watch?v=abc", "YouTube"),
        ("https://youtu.be/abc", "YouTube"),
        ("https://m.facebook.com/watch/?v=1", "Facebook"),
        ("https://www.tiktok.com/@a/video/1", "TikTok"),
    ] {
        let err = velora_engine::media::resolve(&client, &Url::parse(url).unwrap()).await.unwrap_err();
        assert_eq!(err.code, "unsupported_site", "{url}");
        assert!(err.message.contains(site), "{url}: {}", err.message);
    }
    // Lookalike hosts and CDNs are not caught by the rule.
    for url in ["https://notinstagram.com/x", "https://scontent.cdninstagram.com/v/file.mp4", "https://example.com/youtube.com/a.zip"] {
        assert!(velora_engine::media::unsupported_site(&Url::parse(url).unwrap()).is_none(), "{url}");
    }
}

#[tokio::test]
async fn ordinary_urls_are_not_resolved() {
    let client = reqwest::Client::new();
    let r = velora_engine::media::resolve(&client, &Url::parse("https://example.com/file.zip").unwrap()).await.unwrap();
    assert!(r.is_none());
}
