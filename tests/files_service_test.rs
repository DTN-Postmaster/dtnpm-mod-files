use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use bytes::Bytes;
use dtnpm_mod_files::{FileBundleRecord, FilesModule};
use dtnpm_web_core::TestServerBuilder;
use hardy_bpv7::builder::Builder;
use hardy_bpv7::creation_timestamp::CreationTimestamp;
use hardy_bpv7::eid::Eid;
use serde_json::Value;

#[tokio::test]
async fn test_files_service_workflow_and_degraded_cases() {
    let srv = TestServerBuilder::new()
        .node_id("dtn://stata/")
        .callsign("STATA")
        .with_module(Arc::new(FilesModule::new()))
        .build()
        .await
        .expect("TestServerBuilder should build with FilesModule");

    // 0. Capabilities should report files module
    let caps_res = srv.get("/api/capabilities").await;
    assert_eq!(caps_res.status(), StatusCode::OK);
    let caps_bytes = axum::body::to_bytes(caps_res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let caps: Value = serde_json::from_slice(&caps_bytes).unwrap();
    assert_eq!(caps["modules"][0]["id"], "files");

    // 1. Initial GET /api/files/v1 should be empty
    let res = srv.get("/api/files/v1").await;
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let list: Vec<FileBundleRecord> = serde_json::from_slice(&bytes).unwrap();
    assert!(list.is_empty());

    // 2. Deliver inbound bundle to dtn://stata/files with Markdown + Data-URI image
    let sample_markdown = "# Station Notice 📡\nHere is a packet: ![pixel](data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==)\n73 de F4JXQ!";
    let source_eid: Eid = "dtn://f4jxq/files".parse().unwrap();
    let dest_eid: Eid = "dtn://stata/files".parse().unwrap();
    let (_bundle, bin) = Builder::new(source_eid, dest_eid)
        .with_payload(sample_markdown.as_bytes().into())
        .with_lifetime(Duration::from_secs(3600))
        .build(CreationTimestamp::now())
        .expect("bundle should build");

    srv.ctx
        .injector
        .inject(Bytes::from(bin.into_vec()))
        .await
        .expect("injection should succeed");

    tokio::time::sleep(Duration::from_millis(50)).await;

    // 3. List files via GET /api/files/v1
    let res = srv.get("/api/files/v1").await;
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let list: Vec<FileBundleRecord> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(list.len(), 1);
    let bundle_id = list[0].id.clone();
    assert_eq!(list[0].source_eid, "dtn://f4jxq/files");
    assert_eq!(list[0].destination_eid, "dtn://stata/files");
    assert_eq!(list[0].mime_type, "text/markdown");

    // 4. Unread count should be 1
    let res = srv.get("/api/files/v1/unread").await;
    let unread_json: Value = serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(unread_json["unread"], 1);

    // 5. Inspect details and auto-mark read
    let res = srv
        .get(&format!("/api/files/v1/{}", url_encode(&bundle_id)))
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    let detail_json: Value = serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(detail_json["bundle"]["id"], bundle_id);
    assert_eq!(detail_json["bundle"]["is_read"], true);
    assert_eq!(detail_json["file_missing"], false);

    // 6. Download payload raw / inline + Range support
    let res = srv
        .get(&format!("/api/files/v1/{}/payload", url_encode(&bundle_id)))
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers().get("content-type").unwrap(), "text/markdown");
    assert_eq!(res.headers().get("accept-ranges").unwrap(), "bytes");
    let payload_bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&payload_bytes), sample_markdown);

    let res_dl = srv
        .get(&format!(
            "/api/files/v1/{}/payload?download=true",
            url_encode(&bundle_id)
        ))
        .await;
    assert_eq!(res_dl.status(), StatusCode::OK);
    assert!(
        res_dl
            .headers()
            .get("content-disposition")
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("attachment;")
    );

    let res_range = srv
        .oneshot(
            Request::builder()
                .uri(format!("/api/files/v1/{}/payload", url_encode(&bundle_id)))
                .header("Range", "bytes=0-9")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(res_range.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        res_range
            .headers()
            .get("content-range")
            .unwrap()
            .to_str()
            .unwrap(),
        &format!("bytes 0-9/{}", sample_markdown.len())
    );
    let range_bytes = axum::body::to_bytes(res_range.into_body(), 1024 * 1024)
        .await
        .unwrap();
    assert_eq!(range_bytes.len(), 10);
    assert_eq!(&range_bytes[..], &sample_markdown.as_bytes()[0..10]);

    // 7. Toggle sticky status
    let res = srv
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/files/v1/{}/sticky", url_encode(&bundle_id)))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    let sticky_json: Value = serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(sticky_json["is_sticky"], true);

    // 8. Degraded cases:
    let res = srv.get("/api/files/v1/non-existent-bundle-id").await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    let res = srv
        .get("/api/files/v1/non-existent-bundle-id/payload")
        .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    let res = srv
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/files/v1/non-existent-bundle-id/sticky")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // 8b. Physical file deleted on disk -> API reports file_missing: true and payload download 404
    let file_rec = srv
        .ctx
        .internal_storage
        .get_file_bundle(&bundle_id)
        .unwrap()
        .unwrap();
    let _ = std::fs::remove_file(&file_rec.payload_path);

    let res = srv
        .get(&format!("/api/files/v1/{}", url_encode(&bundle_id)))
        .await;
    let detail_json: Value = serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(detail_json["file_missing"], true);

    let res = srv
        .get(&format!("/api/files/v1/{}/payload", url_encode(&bundle_id)))
        .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // 8c. Deletion succeeds even when physical payload was already deleted
    let res = srv
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/files/v1/{}", url_encode(&bundle_id)))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    assert!(
        srv.ctx
            .internal_storage
            .get_file_bundle(&bundle_id)
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn test_files_payload_range_requests_and_mime_detection() {
    let srv = TestServerBuilder::new()
        .node_id("dtn://stata/")
        .callsign("STATA")
        .with_module(Arc::new(FilesModule::new()))
        .build()
        .await
        .unwrap();

    let storage_dir = srv.ctx.internal_storage.files_dir().to_path_buf();

    let mut ogg_opus_payload = vec![0u8; 50];
    ogg_opus_payload[0..4].copy_from_slice(b"OggS");
    ogg_opus_payload[26] = 1;
    ogg_opus_payload[28..36].copy_from_slice(b"OpusHead");
    for (idx, slot) in ogg_opus_payload[40..50].iter_mut().enumerate() {
        *slot = (40 + idx) as u8;
    }

    let payload_path = storage_dir.join("test_audio.ogg");
    tokio::fs::write(&payload_path, &ogg_opus_payload)
        .await
        .unwrap();

    let record = FileBundleRecord {
        id: "test_audio_bundle".to_string(),
        source_eid: "dtn://sender/files".to_string(),
        destination_eid: "dtn://stata/files".to_string(),
        creation_timestamp: 12345,
        lifetime_secs: 3600,
        size: 50,
        mime_type: "audio/ogg".to_string(),
        payload_path: payload_path.to_str().unwrap().to_string(),
        created_at: "2026-11-05T10:00:00Z".to_string(),
        expiry_timestamp: 12345 + 3600 * 1000,
        is_sticky: false,
        is_read: false,
        auth_status: "unsigned".to_string(),
    };
    srv.ctx
        .internal_storage
        .insert_file_bundle(&record)
        .unwrap();

    let mut mp4_payload = vec![0u8; 64];
    mp4_payload[0..4].copy_from_slice(b"\x00\x00\x00\x20");
    mp4_payload[4..8].copy_from_slice(b"ftyp");
    mp4_payload[8..12].copy_from_slice(b"isom");
    let video_path = storage_dir.join("test_video.mp4");
    tokio::fs::write(&video_path, &mp4_payload).await.unwrap();

    let video_record = FileBundleRecord {
        id: "test_video_bundle".to_string(),
        source_eid: "dtn://sender/files".to_string(),
        destination_eid: "dtn://stata/files".to_string(),
        creation_timestamp: 12346,
        lifetime_secs: 3600,
        size: 64,
        mime_type: "video/mp4".to_string(),
        payload_path: video_path.to_str().unwrap().to_string(),
        created_at: "2026-11-05T10:00:01Z".to_string(),
        expiry_timestamp: 12346 + 3600 * 1000,
        is_sticky: false,
        is_read: false,
        auth_status: "unsigned".to_string(),
    };
    srv.ctx
        .internal_storage
        .insert_file_bundle(&video_record)
        .unwrap();

    let svg_payload =
        b"<svg xmlns=\"http://www.w3.org/2000/svg\"><circle cx=\"50\" cy=\"50\" r=\"40\"/></svg>";
    let svg_path = storage_dir.join("test_image.svg");
    tokio::fs::write(&svg_path, svg_payload).await.unwrap();

    let svg_record = FileBundleRecord {
        id: "test_svg_bundle".to_string(),
        source_eid: "dtn://sender/files".to_string(),
        destination_eid: "dtn://stata/files".to_string(),
        creation_timestamp: 12347,
        lifetime_secs: 3600,
        size: svg_payload.len() as u64,
        mime_type: "image/svg+xml".to_string(),
        payload_path: svg_path.to_str().unwrap().to_string(),
        created_at: "2026-11-05T10:00:02Z".to_string(),
        expiry_timestamp: 12347 + 3600 * 1000,
        is_sticky: false,
        is_read: false,
        auth_status: "unsigned".to_string(),
    };
    srv.ctx
        .internal_storage
        .insert_file_bundle(&svg_record)
        .unwrap();

    // 1. Normal full download: 200 OK, dynamic MIME type detected as audio/ogg; codecs=opus
    let res = srv.get("/api/files/v1/test_audio_bundle/payload").await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers().get("content-type").unwrap().to_str().unwrap(),
        "audio/ogg; codecs=opus"
    );
    assert_eq!(
        res.headers()
            .get("accept-ranges")
            .unwrap()
            .to_str()
            .unwrap(),
        "bytes"
    );
    assert_eq!(
        res.headers()
            .get("content-disposition")
            .unwrap()
            .to_str()
            .unwrap(),
        "inline; filename=\"test_audio.ogg\""
    );

    // 2. Standard range request: bytes=0-9
    let res = srv
        .oneshot(
            Request::builder()
                .uri("/api/files/v1/test_audio_bundle/payload")
                .header("range", "bytes=0-9")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(res.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        res.headers()
            .get("content-range")
            .unwrap()
            .to_str()
            .unwrap(),
        "bytes 0-9/50"
    );
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
    assert_eq!(body_bytes.len(), 10);
    assert_eq!(&body_bytes[0..4], b"OggS");

    // 3. Suffix range request: bytes=-10
    let res = srv
        .oneshot(
            Request::builder()
                .uri("/api/files/v1/test_audio_bundle/payload")
                .header("range", "bytes=-10")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(res.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        res.headers()
            .get("content-range")
            .unwrap()
            .to_str()
            .unwrap(),
        "bytes 40-49/50"
    );
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
    assert_eq!(body_bytes.len(), 10);
    assert_eq!(body_bytes[0], 40);
    assert_eq!(body_bytes[9], 49);

    // 4. Suffix range request larger than file: bytes=-100
    let res = srv
        .oneshot(
            Request::builder()
                .uri("/api/files/v1/test_audio_bundle/payload")
                .header("range", "bytes=-100")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(res.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        res.headers()
            .get("content-range")
            .unwrap()
            .to_str()
            .unwrap(),
        "bytes 0-49/50"
    );

    // 5. Out of bounds range request: bytes=100- -> 416 Range Not Satisfiable
    let res = srv
        .oneshot(
            Request::builder()
                .uri("/api/files/v1/test_audio_bundle/payload")
                .header("range", "bytes=100-")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(res.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(
        res.headers()
            .get("content-range")
            .unwrap()
            .to_str()
            .unwrap(),
        "bytes */50"
    );

    // 6. Video full download
    let res = srv.get("/api/files/v1/test_video_bundle/payload").await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers().get("content-type").unwrap().to_str().unwrap(),
        "video/mp4"
    );
    assert_eq!(
        res.headers()
            .get("content-disposition")
            .unwrap()
            .to_str()
            .unwrap(),
        "inline; filename=\"test_video.mp4\""
    );

    // 7. Video partial range request: bytes=4-7
    let res = srv
        .oneshot(
            Request::builder()
                .uri("/api/files/v1/test_video_bundle/payload")
                .header("range", "bytes=4-7")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(res.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        res.headers()
            .get("content-range")
            .unwrap()
            .to_str()
            .unwrap(),
        "bytes 4-7/64"
    );
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
    assert_eq!(&body_bytes[..], b"ftyp");

    // 8. SVG full download
    let res = srv.get("/api/files/v1/test_svg_bundle/payload").await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers().get("content-type").unwrap().to_str().unwrap(),
        "image/svg+xml"
    );
    assert_eq!(
        res.headers()
            .get("content-disposition")
            .unwrap()
            .to_str()
            .unwrap(),
        "inline; filename=\"test_image.svg\""
    );
}

fn url_encode(input: &str) -> String {
    let mut encoded = String::new();
    for byte in input.bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                use std::fmt::Write;
                let _ = write!(&mut encoded, "%{:02X}", byte);
            }
        }
    }
    encoded
}
