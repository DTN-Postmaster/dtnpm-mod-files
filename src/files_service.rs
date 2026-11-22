//! Built-in permanent Files service handling incoming `dtn://<station>/files` bundles.
//!
//! Stores raw received files on disk and saves bundle metadata in `InternalStorage`.

use std::sync::Arc;

use bytes::Bytes;
use hardy_async::async_trait;
use hardy_bpa::services::{Result as ServiceResult, Service, ServiceSink, StatusNotify};
use hardy_bpa::stream::{Receiver, Segment, buffer_stream};
use hardy_bpv7::bundle::Id;
use hardy_bpv7::eid::Eid;
use hardy_bpv7::parse::parse;
use hardy_bpv7::status_report::ReasonCode;
use time::OffsetDateTime;
use tokio::sync::OnceCell;
use tracing::{debug, error, info, warn};

use dtnpm_core::internal_storage::{FileBundleRecord, InternalStorage, status_notify_to_parts};
use dtnpm_core::security::{AuthStatus, DirectionalKeyStore, VerifyPolicy, verify_parsed_bundle};
use dtnpm_core::service_tracker::ServiceTracker;
use sha2::{Digest, Sha256};

/// Permanent DTN BPv7 service for file transfers and bundle inspection (`BUNDLE_LIST`).
#[derive(Clone)]
pub struct FilesService {
    storage: InternalStorage,
    service_tracker: ServiceTracker,
    keystore: Arc<DirectionalKeyStore>,
    sink: Arc<OnceCell<Box<dyn ServiceSink>>>,
}

impl FilesService {
    /// Creates a new `FilesService` bound to station internal storage, service tracker and keystore.
    pub fn new(
        storage: InternalStorage,
        service_tracker: ServiceTracker,
        keystore: Arc<DirectionalKeyStore>,
    ) -> Self {
        Self {
            storage,
            service_tracker,
            keystore,
            sink: Arc::new(OnceCell::new()),
        }
    }

    /// Handles a delivered bundle payload stream for the `files` service.
    async fn handle_delivered_bundle(&self, data: Bytes) -> ServiceResult<()> {
        let Ok(parsed) = parse(data) else {
            debug!("FilesService: failed to parse delivered bundle");
            return Ok(());
        };

        let auth_status = verify_parsed_bundle(&parsed, &self.keystore);
        let policy = self.keystore.get_policy();
        if policy == VerifyPolicy::Strict && auth_status != AuthStatus::Valid {
            warn!(
                auth_status = ?auth_status,
                "FilesService: dropping delivered bundle due to strict policy without valid BPSec signature"
            );
            return Ok(());
        }

        let bundle = &parsed.bundle;
        let source_eid = &bundle.primary.id.source;
        let dest_eid = &bundle.primary.destination;

        // Skip null source or administrative records
        if source_eid.is_null() || bundle.primary.flags.is_admin_record {
            debug!("FilesService: ignoring null source or admin bundle");
            return Ok(());
        }

        // Extract payload block (#1)
        let Some(payload_slice) = bundle
            .blocks
            .get(&1)
            .and_then(|block| block.payload(&parsed.data))
        else {
            debug!("FilesService: delivered bundle has no payload block");
            return Ok(());
        };

        let bundle_id = bundle.primary.id.to_key();
        let dt = bundle
            .primary
            .id
            .timestamp
            .as_datetime()
            .unwrap_or_else(OffsetDateTime::now_utc);

        let created_at = dt
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| dt.to_string());

        let creation_timestamp = (dt.unix_timestamp_nanos() / 1_000_000) as i64;
        let lifetime_secs = bundle.primary.lifetime.as_secs().min(365 * 86_400);
        let expiry_timestamp =
            creation_timestamp.saturating_add((lifetime_secs as i64).saturating_mul(1000));

        let (inferred_mime, _) = dtnpm_core::bundle_inspector::infer_mime_type(payload_slice);
        let mime_type = inferred_mime.to_string();

        // Determine file extension from mime type
        let ext = match mime_type.as_str() {
            "text/markdown" => "md",
            "text/plain" => "txt",
            "application/json" => "json",
            "application/senml+json" => "senml.json",
            "application/geo+json" => "geojson",
            "application/gpx+xml" => "gpx",
            "application/vnd.google-earth.kml+xml" => "kml",
            "application/pdf" => "pdf",
            "application/vnd.sqlite3" => "sqlite",
            "application/zip" => "zip",
            "application/gzip" => "gz",
            "application/x-tar" => "tar",
            "application/x-xz" => "xz",
            "application/zstd" => "zst",
            "application/x-7z-compressed" => "7z",
            "application/x-bzip2" => "bz2",
            "image/png" => "png",
            "image/jpeg" => "jpg",
            "image/gif" => "gif",
            "image/webp" => "webp",
            "image/bmp" => "bmp",
            "image/avif" => "avif",
            "image/svg+xml" => "svg",
            "video/mp4" => "mp4",
            "video/webm" => "webm",
            "video/x-matroska" => "mkv",
            "video/ogg" => "ogv",
            "audio/mp4" => "m4a",
            "audio/wav" => "wav",
            "audio/flac" => "flac",
            "audio/mpeg" => "mp3",
            s if s.starts_with("audio/ogg") => "ogg",
            s if s.starts_with("audio/webm") => "webm",
            _ => "bin",
        };

        let safe_name = sanitize_bundle_filename(&bundle_id);
        let filename = format!("{safe_name}.{ext}");
        let payload_path = self.storage.files_dir().join(&filename);

        // Ensure storage capacity under quota policy before saving to disk
        if let Err(e) = self.storage.ensure_capacity(payload_slice.len() as u64) {
            warn!(
                "FilesService: storage quota exceeded for incoming bundle {}: {}",
                bundle_id, e
            );
            return Ok(());
        }

        // Save payload file to disk without blocking the async runtime
        if let Err(e) = tokio::fs::write(&payload_path, payload_slice).await {
            error!(
                "FilesService: failed to write payload file to {}: {}",
                payload_path.display(),
                e
            );
            return Ok(());
        }

        let record = FileBundleRecord {
            id: bundle_id.clone(),
            source_eid: source_eid.to_string(),
            destination_eid: dest_eid.to_string(),
            creation_timestamp,
            lifetime_secs,
            expiry_timestamp,
            size: payload_slice.len() as u64,
            payload_path: payload_path.to_string_lossy().to_string(),
            mime_type,
            is_sticky: false,
            is_read: false,
            created_at,
            auth_status: auth_status.as_str().to_string(),
            delivery_status: None,
            delivery_reason: None,
            delivery_updated_at: None,
        };

        if let Err(e) = self.storage.insert_file_bundle(&record) {
            warn!(
                "FilesService: failed to insert bundle record {}: {}",
                bundle_id, e
            );
            if let Err(rm_err) = tokio::fs::remove_file(&payload_path).await {
                warn!(
                    "FilesService: failed to clean up orphan file {} after DB insert failure: {}",
                    payload_path.display(),
                    rm_err
                );
            }
        } else {
            info!(
                "Stored new file bundle from {} (id: {}, bytes: {})",
                source_eid,
                bundle_id,
                payload_slice.len()
            );
            self.service_tracker.broadcast_bundles_update();
            self.service_tracker.broadcast_file_received(&record);
        }

        Ok(())
    }
}

#[async_trait]
impl Service for FilesService {
    async fn on_register(&self, _source: &Eid, sink: Box<dyn ServiceSink>) {
        let _ = self.sink.set(sink);
        debug!("FilesService registered with BPA");
    }

    async fn on_unregister(&self) {
        debug!("FilesService unregistered from BPA");
    }

    async fn on_status_notify(
        &self,
        bundle_id: &Id,
        from: &Eid,
        kind: StatusNotify,
        reason: ReasonCode,
        timestamp: Option<OffsetDateTime>,
    ) {
        let (status_str, reason_u64, ts_ms) = status_notify_to_parts(kind, reason, timestamp);
        let key = bundle_id.to_key();

        match self
            .storage
            .update_file_delivery_status(&key, status_str, reason_u64, ts_ms)
        {
            Ok(true) => {
                debug!(
                    bundle_id = %key,
                    from = %from,
                    delivery_status = status_str,
                    reason = reason_u64,
                    "Updated file bundle delivery receipt"
                );
                self.service_tracker.broadcast_bundles_update();
            }
            Ok(false) => debug!(
                bundle_id = %key,
                delivery_status = status_str,
                "Ignored file status notification (unknown bundle or non-progressive status)"
            ),
            Err(err) => warn!(
                bundle_id = %key,
                delivery_status = status_str,
                "Failed to persist file delivery receipt: {err}"
            ),
        }
    }

    async fn on_deliver(
        &self,
        bundle_id: &Id,
        _expiry: OffsetDateTime,
        total_len: u64,
        stream: &mut dyn Receiver<Segment>,
    ) -> ServiceResult<()> {
        debug!("FilesService on_deliver called for bundle_id={bundle_id:?}, len={total_len}");
        let max_bytes = self.storage.max_storage_bytes();
        if total_len > max_bytes {
            warn!(
                bundle_id = ?bundle_id,
                total_len,
                max_storage_bytes = max_bytes,
                "FilesService: dropping incoming bundle before buffering because total_len exceeds storage quota"
            );
            return Ok(());
        }
        let data = buffer_stream(stream, total_len).await?;
        self.handle_delivered_bundle(data).await
    }
}

/// Sanitizes a bundle ID into a bounded ASCII filename stem (`<= 96` characters)
/// combining up to 79 safe ASCII characters with a 16-hex-char SHA-256 prefix
/// to prevent `ENAMETOOLONG` and path traversal while remaining collision-resistant.
fn sanitize_bundle_filename(bundle_id: &str) -> String {
    let digest = Sha256::digest(bundle_id.as_bytes());
    let mut hash_hex = String::with_capacity(16);
    for byte in &digest[..8] {
        use std::fmt::Write as _;
        let _ = write!(hash_hex, "{byte:02x}");
    }

    let prefix: String = bundle_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(79)
        .collect();

    if prefix.is_empty() {
        hash_hex
    } else {
        format!("{prefix}_{hash_hex}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dtnpm_core::MockBpa;
    use dtnpm_core::security::{Direction, KeyEntry, sign_bundle_with_key};
    use hardy_bpa::stream::RecvError;
    use hardy_bpv7::builder::Builder;
    use hardy_bpv7::creation_timestamp::CreationTimestamp;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    fn make_files_service(
        keystore: Arc<DirectionalKeyStore>,
        max_quota: Option<u64>,
    ) -> (FilesService, InternalStorage, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let storage = match max_quota {
            Some(q) => InternalStorage::new_disk_with_quota(tmp.path(), q).unwrap(),
            None => InternalStorage::new_disk(tmp.path()).unwrap(),
        };
        let bpa = Arc::new(MockBpa::new("dtn://stata/"));
        let tracker = ServiceTracker::new(bpa);
        (
            FilesService::new(storage.clone(), tracker, keystore),
            storage,
            tmp,
        )
    }

    fn build_file_bundle(src: &str, dst: &str, payload: &[u8], lifetime_secs: u64) -> Vec<u8> {
        let src_eid: Eid = src.parse().unwrap();
        let dst_eid: Eid = dst.parse().unwrap();
        let (_bundle, raw) = Builder::new(src_eid, dst_eid)
            .with_lifetime(Duration::from_secs(lifetime_secs))
            .with_payload(payload.to_vec().into())
            .build(CreationTimestamp::now())
            .unwrap();
        raw.into_vec()
    }

    #[tokio::test]
    async fn strict_policy_rejects_unsigned_missing_key_and_invalid_accepts_valid() {
        let keystore = Arc::new(DirectionalKeyStore::empty());
        keystore.set_policy(VerifyPolicy::Strict).unwrap();
        let valid_secret = b"files_inbound_secret_key_32_byte".to_vec();
        keystore
            .add_key(
                Direction::Inbound,
                "dtn://f4jxq/*".to_string(),
                valid_secret.clone(),
                None,
            )
            .unwrap();

        let (service, storage, _tmp) = make_files_service(keystore, None);

        // 1. Unsigned bundle must be dropped
        let unsigned = build_file_bundle("dtn://f4jxq/files", "dtn://stata/files", b"hello", 3600);
        service
            .handle_delivered_bundle(Bytes::from(unsigned.clone()))
            .await
            .unwrap();
        assert!(storage.list_file_bundles().unwrap().is_empty());

        // 2. MissingKey bundle must be dropped
        let unknown_raw =
            build_file_bundle("dtn://unknown/files", "dtn://stata/files", b"unknown", 3600);
        let unknown_parsed = parse(Bytes::from(unknown_raw.clone())).unwrap();
        let unknown_key = KeyEntry {
            id: "u1".into(),
            direction: Direction::Outbound,
            pattern: "dtn://stata/*".into(),
            label: None,
            secret: b"other_secret_key_32_bytes_123456".to_vec(),
            created_at: "now".into(),
        };
        let missing_signed =
            sign_bundle_with_key(&unknown_parsed.bundle, &unknown_raw, &unknown_key, None).unwrap();
        service
            .handle_delivered_bundle(Bytes::from(missing_signed))
            .await
            .unwrap();
        assert!(storage.list_file_bundles().unwrap().is_empty());

        // 3. Wrong secret (Invalid) must be dropped
        let parsed = parse(Bytes::from(unsigned.clone())).unwrap();
        let wrong_key = KeyEntry {
            id: "bad".into(),
            direction: Direction::Outbound,
            pattern: "dtn://stata/*".into(),
            label: None,
            secret: b"wrong_secret_key_32_bytes_123456".to_vec(),
            created_at: "now".into(),
        };
        let invalid_signed =
            sign_bundle_with_key(&parsed.bundle, &unsigned, &wrong_key, None).unwrap();
        service
            .handle_delivered_bundle(Bytes::from(invalid_signed))
            .await
            .unwrap();
        assert!(storage.list_file_bundles().unwrap().is_empty());

        // 4. Valid signature must be accepted
        let good_key = KeyEntry {
            id: "good".into(),
            direction: Direction::Outbound,
            pattern: "dtn://stata/*".into(),
            label: None,
            secret: valid_secret,
            created_at: "now".into(),
        };
        let valid_signed =
            sign_bundle_with_key(&parsed.bundle, &unsigned, &good_key, None).unwrap();
        service
            .handle_delivered_bundle(Bytes::from(valid_signed))
            .await
            .unwrap();
        assert_eq!(storage.list_file_bundles().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn warn_and_ignore_policies_accept_unsigned_bundles() {
        for policy in [VerifyPolicy::Warn, VerifyPolicy::Ignore] {
            let keystore = Arc::new(DirectionalKeyStore::empty());
            keystore.set_policy(policy).unwrap();
            let (service, storage, _tmp) = make_files_service(keystore, None);
            let unsigned =
                build_file_bundle("dtn://f4jxq/files", "dtn://stata/files", b"opt-in", 3600);
            service
                .handle_delivered_bundle(Bytes::from(unsigned))
                .await
                .unwrap();
            assert_eq!(
                storage.list_file_bundles().unwrap().len(),
                1,
                "policy {policy:?} should accept unsigned"
            );
        }
    }

    #[tokio::test]
    async fn huge_bundle_lifetime_is_clamped_without_overflow() {
        let keystore = Arc::new(DirectionalKeyStore::empty());
        keystore.set_policy(VerifyPolicy::Warn).unwrap();
        let (service, storage, _tmp) = make_files_service(keystore, None);
        let raw = build_file_bundle(
            "dtn://f4jxq/files",
            "dtn://stata/files",
            b"clamped",
            u64::MAX / 1000,
        );
        service
            .handle_delivered_bundle(Bytes::from(raw))
            .await
            .unwrap();
        let files = storage.list_file_bundles().unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].lifetime_secs, 365 * 86_400);
        assert_eq!(
            files[0].expiry_timestamp - files[0].creation_timestamp,
            365 * 86_400 * 1000
        );
    }

    struct TrackingRejectStream {
        recv_called: Arc<AtomicBool>,
    }

    #[async_trait]
    impl Receiver<Segment> for TrackingRejectStream {
        async fn recv(&mut self) -> core::result::Result<Segment, RecvError> {
            self.recv_called.store(true, Ordering::SeqCst);
            Err(RecvError)
        }
    }

    #[tokio::test]
    async fn on_deliver_rejects_oversized_bundle_before_buffering_stream() {
        let keystore = Arc::new(DirectionalKeyStore::empty());
        let (service, _storage, _tmp) = make_files_service(keystore, Some(1024));
        let recv_called = Arc::new(AtomicBool::new(false));
        let mut stream = TrackingRejectStream {
            recv_called: recv_called.clone(),
        };
        let raw = build_file_bundle("dtn://f4jxq/files", "dtn://stata/files", b"small", 3600);
        let parsed = parse(Bytes::from(raw)).unwrap();

        service
            .on_deliver(
                &parsed.bundle.primary.id,
                OffsetDateTime::now_utc(),
                2048,
                &mut stream,
            )
            .await
            .unwrap();

        assert!(
            !recv_called.load(Ordering::SeqCst),
            "buffer_stream must not pull segments when total_len > max_storage_bytes"
        );
    }

    #[test]
    fn sanitize_bundle_filename_bounds_length_and_avoids_traversal() {
        let long_a = format!("{}alpha", "a".repeat(250));
        let long_b = format!("{}bravo", "a".repeat(250));
        let safe_a = sanitize_bundle_filename(&long_a);
        let safe_b = sanitize_bundle_filename(&long_b);
        assert!(safe_a.len() <= 96);
        assert!(safe_b.len() <= 96);
        assert_ne!(safe_a, safe_b);
        assert!(
            safe_a
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );

        let traversal = sanitize_bundle_filename("../../etc/passwd\0foo");
        assert!(!traversal.contains('.'));
        assert!(!traversal.contains('/'));
        assert!(!traversal.contains('\\'));
        assert!(!traversal.contains('\0'));
    }

    #[tokio::test]
    async fn orphan_file_removed_when_db_insert_fails() {
        let keystore = Arc::new(DirectionalKeyStore::empty());
        let (service, storage, _tmp) = make_files_service(keystore, None);
        {
            let conn_arc = storage.connection();
            let conn = conn_arc.lock().unwrap();
            conn.execute_batch(
                "CREATE TRIGGER fail_file_insert BEFORE INSERT ON files_bundles \
                 BEGIN SELECT RAISE(ABORT, 'simulated db error'); END;",
            )
            .unwrap();
        }

        let raw = build_file_bundle("dtn://f4jxq/files", "dtn://stata/files", b"orphan", 3600);
        service
            .handle_delivered_bundle(Bytes::from(raw))
            .await
            .unwrap();

        let entries: Vec<_> = std::fs::read_dir(storage.files_dir()).unwrap().collect();
        assert!(
            entries.is_empty(),
            "payload file must be removed when DB insert fails"
        );
    }

    #[tokio::test]
    async fn status_notify_persists_delivery_receipt_for_file_bundle() {
        let keystore = Arc::new(DirectionalKeyStore::empty());
        let (service, storage, _tmp) = make_files_service(keystore, None);

        let src: Eid = "dtn://stata/files".parse().unwrap();
        let dst: Eid = "dtn://statb/files".parse().unwrap();
        let (bundle, _) = Builder::new(src, dst)
            .with_lifetime(Duration::from_secs(3600))
            .with_payload(b"file receipt test".to_vec().into())
            .build(CreationTimestamp::now())
            .unwrap();
        let bundle_key = bundle.primary.id.to_key();

        let payload_path = storage.files_dir().join("receipt.bin");
        std::fs::write(&payload_path, b"file receipt test").unwrap();
        let record = FileBundleRecord {
            id: bundle_key.clone(),
            source_eid: "dtn://stata/files".to_string(),
            destination_eid: "dtn://statb/files".to_string(),
            creation_timestamp: 1_700_000_000_000,
            lifetime_secs: 3600,
            expiry_timestamp: 1_700_003_600_000,
            size: 17,
            payload_path: payload_path.to_string_lossy().to_string(),
            mime_type: "application/octet-stream".to_string(),
            is_sticky: false,
            is_read: true,
            created_at: "2026-11-22T12:00:00Z".to_string(),
            auth_status: "valid".to_string(),
            delivery_status: None,
            delivery_reason: None,
            delivery_updated_at: None,
        };
        storage.insert_file_bundle(&record).unwrap();

        let relay_from: Eid = "dtn://relay1/".parse().unwrap();
        let ts1 = OffsetDateTime::from_unix_timestamp(1_700_000_100).unwrap();
        service
            .on_status_notify(
                &bundle.primary.id,
                &relay_from,
                StatusNotify::Forwarded,
                ReasonCode::ForwardedOverUnidirectionalLink,
                Some(ts1),
            )
            .await;

        let mid = storage.get_file_bundle(&bundle_key).unwrap().unwrap();
        assert_eq!(mid.delivery_status.as_deref(), Some("forwarded"));
        assert_eq!(mid.delivery_reason, Some(2));
        assert_eq!(mid.delivery_updated_at, Some(1_700_000_100_000));

        let dest_from: Eid = "dtn://statb/".parse().unwrap();
        let ts2 = OffsetDateTime::from_unix_timestamp(1_700_000_200).unwrap();
        service
            .on_status_notify(
                &bundle.primary.id,
                &dest_from,
                StatusNotify::Delivered,
                ReasonCode::NoAdditionalInformation,
                Some(ts2),
            )
            .await;

        let done = storage.get_file_bundle(&bundle_key).unwrap().unwrap();
        assert_eq!(done.delivery_status.as_deref(), Some("delivered"));
        assert_eq!(done.delivery_reason, Some(0));
        assert_eq!(done.delivery_updated_at, Some(1_700_000_200_000));
    }
}
