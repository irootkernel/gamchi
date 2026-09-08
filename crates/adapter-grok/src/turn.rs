//! Run one ACP turn: spawn `grok agent stdio` (or a test agent) and publish items.

use crate::launch::{plan_launch, ExtraSpawnFields, LaunchError, LaunchPlan, LaunchRequest};
use crate::map::Mapper;
use agent_client_protocol::schema::v1::{
    ClientCapabilities, ContentBlock, FileSystemCapabilities, InitializeRequest, NewSessionRequest,
    PermissionOptionKind, PromptRequest, ReadTextFileRequest, ReadTextFileResponse,
    RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionNotification, StopReason, TextContent, WriteTextFileRequest,
    WriteTextFileResponse,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::{Agent, ByteStreams, Client, ConnectionTo};
use samchi_core::ledger::{Ledger, LedgerError, NewThread, NewTurn};
use samchi_core::source_wire::{
    map_stop_reason, ApprovalPolicy, ThreadSandbox, Turn, TurnStatus, UserInput,
};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How to launch the ACP child.
#[derive(Debug, Clone)]
pub enum AgentCommand {
    /// Plan argv from spawn fields and run `grok`.
    Grok { program: PathBuf },
    /// Test-only: run this argv in `cwd` (fake agent). Skips live Grok.
    Override { program: PathBuf, args: Vec<String> },
}

/// One adapter turn.
#[derive(Debug, Clone)]
pub struct TurnRequest {
    pub cwd: PathBuf,
    pub prompt: String,
    pub approval: ApprovalPolicy,
    pub sandbox: ThreadSandbox,
    pub extra: ExtraSpawnFields,
    pub model: String,
    pub command: AgentCommand,
}

/// Terminal turn plus git `files_changed`.
#[derive(Debug, Clone)]
pub struct TurnOutcome {
    pub turn: Turn,
    pub files_changed: Vec<String>,
    pub files_changed_complete: bool,
}

/// Adapter failure. Spawn is refused before the child starts when launch fails.
#[derive(Debug)]
pub enum AdapterError {
    Launch(LaunchError),
    Ledger(LedgerError),
    Io(io::Error),
    Acp(String),
    ExcludedFatal(String),
}

impl std::fmt::Display for AdapterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Launch(err) => write!(f, "{err}"),
            Self::Ledger(err) => write!(f, "{err}"),
            Self::Io(err) => write!(f, "{err}"),
            Self::Acp(err) | Self::ExcludedFatal(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for AdapterError {}

impl From<LaunchError> for AdapterError {
    fn from(err: LaunchError) -> Self {
        Self::Launch(err)
    }
}

impl From<LedgerError> for AdapterError {
    fn from(err: LedgerError) -> Self {
        Self::Ledger(err)
    }
}

impl From<io::Error> for AdapterError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

struct Shared {
    mapper: Mapper,
    ledger: Arc<Ledger>,
    turn_id: String,
    fatal: Option<String>,
}

/// Admit a ledger turn, spawn the ACP child, map updates, publish terminal.
pub fn run_turn(ledger: Arc<Ledger>, req: &TurnRequest) -> Result<TurnOutcome, AdapterError> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|err| AdapterError::Acp(err.to_string()))?;
    rt.block_on(run_turn_async(ledger, req))
}

async fn run_turn_async(
    ledger: Arc<Ledger>,
    req: &TurnRequest,
) -> Result<TurnOutcome, AdapterError> {
    let plan = match &req.command {
        AgentCommand::Grok { program } => plan_launch(&LaunchRequest {
            program,
            cwd: &req.cwd,
            approval: req.approval,
            sandbox: req.sandbox,
            extra: req.extra.clone(),
        })?,
        AgentCommand::Override { program, args } => LaunchPlan {
            program: program.clone(),
            args: args.clone(),
        },
    };

    let start_head = git_head(&req.cwd);
    let thread = ledger.create_thread(&NewThread {
        cwd: req.cwd.display().to_string(),
        model: req.model.clone(),
        sandbox: req.sandbox,
        approval_policy: req.approval,
        developer_instructions: String::new(),
        acp_session_id: String::new(),
    })?;
    let turn = ledger.admit_turn(&NewTurn {
        thread_id: thread.id.clone(),
        input: vec![UserInput {
            kind: "text".to_string(),
            text: req.prompt.clone(),
        }],
        model: req.model.clone(),
        effort: String::new(),
        client_request_id: None,
    })?;
    let generation_id = turn.generation_id.clone();
    let turn_id = turn.id.clone();

    let mapper = Mapper::new(&req.prompt);
    for item in mapper.items() {
        ledger.upsert_item(&turn_id, item.clone())?;
    }
    let shared = Arc::new(Mutex::new(Shared {
        mapper,
        ledger: ledger.clone(),
        turn_id: turn_id.clone(),
        fatal: None,
    }));

    let mut std_cmd = std::process::Command::new(&plan.program);
    std_cmd.args(&plan.args);
    std_cmd.current_dir(&req.cwd);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        std_cmd.process_group(0);
    }
    let mut cmd = async_process::Command::from(std_cmd);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(AdapterError::Io)?;
    let child_pid = child.id();
    ledger.set_child_pid(&generation_id, child_pid)?;

    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| AdapterError::Acp("child stdin".to_string()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AdapterError::Acp("child stdout".to_string()))?;
    if let Some(mut stderr) = child.stderr.take() {
        tokio::spawn(async move {
            let mut buf = Vec::new();
            let _ = futures::AsyncReadExt::read_to_end(&mut stderr, &mut buf).await;
        });
    }

    let prompt = req.prompt.clone();
    let cwd = req.cwd.clone();
    let shared_n = shared.clone();
    let shared_p = shared.clone();
    let cwd_w = cwd.clone();
    let cwd_r = cwd.clone();

    let client_result = Client
        .builder()
        .name("samchi-for-grok")
        .on_receive_notification(
            {
                let shared = shared_n;
                async move |notification: SessionNotification, _cx| {
                    apply_update(&shared, &notification);
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_request(
            async move |req: WriteTextFileRequest, responder, _cx| {
                write_text_file(&cwd_w, &req.path, &req.content)
                    .map_err(agent_client_protocol::Error::into_internal_error)?;
                responder.respond(WriteTextFileResponse::new())
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |req: ReadTextFileRequest, responder, _cx| {
                let body = read_text_file(&cwd_r, &req.path).unwrap_or_default();
                responder.respond(ReadTextFileResponse::new(body))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let shared = shared_p;
                async move |req: RequestPermissionRequest, responder, _cx| {
                    responder.respond(permission_response(&req, &shared))
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_with(ByteStreams::new(stdin, stdout), {
            let shared = shared.clone();
            let ledger = ledger.clone();
            let turn_id = turn_id.clone();
            async move |connection: ConnectionTo<Agent>| {
                drive_prompt(&connection, &cwd, &prompt, &shared, &ledger, &turn_id).await
            }
        })
        .await;

    let _ = child.kill();
    let _ = tokio::time::timeout(Duration::from_secs(2), child.status()).await;
    let _ = ledger.mark_child_eof(&generation_id);

    match client_result {
        Ok(stop) => finish_turn(
            &ledger,
            &shared,
            &turn_id,
            stop,
            &req.cwd,
            start_head.as_deref(),
        ),
        Err(err) => {
            let _ = ledger.publish_terminal(&turn_id, TurnStatus::Failed, "", &err.to_string());
            Err(AdapterError::Acp(err.to_string()))
        }
    }
}

async fn drive_prompt(
    connection: &ConnectionTo<Agent>,
    cwd: &Path,
    prompt: &str,
    shared: &Arc<Mutex<Shared>>,
    ledger: &Ledger,
    turn_id: &str,
) -> agent_client_protocol::Result<StopReason> {
    let caps = ClientCapabilities::new().fs(FileSystemCapabilities::new()
        .read_text_file(true)
        .write_text_file(true));
    connection
        .send_request(InitializeRequest::new(ProtocolVersion::V1).client_capabilities(caps))
        .block_task()
        .await?;
    let session = connection
        .send_request(NewSessionRequest::new(cwd.to_path_buf()))
        .block_task()
        .await?;
    let prompt_result = connection
        .send_request(PromptRequest::new(
            session.session_id,
            vec![ContentBlock::Text(TextContent::new(prompt))],
        ))
        .block_task()
        .await?;
    if let Some(item) = shared.lock().expect("mapper").mapper.finish_agent_message() {
        let _ = ledger.upsert_item(turn_id, item);
    }
    Ok(prompt_result.stop_reason)
}

fn apply_update(shared: &Arc<Mutex<Shared>>, notification: &SessionNotification) {
    let mut inner = shared.lock().expect("mapper");
    match inner.mapper.apply(&notification.update) {
        Ok(items) => {
            for item in items {
                let _ = inner.ledger.upsert_item(&inner.turn_id, item);
            }
        }
        Err(fatal) => {
            inner.fatal = Some(fatal.item_type);
        }
    }
}

fn permission_response(
    req: &RequestPermissionRequest,
    shared: &Arc<Mutex<Shared>>,
) -> RequestPermissionResponse {
    let allow = req.options.iter().find(|o| {
        matches!(
            o.kind,
            PermissionOptionKind::AllowAlways | PermissionOptionKind::AllowOnce
        )
    });
    match allow {
        Some(opt) => RequestPermissionResponse::new(RequestPermissionOutcome::Selected(
            SelectedPermissionOutcome::new(opt.option_id.clone()),
        )),
        None => {
            shared.lock().expect("mapper").fatal =
                Some("session/request_permission without allow option".to_string());
            RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled)
        }
    }
}

fn write_text_file(cwd: &Path, path: &Path, content: &str) -> io::Result<()> {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };
    if let Some(parent) = abs.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(abs, content)
}

fn read_text_file(cwd: &Path, path: &Path) -> io::Result<String> {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };
    fs::read_to_string(abs)
}

fn finish_turn(
    ledger: &Ledger,
    shared: &Arc<Mutex<Shared>>,
    turn_id: &str,
    stop: StopReason,
    cwd: &Path,
    start_head: Option<&str>,
) -> Result<TurnOutcome, AdapterError> {
    let fatal = shared.lock().expect("mapper").fatal.clone();
    if let Some(fatal) = fatal {
        let turn = ledger.publish_terminal(turn_id, TurnStatus::Failed, "", &fatal)?;
        return Err(AdapterError::ExcludedFatal(format!(
            "turn {} failed: {fatal}",
            turn.id
        )));
    }
    let stop_s = stop_reason_str(stop);
    let status = map_stop_reason(stop_s);
    let failure = if status == TurnStatus::Failed {
        stop_s
    } else {
        ""
    };
    let turn = ledger.publish_terminal(turn_id, status, stop_s, failure)?;
    let (files_changed, files_changed_complete) = git_files_changed(cwd, start_head);
    Ok(TurnOutcome {
        turn,
        files_changed,
        files_changed_complete,
    })
}

fn stop_reason_str(stop: StopReason) -> &'static str {
    match stop {
        StopReason::EndTurn => "end_turn",
        StopReason::Cancelled => "cancelled",
        StopReason::Refusal => "refusal",
        StopReason::MaxTokens => "max_tokens",
        StopReason::MaxTurnRequests => "max_turn_requests",
        _ => "unknown",
    }
}

fn git_head(cwd: &Path) -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["-C", cwd.to_str()?, "rev-parse", "HEAD"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?;
    let s = s.trim();
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

fn git_files_changed(cwd: &Path, start_head: Option<&str>) -> (Vec<String>, bool) {
    let Some(head) = start_head else {
        return (Vec::new(), false);
    };
    let Some(cwd_s) = cwd.to_str() else {
        return (Vec::new(), false);
    };
    let out = std::process::Command::new("git")
        .args(["-C", cwd_s, "diff", "--name-only", head])
        .output();
    match out {
        Ok(out) if out.status.success() => {
            let names = String::from_utf8_lossy(&out.stdout)
                .lines()
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect();
            (names, true)
        }
        _ => (Vec::new(), false),
    }
}
