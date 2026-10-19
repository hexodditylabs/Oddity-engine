//! Oddity Engine's MCP server (`roadmap/commands.md`, "MCP").
//!
//! It contains no list of commands: it builds itself from the command
//! registry, so a command an extension registers is a tool straight away.
//!
//! | MCP | Comes from |
//! | --- | --- |
//! | Tools | Every command and query (`entity.spawn` is the tool `entity_spawn`), with its description, argument schema and hints |
//! | Resources | The level tree, selection, history, node types and commands, read through their queries |
//! | Notifications | Every executed command, whoever ran it |
//!
//! Tool calls run as `Source::Mcp`, so an agent's changes are labelled in
//! History and undo like your own. `rmcp` runs on Tokio, so each call
//! crosses into the Bevy world through a channel and runs in an exclusive
//! system, in order with the editor's own edits (see `bridge`).
//!
//! `oddity-editor mcp <project>` serves it on stdin and stdout, headless.

mod bridge;
mod server;
pub mod tools;

use bevy_app::{App, AppExit, Plugin, Update};
use bevy_ecs::world::World;
use rmcp::{RoleServer, transport::IntoTransport};
use serde_json::Value;
use tokio::sync::mpsc;

pub use bridge::WorldBridge;

/// Why serving MCP stopped with an error.
#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    /// The client and server never agreed to talk.
    #[error("MCP handshake failed: {0}")]
    Handshake(String),
    /// The connection failed after the handshake.
    #[error("MCP server stopped: {0}")]
    Stopped(String),
}

/// Lets an MCP server reach this app's world. With [`McpPlugin::stdio`], it
/// also serves MCP on stdin and stdout, and quits the app when the client
/// hangs up.
#[derive(Default)]
pub struct McpPlugin {
    stdio: bool,
}

impl McpPlugin {
    /// Serve MCP on stdin and stdout, from a background thread.
    pub fn stdio() -> Self {
        Self { stdio: true }
    }
}

impl Plugin for McpPlugin {
    fn build(&self, app: &mut App) {
        let (handle, jobs, executed) = bridge::resources();
        app.insert_resource(handle)
            .insert_resource(jobs)
            .insert_resource(executed)
            .add_systems(Update, (bridge::run_jobs, bridge::report_executed));
        if self.stdio {
            let server = McpServer::new(app.world());
            std::thread::spawn(move || serve_stdio(server));
        }
    }
}

/// Serve on stdin and stdout until the client hangs up, then quit the app.
fn serve_stdio(server: McpServer) {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            tracing::error!("could not start the MCP server: {error}");
            return;
        }
    };
    runtime.block_on(async move {
        let bridge = server.bridge.clone();
        if let Err(error) = server.serve(rmcp::transport::stdio()).await {
            tracing::error!("{error}");
        }
        bridge
            .run(|world| {
                world.write_message(AppExit::Success);
            })
            .await;
    });
}

/// What an MCP server needs from one app: the way into its world and the
/// stream of commands executed there.
pub struct McpServer {
    bridge: WorldBridge,
    executed: Option<mpsc::UnboundedReceiver<Value>>,
}

impl McpServer {
    /// The server for the app owning `world`, which must have [`McpPlugin`].
    /// Only the first server made for an app is told about executed
    /// commands.
    ///
    /// # Panics
    ///
    /// If the app has no [`McpPlugin`].
    pub fn new(world: &World) -> Self {
        Self {
            bridge: world.resource::<WorldBridge>().clone(),
            executed: world.resource::<bridge::Executed>().take(),
        }
    }

    /// Serve MCP over `transport` until the client hangs up. Runs on the
    /// current Tokio runtime; the app must keep updating meanwhile, since
    /// that is when requests are answered.
    pub async fn serve<T, E, A>(self, transport: T) -> Result<(), ServeError>
    where
        T: IntoTransport<RoleServer, E, A>,
        E: std::error::Error + Send + Sync + 'static,
    {
        server::serve(self.bridge, self.executed, transport).await
    }
}
