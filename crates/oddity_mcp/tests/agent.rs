//! K6's exit test: an MCP client spawns a node, sets a field, reads it back
//! with a query and undoes, all through tools; and a command an extension
//! registers is a tool without any change to `oddity_mcp`.
//!
//! The app runs on the test's thread, updating until the client, on its
//! own thread, is done: as in the editor, requests are answered when the
//! app updates.

// The client side of executed-command notifications; see `server.rs`.
#![allow(deprecated)]

use std::borrow::Cow;

use bevy_app::App;
use bevy_ecs::world::World;
use oddity_core::{
    Args, CommandError, CommandRegistry, CommandSpec, EditorCommand, History, Invocation,
    NamedCommand, headless_app,
    testing::{Health, Speed, standard_level},
};
use oddity_mcp::{McpPlugin, McpServer};
use rmcp::{
    ClientHandler, RoleClient, ServiceExt,
    model::{
        CallToolRequestParams, CallToolResult, ClientCapabilities, ClientConfig, Implementation,
        LoggingMessageNotificationParam, ReadResourceRequestParams, ResourceContents,
    },
    service::{NotificationContext, RunningService},
};
use serde_json::{Value, json};
use tokio::sync::mpsc;

/// An extension's command, registered without `oddity_mcp` knowing it.
struct Ping;

impl EditorCommand for Ping {
    fn label(&self) -> Cow<'_, str> {
        "Ping".into()
    }

    fn apply(&mut self, _world: &mut World) -> Result<(), CommandError> {
        Ok(())
    }

    fn undo(&mut self, _world: &mut World) -> Result<(), CommandError> {
        Ok(())
    }

    fn invocation(&self) -> Invocation {
        Invocation::new("test.ping", json!({}))
    }

    fn undoable(&self) -> bool {
        false
    }

    fn output(&self) -> Value {
        json!({ "pong": true })
    }
}

impl NamedCommand for Ping {
    const SPEC: CommandSpec = CommandSpec::action("test.ping", "Answer with pong.", &[]);

    fn from_args(_world: &mut World, _args: &Args) -> Result<Self, CommandError> {
        Ok(Ping)
    }
}

/// The agent: it introduces itself, and keeps the notifications it gets.
struct Agent {
    notices: mpsc::UnboundedSender<LoggingMessageNotificationParam>,
}

impl ClientHandler for Agent {
    fn get_info(&self) -> ClientConfig {
        ClientConfig::new(
            ClientCapabilities::default(),
            Implementation::new("test-agent", "1.0"),
        )
    }

    async fn on_logging_message(
        &self,
        params: LoggingMessageNotificationParam,
        _context: NotificationContext<RoleClient>,
    ) {
        let _ = self.notices.send(params);
    }
}

/// A headless editor with the standard test level and an extension's command.
fn editor() -> App {
    let mut app = headless_app();
    app.register_type::<Health>()
        .register_type::<Speed>()
        .add_plugins(McpPlugin::default());
    app.world_mut()
        .resource_mut::<CommandRegistry>()
        .register_named::<Ping>();
    standard_level(app.world_mut());
    app
}

async fn call(
    client: &RunningService<RoleClient, Agent>,
    tool: &str,
    args: Value,
) -> CallToolResult {
    let Value::Object(args) = args else {
        panic!("arguments are an object");
    };
    client
        .call_tool(CallToolRequestParams::new(tool.to_owned()).with_arguments(args))
        .await
        .unwrap_or_else(|error| panic!("{tool}: {error}"))
}

/// A successful call's JSON answer.
fn answer(result: &CallToolResult) -> Value {
    assert_ne!(result.is_error, Some(true), "{result:?}");
    result.structured_content.clone().unwrap_or_else(|| {
        let text = &result.content[0].as_text().expect("text content").text;
        serde_json::from_str(text).expect("JSON text")
    })
}

#[test]
fn an_agent_edits_reads_back_and_undoes_through_tools() {
    let mut app = editor();
    let server = McpServer::new(app.world());

    let agent = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async move {
            let (server_io, client_io) = tokio::io::duplex(1 << 16);
            let served = tokio::spawn(server.serve(server_io));
            let (notices, mut received) = mpsc::unbounded_channel();
            let client = Agent { notices }.serve(client_io).await.unwrap();

            // Tools come from the registry, the extension's included.
            let tools = client.list_all_tools().await.unwrap();
            let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
            for expected in ["entity_spawn", "component_set", "history_undo", "test_ping"] {
                assert!(
                    names.contains(&expected),
                    "{expected} missing from {names:?}"
                );
            }
            let level_tree = tools.iter().find(|t| t.name == "level_tree").unwrap();
            assert_eq!(
                level_tree.annotations.as_ref().unwrap().read_only_hint,
                Some(true)
            );
            assert_eq!(
                answer(&call(&client, "test_ping", json!({})).await),
                json!({"pong": true})
            );

            // Spawn a node, give it a health bar and set it.
            let spawned = answer(&call(&client, "entity_spawn", json!({"name": "Crate"})).await);
            let crate_ = spawned["entity"].clone();
            call(
                &client,
                "component_add",
                json!({"entity": crate_, "component": "Health"}),
            )
            .await;
            let set =
                json!({"entity": crate_, "component": "Health", "path": "current", "value": 7.0});
            answer(&call(&client, "component_set", set).await);

            // Read it back with a query, undo, and read again.
            let get = json!({"entity": crate_, "component": "Health", "path": "current"});
            let read = answer(&call(&client, "component_get", get.clone()).await);
            assert_eq!(read["value"], json!(7.0));
            call(&client, "history_undo", json!({})).await;
            let read = answer(&call(&client, "component_get", get).await);
            assert_eq!(read["value"], json!(0.0), "undo put the default back");

            // A failing command is a tool error the agent can read.
            let failed = call(
                &client,
                "entity_rename",
                json!({"entity": 999_999, "name": "X"}),
            )
            .await;
            assert_eq!(failed.is_error, Some(true));

            // Resources read through queries.
            let tree = client
                .read_resource(ReadResourceRequestParams::new("oddity://level/tree"))
                .await
                .unwrap();
            let ResourceContents::TextResourceContents { text, .. } = &tree.contents[0] else {
                panic!("the level tree is text");
            };
            assert!(text.contains("Crate"), "{text}");

            // Every executed command was announced.
            let first = received.recv().await.expect("a notification");
            assert_eq!(first.logger.as_deref(), Some("oddity.commands"));
            assert_eq!(first.data["id"], "test.ping");

            client.cancel().await.unwrap();
            let _ = served.await;
        });
    });

    while !agent.is_finished() {
        app.update();
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    if let Err(panic) = agent.join() {
        std::panic::resume_unwind(panic);
    }

    // The agent's steps are labelled as its own in History.
    let history = app.world().resource::<History>().to_json();
    assert_eq!(
        history["undo"][0]["source"],
        json!({"kind": "mcp", "client": "test-agent"})
    );
}
