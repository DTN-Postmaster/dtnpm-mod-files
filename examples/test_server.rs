use std::sync::Arc;

use dtnpm_mod_files::FilesModule;
use dtnpm_web_core::TestServerBuilder;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let srv = TestServerBuilder::new()
        .node_id("dtn://files-dev/")
        .callsign("FILESDEV")
        .with_module(Arc::new(FilesModule::new()))
        .build()
        .await?;

    let (addr, handle) = srv.spawn_http().await?;
    println!("dtnpm-mod-files test server listening on http://{addr}");
    println!("Capabilities: http://{addr}/api/capabilities");
    println!("Files API:    http://{addr}/api/files/v1");

    tokio::signal::ctrl_c().await?;
    handle.abort();
    Ok(())
}
