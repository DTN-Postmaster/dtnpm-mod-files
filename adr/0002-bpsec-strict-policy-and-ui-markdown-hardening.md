# ADR 0002: BPSec Strict Verification, Token File Auth, and Markdown Hardening

- **Status**: Accepted
- **Scope**: `dtnpm-mod-files` (`src/files_service.rs`, `src/main.rs`, `ui/src/utils/markdown.ts`, `tests/files_service_test.rs`)

## Context
- `FilesService` under `VerifyPolicy::Strict` previously checked `auth_status == AuthStatus::Invalid`, accepting unauthenticated `Unsigned` and `MissingKey` bundles.
- Standalone gRPC authentication lacked `--token-file` support.
- `ui/src/utils/markdown.ts` used static placeholders (`@@HDYTOKEN...@@`), complex nested regexes, post-escape autolinks, and permitted SVG data URIs plus unconditional external HTTP(S) image fetches (note `FilesView.tsx` invokes `renderMarkdown` with `{ allowImages: true }`).

## Decision
1. Require `auth_status == AuthStatus::Valid` under `VerifyPolicy::Strict` while preserving opt-in `VerifyPolicy::Warn` and `VerifyPolicy::Ignore`.
2. Add `--token-file <PATH>` (`DTN_HDY_TOKEN_FILE`) alongside `--token` in `src/main.rs`.
3. Update `ui/src/utils/markdown.ts` to match `dtnpm-web-core`: per-call cryptographic nonce placeholders, linear link regex, autolinks prior to `escapeHtml`, and `isSafeImageUrl` restricted to raster `data:image` URIs and relative paths unless `allowExternalImages: true` is opted into.
4. Align `tests/files_service_test.rs` with `dtnpm-web-core`'s hardened `attachment` `Content-Disposition` for `image/svg+xml` downloads.

## Consequences
- `Strict` BPSec policy blocks unauthenticated/unknown-key file transfers cleanly without breaking explicit opt-in testing modes.
- Markdown file previews and SVG downloads are hardened against stored XSS and unsolicited external image fetches.
