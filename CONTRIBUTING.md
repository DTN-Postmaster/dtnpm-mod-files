# Contributing Guide — `dtnpm-mod-files`

This guide outlines the development rules and security requirements for the `dtnpm-mod-files` file transfer module.

---

## 1. Core Principles

- **Zero Panic / Zero Unwrap**: No `.unwrap()` or `.expect()` calls in production code.
- **Storage & Quota Security**:
  - Always check the bundle size before buffering it into memory.
  - Always sanitize the filename and bound its length.
  - Always clean up the file on disk if the SQLite transaction fails.
- **Architectural Traceability (ADR)**:
  - Any change to storage management or the API must be documented in an ADR file under `adr/` and indexed in `adr/README.md`.

---

## 2. Local Verification

```bash
cargo fmt --all -- --check
cargo clippy --offline --workspace --lib --bins -- -D warnings -D clippy::unwrap_used -D clippy::panic
cargo test --offline --all-features
./scripts/check-security.sh --strict
```
