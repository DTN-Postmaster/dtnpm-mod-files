//! DTN Files fullstack module implementing [`dtnpm_web_core::WebModule`].

use std::sync::Arc;

use axum::Router;
use axum::routing::{get, post};
use hardy_async::async_trait;
use hardy_bpv7::eid::Service;
use rusqlite::Connection;

use dtnpm_web_core::{ModuleContext, WebModule};

pub mod files_service;
pub mod routes;

pub use dtnpm_core::internal_storage::FileBundleRecord;
pub use files_service::FilesService;

/// SQL schema applied on the shared SQLite connection when registering [`FilesModule`].
pub const FILES_SCHEMA_SQL: &str = include_str!("../migrations/0001_files_schema.sql");

/// Modular DTN Files plugin exposing `/api/files/v1/...` routes and registering `dtn://<node>/files`.
#[derive(Debug, Default, Clone)]
pub struct FilesModule;

impl FilesModule {
    /// Creates a new [`FilesModule`] instance.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl WebModule for FilesModule {
    fn id(&self) -> &'static str {
        "files"
    }

    fn name(&self) -> &'static str {
        "DTN Files"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn services(&self) -> Vec<String> {
        vec!["files".to_string()]
    }

    fn migrate(&self, conn: &Connection) -> anyhow::Result<()> {
        conn.execute_batch(FILES_SCHEMA_SQL)?;
        Ok(())
    }

    async fn register_bpa(&self, ctx: &ModuleContext) -> anyhow::Result<()> {
        let files_service = Arc::new(FilesService::new(
            ctx.internal_storage.clone(),
            ctx.service_tracker.clone(),
            ctx.keystore.clone(),
        ));
        ctx.service_tracker
            .register_service_with_type(
                Service::Dtn("files".into()),
                files_service,
                "Built-in Files",
            )
            .await
            .map_err(|e| anyhow::anyhow!("Failed to register Files service: {e}"))?;
        Ok(())
    }

    fn api_routes(&self, ctx: &ModuleContext) -> Router<()> {
        Router::new()
            .route("/", get(routes::list_files))
            .route("/unread", get(routes::get_unread_count))
            .route(
                "/{id}",
                get(routes::get_file_details).delete(routes::delete_file_bundle),
            )
            .route("/{id}/payload", get(routes::download_file_payload))
            .route("/{id}/sticky", post(routes::toggle_file_sticky))
            .route("/{id}/read", post(routes::mark_file_read))
            .with_state(ctx.clone())
    }
}
