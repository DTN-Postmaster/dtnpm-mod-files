# ADR 0001: Pre-Stream Quota Checks, Bounded Filenames, Async Writes, and Lifetime Clamping

- **Status**: Accepted
- **Scope**: `dtnpm-mod-files` (`src/files_service.rs`, `Cargo.toml`)

## Context
Four storage and resource-exhaustion issues were identified in `FilesService`:
1. `on_deliver` buffered the entire incoming stream via `buffer_stream` before checking `InternalStorage` quota, allowing an oversized incoming bundle stream to exhaust memory.
2. Bundle IDs containing long EIDs or punctuation produced unsanitized, unbounded filenames that could trigger `ENAMETOOLONG` on POSIX filesystems.
3. Synchronous `std::fs::write` blocked Tokio worker threads, and failed `insert_file_bundle` calls left orphaned payload files on disk.
4. `lifetime_secs as i64 * 1000` could overflow on extreme BPv7 lifetimes.

## Decision
1. Reject bundles in `on_deliver` when `total_len > self.storage.max_storage_bytes()` before calling `buffer_stream`.
2. Sanitize bundle IDs via `sanitize_bundle_filename`, keeping up to 79 ASCII alphanumeric/`-`/`_` characters followed by `_` and a 16-hex-character SHA-256 prefix (`<= 96` characters total).
3. Persist payloads using `tokio::fs::write(...).await`, and remove the written file via `tokio::fs::remove_file` if `insert_file_bundle` returns an error.
4. Clamp incoming `lifetime_secs` to `365 * 86_400` and compute `expiry_timestamp` using `saturating_mul` and `saturating_add`.

## Consequences
- Oversized incoming streams are dropped prior to memory allocation, async executors are not blocked by disk writes, disk/SQLite state stays consistent on DB errors, and long bundle IDs remain unique without hitting filesystem filename limits.
