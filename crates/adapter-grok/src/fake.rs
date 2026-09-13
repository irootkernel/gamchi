//! Deterministic ACP v1 agent: initialize, session/new, session/prompt, update.

use agent_client_protocol::schema::v1::{
    AgentCapabilities, ContentBlock, ContentChunk, InitializeRequest, InitializeResponse,
    LoadSessionRequest, LoadSessionResponse, NewSessionRequest, NewSessionResponse,
    PermissionOption, PermissionOptionKind, PromptRequest, PromptResponse,
    RequestPermissionOutcome, RequestPermissionRequest, SessionId, SessionNotification,
    SessionUpdate, StopReason, TextContent, ToolCall, ToolCallStatus, ToolCallUpdate,
    ToolCallUpdateFields, ToolKind,
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
        .name("gamchi-fake-acp-agent")
        .on_receive_request(
            async move |initialize: InitializeRequest, responder, _connection| {
                let load = std::env::var_os("GAMCHI_FAKE_NO_LOAD").is_none();
                responder.respond(
                    InitializeResponse::new(initialize.protocol_version)
                        .agent_capabilities(AgentCapabilities::new().load_session(load)),
                )
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |req: NewSessionRequest, responder, _connection| {
                let rules = req
                    .meta
                    .as_ref()
                    .and_then(|meta| meta.get("rules"))
                    .cloned();
                let line = serde_json::json!({"rules": rules});
                if let Ok(mut file) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(".gamchi-fake-session-new.jsonl")
                {
                    use std::io::Write as _;
                    let _ = writeln!(file, "{line}");
                }
                responder.respond(NewSessionResponse::new(SessionId::new(STUB_SESSION_ID)))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |req: LoadSessionRequest, responder, connection| {
                if std::env::var_os("GAMCHI_FAKE_LOAD_ERROR").is_some() {
                    return Err(agent_client_protocol::Error::into_internal_error(
                        std::io::Error::other("session/load failed"),
                    ));
                }
                emit_stub_updates(&connection, &req.session_id)?;
                responder.respond(LoadSessionResponse::new())
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |req: PromptRequest, responder, connection| {
                if let Ok(secs) = std::env::var("GAMCHI_FAKE_HANG_SECS") {
                    let secs: u64 = secs.parse().unwrap_or(60);
                    tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
                }
                if std::env::var_os("GAMCHI_FAKE_ASK_PERMISSION").is_none() {
                    emit_stub_updates(&connection, &req.session_id)?;
                    return responder.respond(PromptResponse::new(StopReason::EndTurn));
                }
                let spawned = connection.clone();
                connection
                    .spawn(async move { prompt_with_permission(req, responder, spawned).await })?;
                Ok(())
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_to(transport)
        .await
}

async fn prompt_with_permission(
    req: PromptRequest,
    responder: agent_client_protocol::Responder<PromptResponse>,
    connection: agent_client_protocol::ConnectionTo<agent_client_protocol::Client>,
) -> Result<()> {
    let options = vec![
        PermissionOption::new("allow-once", "Allow once", PermissionOptionKind::AllowOnce),
        PermissionOption::new(
            "allow-always",
            "Allow always",
            PermissionOptionKind::AllowAlways,
        ),
        PermissionOption::new(
            "reject-once",
            "Reject once",
            PermissionOptionKind::RejectOnce,
        ),
    ];
    let tool = ToolCallUpdate::new(
        "perm-shell",
        ToolCallUpdateFields::new().kind(ToolKind::Execute),
    );
    let resp = connection
        .send_request(RequestPermissionRequest::new(
            req.session_id.clone(),
            tool,
            options,
        ))
        .block_task()
        .await?;
    let allow = matches!(
        resp.outcome,
        RequestPermissionOutcome::Selected(ref sel)
            if sel.option_id.to_string().contains("allow")
    );
    if !allow {
        responder.respond(PromptResponse::new(StopReason::Cancelled))?;
        return Ok(());
    }
    emit_stub_updates(&connection, &req.session_id)?;
    responder.respond(PromptResponse::new(StopReason::EndTurn))?;
    Ok(())
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
