//! Drive initialize → session/new → session/prompt over ACP stdio.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use agent_client_protocol::schema::v1::{
    ContentBlock, ContentChunk, InitializeRequest, NewSessionRequest, PromptRequest,
    SessionNotification, SessionUpdate, StopReason, TextContent, ToolCallStatus, ToolKind,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::{AcpAgent, Agent, Channel, Client, ConnectionTo};
use grokgrok_adapter_grok::{run_fake_agent, STUB_REPLY, STUB_SESSION_ID, STUB_TOOL_CALL_ID};

struct Collected {
    message: String,
    edit_tool: bool,
    edit_completed: bool,
}

impl Collected {
    fn from_updates(updates: &[SessionUpdate]) -> Self {
        let mut out = Self {
            message: String::new(),
            edit_tool: false,
            edit_completed: false,
        };
        for update in updates {
            match update {
                SessionUpdate::AgentMessageChunk(ContentChunk {
                    content: ContentBlock::Text(text),
                    ..
                }) => out.message.push_str(&text.text),
                SessionUpdate::ToolCall(call)
                    if call.tool_call_id.to_string() == STUB_TOOL_CALL_ID
                        && call.kind == ToolKind::Edit =>
                {
                    out.edit_tool = true;
                }
                SessionUpdate::ToolCallUpdate(update)
                    if update.tool_call_id.to_string() == STUB_TOOL_CALL_ID
                        && update.fields.status == Some(ToolCallStatus::Completed) =>
                {
                    out.edit_completed = true;
                }
                _ => {}
            }
        }
        out
    }
}

async fn run_client(
    transport: impl agent_client_protocol::ConnectTo<Client>,
) -> agent_client_protocol::Result<Collected> {
    let updates = Arc::new(Mutex::new(Vec::new()));
    let for_handler = updates.clone();

    Client
        .builder()
        .name("grokgrok-acp-harness")
        .on_receive_notification(
            async move |notification: SessionNotification, _cx| {
                assert_eq!(notification.session_id.to_string(), STUB_SESSION_ID);
                for_handler
                    .lock()
                    .expect("updates")
                    .push(notification.update);
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .connect_with(transport, |connection: ConnectionTo<Agent>| {
            let updates = updates.clone();
            async move {
                connection
                    .send_request(InitializeRequest::new(ProtocolVersion::V1))
                    .block_task()
                    .await?;
                let new_session = connection
                    .send_request(NewSessionRequest::new(PathBuf::from("/")))
                    .block_task()
                    .await?;
                assert_eq!(new_session.session_id.to_string(), STUB_SESSION_ID);

                let prompt = connection
                    .send_request(PromptRequest::new(
                        new_session.session_id,
                        vec![ContentBlock::Text(TextContent::new("ping"))],
                    ))
                    .block_task()
                    .await?;
                assert_eq!(prompt.stop_reason, StopReason::EndTurn);

                let collected = Collected::from_updates(&updates.lock().expect("updates"));
                Ok(collected)
            }
        })
        .await
}

fn assert_stub_turn(collected: &Collected) {
    assert_eq!(collected.message, STUB_REPLY);
    assert!(collected.edit_tool, "missing canned tool_call kind=edit");
    assert!(
        collected.edit_completed,
        "missing canned tool_call_update completed"
    );
}

#[tokio::test]
async fn in_process_session_new_prompt_update() {
    let (agent_side, client_side) = Channel::duplex();
    let agent = tokio::spawn(async move { run_fake_agent(agent_side).await });
    let collected = run_client(client_side)
        .await
        .unwrap_or_else(|err| panic!("client: {err:?}"));
    assert_stub_turn(&collected);
    agent.abort();
}

#[tokio::test]
async fn stdio_session_new_prompt_update() {
    let agent = AcpAgent::from_args([env!("CARGO_BIN_EXE_fake-acp-agent")])
        .unwrap_or_else(|err| panic!("spawn fake-acp-agent: {err:?}"));
    let collected = run_client(agent)
        .await
        .unwrap_or_else(|err| panic!("client: {err:?}"));
    assert_stub_turn(&collected);
}
