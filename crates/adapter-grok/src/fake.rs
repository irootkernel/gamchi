//! Deterministic ACP v1 agent: initialize, session/new, session/prompt, update.

use agent_client_protocol::schema::v1::{
    AgentCapabilities, ContentBlock, ContentChunk, InitializeRequest, InitializeResponse,
    NewSessionRequest, NewSessionResponse, PromptRequest, PromptResponse, SessionId,
    SessionNotification, SessionUpdate, StopReason, TextContent, ToolCall, ToolCallStatus,
    ToolCallUpdate, ToolCallUpdateFields, ToolKind,
};
use agent_client_protocol::{Agent, ConnectTo, Result};

/// Session id returned by every `session/new`.
pub const STUB_SESSION_ID: &str = "stub-session";
/// Text of the canned `agent_message_chunk`.
pub const STUB_REPLY: &str = "stub-reply";
/// Tool call id of the canned edit (no real file write).
pub const STUB_TOOL_CALL_ID: &str = "stub-edit";

/// Serve the fake agent on `transport` until the peer closes.
pub async fn run_fake_agent(transport: impl ConnectTo<Agent>) -> Result<()> {
    Agent
        .builder()
        .name("grokgrok-fake-acp-agent")
        .on_receive_request(
            async move |initialize: InitializeRequest, responder, _connection| {
                responder.respond(
                    InitializeResponse::new(initialize.protocol_version)
                        .agent_capabilities(AgentCapabilities::new()),
                )
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |_req: NewSessionRequest, responder, _connection| {
                responder.respond(NewSessionResponse::new(SessionId::new(STUB_SESSION_ID)))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |req: PromptRequest, responder, connection| {
                emit_stub_updates(&connection, &req.session_id)?;
                responder.respond(PromptResponse::new(StopReason::EndTurn))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_to(transport)
        .await
}

fn emit_stub_updates(
    connection: &agent_client_protocol::ConnectionTo<agent_client_protocol::Client>,
    session_id: &SessionId,
) -> Result<()> {
    connection.send_notification(SessionNotification::new(
        session_id.clone(),
        SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(
            STUB_REPLY,
        )))),
    ))?;
    connection.send_notification(SessionNotification::new(
        session_id.clone(),
        SessionUpdate::ToolCall(
            ToolCall::new(STUB_TOOL_CALL_ID, "stub edit")
                .kind(ToolKind::Edit)
                .status(ToolCallStatus::Pending),
        ),
    ))?;
    connection.send_notification(SessionNotification::new(
        session_id.clone(),
        SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            STUB_TOOL_CALL_ID,
            ToolCallUpdateFields::new().status(ToolCallStatus::Completed),
        )),
    ))?;
    Ok(())
}
