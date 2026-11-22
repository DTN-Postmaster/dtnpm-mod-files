# Agent Guide — `dtnpm-mod-files`

## 🎯 Mission
Provide the complete fullstack DTN file transfer and storage application module (`WebModule`) for **DTN Postmaster**, including the BPA service (`FilesService`), disk quota management, SQLite persistence, the `/api/files/v1/...` REST API, and the Preact UI component `<FilesView />`.

---

## 📝 Documentation & User Interface Language

- **English by Default (Docs & UI)**:
  - **Documentation**: English must be used for all documentation, unless the file is suffixed with `.fr.md` or another language is explicitly requested in the prompt.
  - **User Interface (100% English)**: Every user-facing string in `ui/src/` and `ui/tests/` (labels, placeholders, buttons, transfer/quota statuses, `SecurityBadge` text, `title` tooltips, `aria-label`s, dialogs, and UI error messages) MUST be in English. Keep shared components synchronized with `dtnpm-web-core/ui`.
  - **Navigation Scope**: `Files` is an operational top-level `Navbar` tab in `dtnpm-web-core` (`NavTabId: 'files'`); station-level configuration, routing, security, and filtering panels belong under `Station` sub-tabs (`ConfigView.tsx`), never added as new global navbar tabs.

---

## 🛡️ Security Frameworks and Requirements

### 1. Cyber Resilience Act (CRA) & BPSec (RFC 9172)
- **Strict BPSec Verification by Default**:
  - Under `VerifyPolicy::Strict`, any inbound file without a valid signature (`Unsigned`, `MissingKey`, `Invalid`) is immediately rejected before any physical storage on disk.
- **Bundle Lifetime Clamping**:
  - Arithmetic clamping of bundle lifetimes to prevent any `u64` overflow.
- **Supply-Chain & VEX**:
  - `cargo-audit` and `cargo-deny` audits verified; `not_affected` VEX for `RUSTSEC-2026-0459`.
  - CI actions pinned by full 40-character SHA-1.
- **Security Policy**: Documented in [SECURITY.md](SECURITY.md).

### 2. OWASP & File Storage Security
- **Pre-Quota RAM Exhaustion Protection**:
  - Check `total_len` against the remaining quota BEFORE calling `buffer_stream`: any transfer exceeding the limit is rejected on the stream without allocating a massive memory buffer.
- **Filename Sanitization & Path Traversal Prevention**:
  - Strict filename sanitization: characters restricted to `[A-Za-z0-9_-]` and length capped at 128 characters maximum (preventing `ENAMETOOLONG` and `../` directory traversals).
- **Asynchronous Writes & Orphan File Cleanup**:
  - Physical writes use `tokio::fs::write` so they never block the Tokio async executor.
  - If insertion into the SQLite metadata database fails, the temporary file written to disk is immediately deleted to prevent orphan accumulation.

---

## 🦀 Rust Engineering Standards

1. **Zero Panic / Zero Unwrap Rule**:
   - All code under `src/` must be 100% free of `.unwrap()`, `.expect()`, and the `panic!` macro.
   - Strict linter: `-D warnings -D clippy::unwrap_used -D clippy::panic`.
2. **Mandatory ADRs**:
   - Any Files/BPSec architectural decision must be recorded under `adr/` and indexed in [adr/README.md](adr/README.md).

---

## ⚡ Critical Verification Commands

- `cargo fmt --all -- --check`
- `cargo clippy --offline --workspace --lib --bins -- -D warnings -D clippy::unwrap_used -D clippy::panic`
- `cargo test --offline --all-features`
- `cd ui && pnpm test:e2e` (and `cd ../dtnpm-web-core/ui && pnpm build && pnpm test:e2e` when updating shared UI views)
- `./scripts/check-security.sh --strict`
- `./scripts/check-coverage.sh --summary-only`
