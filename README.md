# dtnpm-mod-files

Fullstack **DTN File Transfer** plugin for **DTN Postmaster** (`dtnpm`), implementing `dtnpm_web_core::WebModule`.

## Features

- **BPA Service (`FilesService`)**: Registers `dtn://<node>/files`, persists incoming file bundles with BPsec auth metadata, and broadcasts real-time SSE notifications.
- **SQLite Persistence**: Provides `migrations/0001_files_schema.sql` (`file_transfers`) over the shared SQLite `Arc<Mutex<Connection>>`.
- **HTTP API (`/api/files/v1/...`)**: Multipart/raw file transmission, received/sent transfer listing, payload download, unread counters, and mark-read endpoints.
- **Standalone Mode**: Includes `dtnpm-mod-files` daemon (`cli` feature) and `examples/test_server.rs`.
- **WebUI Component (`ui/`)**: Preact `<FilesView />` component and Playwright E2E spec.

## Build and Test

Requires sibling checkouts of `../dtnpm-core`, `../dtnpm-web-core`, and `../../hardy/*`.

```bash
cargo test --all-targets
```

Run the standalone mock server example:

```bash
cargo run --example test_server
```

## License

Dual-licensed under the Apache License, Version 2.0 or the MIT License (crate metadata: `Apache-2.0`). See [LICENSE.md](LICENSE.md).
