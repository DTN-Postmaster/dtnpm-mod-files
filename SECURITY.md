# Security Policy — `dtnpm-mod-files`

## Supported Versions

| Version | Supported | Security Updates |
|---------|-----------|------------------|
| 0.2.x   | Yes       | Active           |
| < 0.2   | No        | End of Life      |

## Security Architecture & EU Cyber Resilience Act (CRA) Alignment

`dtnpm-mod-files` implements the DTN BPv7 Files service (`dtn://<station>/files`), payload disk/SQLite persistence, HTTP Range download routes, and Preact UI for `dtn-postmaster`:

- **Memory Safety & Panic-Free Production Targets**: Built in Rust (`edition = "2024"`) and gated in CI with `-D warnings -D clippy::unwrap_used -D clippy::panic`.
- **BPSec Enforcement (`src/files_service.rs`)**: Inbound bundles are verified against `DirectionalKeyStore`. Under `VerifyPolicy::Strict`, only bundles with `AuthStatus::Valid` are accepted (`Unsigned`, `MissingKey`, and `Invalid` are dropped with a warning). Explicit opt-in policies (`VerifyPolicy::Warn`, `VerifyPolicy::Ignore`) remain available for unauthenticated interoperability.
- **Pre-Stream Quota & Safe Disk Persistence (`src/files_service.rs`)**: `on_deliver` enforces `total_len <= storage.max_storage_bytes()` prior to calling `buffer_stream`; filenames are sanitized into bounded ASCII stems (`<= 96` characters) combining safe characters with a 16-hex SHA-256 prefix; writes use async `tokio::fs::write`; and files written prior to a failed `insert_file_bundle` call are removed to prevent orphan disk bloat.
- **Bounded Lifetime Arithmetic (`src/files_service.rs`)**: Bundle lifetimes are clamped to 1 year (`365 * 86_400` seconds) and expiration timestamps use saturating arithmetic (`saturating_mul`, `saturating_add`).
- **XSS-Safe Previews & Markdown Rendering (`ui/src/utils/markdown.ts`, `dtnpm-web-core`)**: Markdown rendering isolates placeholders with per-invocation random nonces, disables SVG/remote images by default with explicit `allowExternalImages` opt-in (`FilesView.tsx` enables `{ allowImages: true }` for raster/relative previews only), and payload downloads rely on `dtnpm-web-core` forcing `attachment` `Content-Disposition` + `default-src 'none'; sandbox` CSP on `image/svg+xml` and HTML payloads.
- **Standalone Secret Handling (`src/main.rs`)**: Supports `--token-file` (`DTN_HDY_TOKEN_FILE`) alongside `--token` (`DTN_HDY_TOKEN`) and Unix Domain Sockets (`--socket`).
- **Supply Chain & VEX**: `serde_yaml` (`RUSTSEC-2024-0449`) is not present in the dependency graph. `RUSTSEC-2026-0459` (`libsqlite3-sys` SQLite `session` extension) is documented in `.cargo/audit.toml` and `deny.toml` as VEX Not Affected because `dtnpm-mod-files` and `dtnpm-core` do not enable the SQLite `session` extension.
- **SBOM & CI Hardening**: CI runs with `permissions: contents: read`, SHA-pinned GitHub Actions, and CycloneDX SBOM generation.

## Reporting a Vulnerability

Do **not** open a public GitHub issue for security vulnerabilities.
Report vulnerabilities privately via GitHub Security Advisories for `dtn-postmaster/dtnpm-mod-files` or by contacting the maintainers directly. Please include:

- Component version and commit hash
- Configuration flags (`verify_policy`, storage quota, standalone CLI/UI options, etc.)
- Minimal proof-of-concept or reproduction steps

We aim to acknowledge reports within 72 hours and coordinate a fix before public disclosure.
