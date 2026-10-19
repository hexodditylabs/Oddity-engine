//! The MCP server: tools, resources and notifications, each answered by
//! running something on the world through the [`WorldBridge`].

// MCP's `notifications/message` is marked deprecated in rmcp 3.x by
// SEP-2577, in favour of `subscriptions/listen`. It still works on the wire,
// and it is the one way to push `CommandExecuted` to clients that predate
// subscriptions, which is every client today.
#![allow(deprecated)]

use std::sync::Arc;

use oddity_core::{CommandError, CommandRegistry, EditorWorldExt, Source};
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler, ServiceExt,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
        ListResourcesResult, ListToolsResult, LoggingLevel, LoggingMessageNotificationParam,
        PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResponse,
        ReadResourceResult, Resource, ResourceContents, ServerCapabilities, ServerConfig, Tool,
        ToolAnnotations,
    },
    service::RequestContext,
    transport::IntoTransport,
};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::{
    ServeError,
    bridge::WorldBridge,
    tools::{command_for, description, hints, input_schema, tool_name},
};

/// What MCP clients see as resources, each read with a query. Only those
/// whose query is registered are listed.
const RESOURCES: &[(&str, &str, &str)] = &[
    ("oddity://level/tree", "Level tree", "level.tree"),
    ("oddity://selection", "Selection", "selection.get"),
    ("oddity://history", "History", "history.list"),
    ("oddity://node_types", "Node types", "node_types.list"),
    ("oddity://commands", "Commands", "commands.list"),
];

/// Logger name on executed-command notifications.
const EXECUTED_LOGGER: &str = "oddity.commands";

/// The MCP handler: every request goes to the world through the bridge.
#[derive(Clone)]
pub(crate) struct OddityMcp {
    bridge: WorldBridge,
}

/// Serve MCP over `transport` until the client hangs up, sending each
/// executed command from `executed` to the client as a notification.
pub(crate) async fn serve<T, E, A>(
    bridge: WorldBridge,
    executed: Option<mpsc::UnboundedReceiver<Value>>,
    transport: T,
) -> Result<(), ServeError>
where
    T: IntoTransport<RoleServer, E, A>,
    E: std::error::Error + Send + Sync + 'static,
{
    let running = OddityMcp { bridge }
        .serve(transport)
        .await
        .map_err(|error| ServeError::Handshake(error.to_string()))?;
    if let Some(mut executed) = executed {
        let peer = running.peer().clone();
        tokio::spawn(async move {
            while let Some(command) = executed.recv().await {
                let notice = LoggingMessageNotificationParam::new(LoggingLevel::Info, command)
                    .with_logger(EXECUTED_LOGGER);
                if peer.notify_logging_message(notice).await.is_err() {
                    break;
                }
            }
        });
    }
    running
        .waiting()
        .await
        .map_err(|error| ServeError::Stopped(error.to_string()))?;
    Ok(())
}

/// The world has gone, so nothing can be answered.
fn stopped() -> McpError {
    McpError::internal_error("the editor has stopped", None)
}

fn tool_error(message: impl Into<String>) -> CallToolResponse {
    CallToolResult::error(vec![ContentBlock::text(message.into())]).into()
}

fn tools(registry: &CommandRegistry) -> Vec<Tool> {
    registry
        .specs()
        .map(|spec| {
            let hints = hints(spec);
            Tool::new(
                tool_name(spec.id),
                description(spec),
                Arc::new(input_schema(spec)),
            )
            .with_annotations(
                ToolAnnotations::new()
                    .read_only(hints.read_only)
                    .destructive(hints.destructive),
            )
        })
        .collect()
}

impl ServerHandler for OddityMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_logging()
                .enable_resources()
                .enable_tools()
                .build(),
        )
        .with_server_info(Implementation::new("oddity", env!("CARGO_PKG_VERSION")))
        .with_instructions(
            "The Oddity Engine editor. Every tool is an editor command: edits can be undone \
             with history_undo, and several calls become one undo step between \
             history_group_begin and history_group_end. Entities are numbers; level_tree \
             lists them.",
        )
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let tools = self
            .bridge
            .run(|world| {
                world
                    .get_resource::<CommandRegistry>()
                    .map(tools)
                    .unwrap_or_default()
            })
            .await
            .ok_or_else(stopped)?;
        Ok(ListToolsResult::with_all_items(tools))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let client = context
            .peer
            .peer_info()
            .map_or_else(|| "agent".to_owned(), |info| info.client_info.name.clone());
        let tool = request.name.to_string();
        let args = Value::Object(request.arguments.unwrap_or_default());
        let result = self
            .bridge
            .run(move |world| {
                let id = world
                    .get_resource::<CommandRegistry>()
                    .and_then(|registry| command_for(registry, &tool))
                    .map(|spec| spec.id)
                    .ok_or_else(|| CommandError::UnknownCommand(tool.clone()))?;
                world.run_command_as(id, args, Source::Mcp { client })
            })
            .await
            .ok_or_else(stopped)?;
        Ok(match result {
            // Structured content must be an object; anything else goes as text.
            Ok(output @ Value::Object(_)) => CallToolResult::structured(output).into(),
            Ok(output) => {
                CallToolResult::success(vec![ContentBlock::text(output.to_string())]).into()
            }
            Err(error) => tool_error(error.to_string()),
        })
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let resources = self
            .bridge
            .run(|world| {
                let registry = world.get_resource::<CommandRegistry>();
                RESOURCES
                    .iter()
                    .filter(|(_, _, query)| registry.is_some_and(|r| r.spec(query).is_some()))
                    .map(|(uri, name, _)| {
                        Resource::new(*uri, *name).with_mime_type("application/json")
                    })
                    .collect::<Vec<_>>()
            })
            .await
            .ok_or_else(stopped)?;
        Ok(ListResourcesResult::with_all_items(resources))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
        let uri = request.uri.clone();
        let Some(&(_, _, query)) = RESOURCES.iter().find(|(known, _, _)| *known == uri) else {
            return Err(McpError::resource_not_found(
                format!("no resource {uri}"),
                None,
            ));
        };
        let value = self
            .bridge
            .run(move |world| world.run_command(query, json!({})))
            .await
            .ok_or_else(stopped)?
            .map_err(|error| McpError::internal_error(error.to_string(), None))?;
        Ok(ReadResourceResult::new(vec![
            ResourceContents::text(value.to_string(), uri).with_mime_type("application/json"),
        ])
        .into())
    }
}
