use std::collections::VecDeque;
use std::io;
use std::sync::{Arc, Mutex};

use rmcp::model::{ClientJsonRpcMessage, ServerJsonRpcMessage};
use rmcp::service::RoleServer;
use rmcp::service::ServerInitializeError;
use rmcp::transport::Transport;
use rmcp::ServerHandler;
use serde_json::json;

use super::*;
use crate::server_handler::build_instructions;

#[derive(Clone)]
struct ScriptedTransport {
    incoming: Arc<Mutex<VecDeque<ClientJsonRpcMessage>>>,
    sent: Arc<Mutex<Vec<ServerJsonRpcMessage>>>,
}

impl ScriptedTransport {
    fn from_messages(messages: Vec<ClientJsonRpcMessage>) -> Self {
        Self {
            incoming: Arc::new(Mutex::new(VecDeque::from(messages))),
            sent: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl Transport<RoleServer> for ScriptedTransport {
    type Error = io::Error;

    fn send(
        &mut self,
        item: ServerJsonRpcMessage,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send + 'static {
        let sent = self.sent.clone();
        async move {
            sent.lock().unwrap().push(item);
            Ok(())
        }
    }

    fn receive(
        &mut self,
    ) -> impl std::future::Future<Output = Option<ClientJsonRpcMessage>> + Send {
        let next = self.incoming.lock().unwrap().pop_front();
        std::future::ready(next)
    }

    fn close(&mut self) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send {
        std::future::ready(Ok(()))
    }
}

fn initialize_request_message() -> ClientJsonRpcMessage {
    serde_json::from_value(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "unit-test-client",
                "version": "1.0.0"
            }
        }
    }))
    .unwrap()
}

fn server_discover_request_message() -> ClientJsonRpcMessage {
    serde_json::from_value(json!({
        "jsonrpc": "2.0",
        "id": 0,
        "method": "server/discover",
        "params": {}
    }))
    .unwrap()
}

fn initialized_notification_message() -> ClientJsonRpcMessage {
    serde_json::from_value(json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    }))
    .unwrap()
}

fn get_config_request_message() -> ClientJsonRpcMessage {
    serde_json::from_value(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "get_config",
            "arguments": {}
        }
    }))
    .unwrap()
}

#[test]
fn get_info_returns_server_name() {
    let server = make_server(false);
    assert_eq!(server.get_info().server_info.name, "codehealth-mcp-server");
}

#[test]
fn get_info_standalone_instructions_omit_api_tools() {
    let info = make_server(true).get_info();
    assert!(!info
        .instructions
        .as_deref()
        .unwrap_or("")
        .contains("select_project"));
}

#[test]
fn get_info_api_instructions_include_api_tools() {
    let info = make_server(false).get_info();
    assert!(info
        .instructions
        .as_deref()
        .unwrap_or("")
        .contains("select_project"));
}

#[test]
fn inlined_schema_for_produces_object_with_type() {
    let schema = crate::inlined_schema_for::<crate::tools::FilePathParam>();
    assert!(schema.contains_key("type") || schema.contains_key("properties"));
}

#[test]
fn build_instructions_standalone_omits_api_tools() {
    let text = build_instructions(true, false, false);
    assert!(text.contains("code_health_review"));
    assert!(!text.contains("select_project"));
}

#[test]
fn build_instructions_api_mode_includes_all_tools() {
    let text = build_instructions(false, false, false);
    assert!(text.contains("code_health_review"));
    assert!(text.contains("select_project"));
    assert!(text.contains("code_ownership_for_path"));
}

#[test]
fn build_instructions_tools_filtered_adds_note() {
    let text = build_instructions(false, true, false);
    assert!(text.contains("enabled_tools"));
    assert!(text.contains("restricted"));
}

#[test]
fn build_instructions_tools_not_filtered_no_note() {
    let text = build_instructions(false, false, false);
    assert!(!text.contains("restricted"));
}

#[test]
fn handle_serve_error_connection_closed_initialize_request_is_ok() {
    let result = crate::handle_serve_error(ServerInitializeError::ConnectionClosed(
        "initialize request".to_string(),
    ));
    assert!(result.is_ok());
}

#[test]
fn handle_serve_error_connection_closed_initialize_notification_is_ok() {
    let result = crate::handle_serve_error(ServerInitializeError::ConnectionClosed(
        "initialize notification".to_string(),
    ));
    assert!(result.is_ok());
}

#[test]
fn handle_serve_error_non_connection_closed_returns_err() {
    let err = ServerInitializeError::Cancelled;
    let result = crate::handle_serve_error(err);
    assert!(result.is_err());
}

#[tokio::test]
async fn serve_or_handle_disconnect_returns_none_when_client_closes_early() {
    let server = make_server(false);
    let transport = ScriptedTransport::from_messages(vec![]);

    let result = crate::serve_or_handle_disconnect(server, transport)
        .await
        .unwrap();

    assert!(result.is_none());
}

#[tokio::test]
async fn serve_or_handle_disconnect_returns_service_on_successful_handshake() {
    let server = make_server(false);
    let transport = ScriptedTransport::from_messages(vec![
        initialize_request_message(),
        initialized_notification_message(),
    ]);

    let mut service = crate::serve_or_handle_disconnect(server, transport)
        .await
        .unwrap()
        .expect("expected initialized service");

    let close_result = service.close().await;
    assert!(close_result.is_ok());
}

#[tokio::test]
async fn initialized_client_can_call_tool_through_server_handler() {
    use rmcp::ServiceExt;

    let transport = ScriptedTransport::from_messages(vec![
        initialize_request_message(),
        initialized_notification_message(),
        get_config_request_message(),
    ]);
    let sent = transport.sent.clone();
    let mut server = make_server(false);
    let probe = crate::TrackingProbe::install(&mut server);

    let service = server
        .serve(transport)
        .await
        .expect("MCP handshake should succeed");
    let _ = tokio::time::timeout(std::time::Duration::from_millis(500), service.waiting()).await;

    let responses: Vec<serde_json::Value> = sent
        .lock()
        .unwrap()
        .iter()
        .filter_map(|message| serde_json::to_value(message).ok())
        .collect();
    assert!(responses.iter().any(|response| {
        response.get("id") == Some(&json!(2)) && response.pointer("/result/content").is_some()
    }));
    assert!(probe.calls().iter().any(|call| matches!(
        call,
        crate::RecordedTrackingCall::Event { name, .. } if name == "get-config"
    )));
}

#[tokio::test]
async fn server_discover_falls_back_to_initialize_on_same_connection() {
    let transport = ScriptedTransport::from_messages(vec![
        server_discover_request_message(),
        initialize_request_message(),
        initialized_notification_message(),
    ]);
    let sent = transport.sent.clone();
    let server = make_server(false);

    let mut service = crate::serve_or_handle_disconnect(
        server,
        crate::transport::DiscoveryFallbackTransport::new(transport),
    )
    .await
    .unwrap()
    .expect("expected initialized service");

    let messages: Vec<serde_json::Value> = sent
        .lock()
        .unwrap()
        .iter()
        .map(|message| serde_json::to_value(message).unwrap())
        .collect();
    assert_eq!(
        (
            &messages[0]["id"],
            &messages[0]["error"]["code"],
            messages
                .iter()
                .any(|message| message.get("result").is_some()),
        ),
        (&json!(0), &json!(-32601), true)
    );

    service.close().await.unwrap();
}

fn prompts_list_request_message() -> ClientJsonRpcMessage {
    serde_json::from_value(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "prompts/list",
        "params": {}
    }))
    .unwrap()
}

fn prompts_get_request_message(name: &str) -> ClientJsonRpcMessage {
    serde_json::from_value(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "prompts/get",
        "params": { "name": name }
    }))
    .unwrap()
}

#[tokio::test]
async fn list_prompts_and_get_prompt_handlers_work() {
    use rmcp::ServiceExt;

    let transport = ScriptedTransport::from_messages(vec![
        initialize_request_message(),
        initialized_notification_message(),
        prompts_list_request_message(),
        prompts_get_request_message("review_code_health"),
    ]);
    let sent = transport.sent.clone();
    let server = make_server(false);

    let service = server
        .serve(transport)
        .await
        .expect("MCP handshake should succeed");

    // Drain remaining client requests (prompts/list + prompts/get).
    let _ = tokio::time::timeout(std::time::Duration::from_millis(500), service.waiting()).await;

    let messages = sent.lock().unwrap().clone();
    let bodies: Vec<serde_json::Value> = messages
        .iter()
        .filter_map(|m| serde_json::to_value(m).ok())
        .collect();

    let list_ok = bodies.iter().any(|b| {
        b.get("id") == Some(&json!(2))
            && b.pointer("/result/prompts")
                .and_then(|p| p.as_array())
                .is_some_and(|prompts| {
                    prompts
                        .iter()
                        .any(|p| p.get("name") == Some(&json!("review_code_health")))
                })
    });
    assert!(
        list_ok,
        "prompts/list handler should return review_code_health; got {bodies:?}"
    );

    let get_ok = bodies.iter().any(|b| {
        b.get("id") == Some(&json!(3))
            && b.pointer("/result/messages")
                .and_then(|m| m.as_array())
                .is_some_and(|messages| !messages.is_empty())
    });
    assert!(
        get_ok,
        "prompts/get handler should return messages; got {bodies:?}"
    );
}
