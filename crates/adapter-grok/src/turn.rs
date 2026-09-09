//! Run one ACP turn: spawn `grok agent stdio` (or a test agent) and publish items.

use crate::launch::{plan_launch, ExtraSpawnFields, LaunchError, LaunchPlan, LaunchRequest};
use crate::map::Mapper;
use crate::teardown::teardown_process_group;
use agent_client_protocol::schema::v1::{
    ClientCapabilities, ContentBlock, FileSystemCapabilities, InitializeRequest,
    LoadSessionRequest, NewSessionRequest, PermissionOptionKind, PromptRequest,
    ReadTextFileRequest, ReadTextFileResponse, RequestPermissionOutcome, RequestPermissionRequest,
    RequestPermissionResponse, SelectedPermissionOutcome, SessionId, SessionNotification,
    StopReason, TextContent, WriteTextFileRequest, WriteTextFileResponse,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::{Agent, ByteStreams, Client, ConnectionTo};
use samchi_core::ledger::{Ledger, LedgerError, NewThread, NewTurn};
use samchi_core::source_wire::{
    map_stop_reason, ApprovalDecision, ApprovalPolicy, ThreadSandbox, Turn, TurnStatus, UserInput,
};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
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
    pub client_request_id: Option<String>,
    /// When set, admit on this thread and `session/load` its stored ACP session.
    pub follow_up_thread_id: Option<String>,
    /// When set, admit on this existing thread with `session/new` (app-server
    /// `turn/start` after `thread/start`). Mutually exclusive with follow-up.
    pub reuse_thread_id: Option<String>,
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
    thread_id: String,
    load_session_id: Option<String>,
    fatal: Option<String>,
    approval: ApprovalPolicy,
    /// `session/load` replay is history, not new-turn items or approvals.
    history: bool,
}

/// Admit a ledger turn, spawn the ACP child, map updates, publish terminal.
pub fn run_turn(ledger: Arc<Ledger>, req: &TurnRequest) -> Result<TurnOutcome, AdapterError> {
    run_turn_on_admit(ledger, req, |_| {})
}

/// Like [`run_turn`], calling `on_admit` after the turn is durable and before the child is driven.
pub fn run_turn_on_admit(
    ledger: Arc<Ledger>,
    req: &TurnRequest,
    on_admit: impl FnOnce(&Turn),
) -> Result<TurnOutcome, AdapterError> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|err| AdapterError::Acp(err.to_string()))?;
    rt.block_on(run_turn_async(ledger, req, on_admit))
}

async fn run_turn_async(
    ledger: Arc<Ledger>,
    req: &TurnRequest,
    on_admit: impl FnOnce(&Turn),
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

    if req.follow_up_thread_id.is_some() && req.reuse_thread_id.is_some() {
        return Err(AdapterError::Acp(
            "follow_up_thread_id and reuse_thread_id are mutually exclusive".to_string(),
        ));
    }

    let start_head = git_head(&req.cwd);
    let (thread, load_session_id) = if let Some(thread_id) = req.follow_up_thread_id.as_deref() {
        let thread = ledger.read_thread(thread_id)?;
        if thread.acp_session_id.is_empty() {
            return Err(AdapterError::Acp(
                "follow-up requires a stored ACP session id".to_string(),
            ));
        }
        let sid = thread.acp_session_id.clone();
        (thread, Some(sid))
    } else if let Some(thread_id) = req.reuse_thread_id.as_deref() {
        (ledger.read_thread(thread_id)?, None)
    } else {
        (
            ledger.create_thread(&NewThread {
                cwd: req.cwd.display().to_string(),
                model: req.model.clone(),
                sandbox: req.sandbox,
                approval_policy: req.approval,
                developer_instructions: String::new(),
                acp_session_id: String::new(),
            })?,
            None,
        )
    };
    let turn = ledger.admit_turn(&NewTurn {
        thread_id: thread.id.clone(),
        input: vec![UserInput {
            kind: "text".to_string(),
            text: req.prompt.clone(),
        }],
        model: req.model.clone(),
        effort: String::new(),
        client_request_id: req.client_request_id.clone(),
    })?;
    on_admit(&turn);
    let generation_id = turn.generation_id.clone();
    let turn_id = turn.id.clone();
    let thread_id = turn.thread_id.clone();
    let fail_admitted = |err: AdapterError| -> AdapterError {
        if let Ok(current) = ledger.read_turn(&turn_id) {
            if !current.status.is_terminal() {
                let _ = ledger.publish_terminal(&turn_id, TurnStatus::Failed, "", &err.to_string());
            }
        }
        err
    };
    if ledger
        .read_turn(&turn_id)
        .map_err(|e| fail_admitted(e.into()))?
        .status
        .is_terminal()
    {
        return Ok(TurnOutcome {
            turn: ledger.read_turn(&turn_id)?,
            files_changed: Vec::new(),
            files_changed_complete: false,
        });
    }

    let mapper = Mapper::new(&req.prompt);
    for item in mapper.items() {
        ledger
            .upsert_item(&turn_id, item.clone())
            .map_err(|e| fail_admitted(e.into()))?;
    }
    let shared = Arc::new(Mutex::new(Shared {
        mapper,
        ledger: ledger.clone(),
        turn_id: turn_id.clone(),
        thread_id: thread_id.clone(),
        load_session_id: load_session_id.clone(),
        fatal: None,
        approval: req.approval,
        history: false,
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
    let mut child = cmd
        .spawn()
        .map_err(AdapterError::Io)
        .map_err(fail_admitted)?;
    let child_pid = child.id();
    ledger
        .set_child_pid(&generation_id, child_pid)
        .map_err(|e| fail_admitted(e.into()))?;
    if ledger.read_turn(&turn_id)?.status.is_terminal() {
        teardown_process_group(child_pid);
        let _ = child.kill();
        return Ok(TurnOutcome {
            turn: ledger.read_turn(&turn_id)?,
            files_changed: Vec::new(),
            files_changed_complete: false,
        });
    }

    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| fail_admitted(AdapterError::Acp("child stdin".to_string())))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| fail_admitted(AdapterError::Acp("child stdout".to_string())))?;
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
            async move |req: ReadTextFileRequest, responder, _cx| match read_text_file(
                &cwd_r, &req.path,
            ) {
                Ok(body) => responder.respond(ReadTextFileResponse::new(body)),
                Err(err) if err.kind() == io::ErrorKind::PermissionDenied => {
                    Err(agent_client_protocol::Error::into_internal_error(err))?
                }
                Err(_) => responder.respond(ReadTextFileResponse::new(String::new())),
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let shared = shared_p;
                async move |req: RequestPermissionRequest, responder, cx| {
                    let (history, approval, ledger, turn_id) = {
                        let inner = shared.lock().expect("mapper");
                        (
                            inner.history,
                            inner.approval,
                            inner.ledger.clone(),
                            inner.turn_id.clone(),
                        )
                    };
                    if history {
                        responder.respond(RequestPermissionResponse::new(
                            RequestPermissionOutcome::Cancelled,
                        ))?;
                        return Ok(());
                    }
                    if !matches!(
                        approval,
                        ApprovalPolicy::Untrusted | ApprovalPolicy::OnRequest
                    ) {
                        responder.respond(auto_allow_permission(&req, &shared))?;
                        return Ok(());
                    }
                    let parked = match ledger.park_approval(&turn_id) {
                        Ok(turn) => turn,
                        Err(_) => {
                            responder.respond(RequestPermissionResponse::new(
                                RequestPermissionOutcome::Cancelled,
                            ))?;
                            return Ok(());
                        }
                    };
                    let request_id = parked.pending_request_id.clone();
                    cx.spawn(async move {
                        responder.respond(finish_gated_permission(
                            &req,
                            &ledger,
                            &turn_id,
                            &request_id,
                        ))?;
                        Ok(())
                    })?;
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_with(ByteStreams::new(stdin, stdout), {
            let shared = shared.clone();
            let ledger = ledger.clone();
            let turn_id = turn_id.clone();
            async move |connection: ConnectionTo<Agent>| {
                let stop = drive_prompt(&connection, &cwd, &prompt, &shared).await?;
                // Concurrent wait/observe treats a dead child as worker_gone.
                // Publish while the ACP connection (and child) is still open.
                let _ = publish_stop(&ledger, &shared, &turn_id, stop);
                Ok(stop)
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
) -> agent_client_protocol::Result<StopReason> {
    let (ledger, turn_id, thread_id, load_session_id) = {
        let inner = shared.lock().expect("mapper");
        (
            inner.ledger.clone(),
            inner.turn_id.clone(),
            inner.thread_id.clone(),
            inner.load_session_id.clone(),
        )
    };
    let caps = ClientCapabilities::new().fs(FileSystemCapabilities::new()
        .read_text_file(true)
        .write_text_file(true));
    let init = connection
        .send_request(InitializeRequest::new(ProtocolVersion::V1).client_capabilities(caps))
        .block_task()
        .await?;
    let session_id = if let Some(sid) = load_session_id.as_deref() {
        if !init.agent_capabilities.load_session {
            let _ = ledger.publish_terminal(
                &turn_id,
                TurnStatus::Failed,
                "",
                "loadSession not advertised",
            );
            return Err(agent_client_protocol::Error::into_internal_error(
                io::Error::other("loadSession not advertised"),
            ));
        }
        {
            let mut inner = shared.lock().expect("mapper");
            inner.history = true;
        }
        let loaded = connection
            .send_request(LoadSessionRequest::new(
                SessionId::new(sid),
                cwd.to_path_buf(),
            ))
            .block_task()
            .await;
        {
            let mut inner = shared.lock().expect("mapper");
            inner.history = false;
        }
        if let Err(err) = loaded {
            let _ =
                ledger.publish_terminal(&turn_id, TurnStatus::Failed, "", "session/load failed");
            return Err(err);
        }
        SessionId::new(sid)
    } else {
        let session = connection
            .send_request(NewSessionRequest::new(cwd.to_path_buf()))
            .block_task()
            .await?;
        let _ = ledger.set_acp_session_id(&thread_id, &session.session_id.to_string());
        session.session_id
    };
    let prompt_result = connection
        .send_request(PromptRequest::new(
            session_id,
            vec![ContentBlock::Text(TextContent::new(prompt))],
        ))
        .block_task()
        .await?;
    if let Some(item) = shared.lock().expect("mapper").mapper.finish_agent_message() {
        let _ = ledger.upsert_item(&turn_id, item);
    }
    Ok(prompt_result.stop_reason)
}

fn apply_update(shared: &Arc<Mutex<Shared>>, notification: &SessionNotification) {
    let mut inner = shared.lock().expect("mapper");
    if inner.history {
        return;
    }
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

fn finish_gated_permission(
    req: &RequestPermissionRequest,
    ledger: &Ledger,
    turn_id: &str,
    request_id: &str,
) -> RequestPermissionResponse {
    let decision = match ledger.wait_pending_decision(turn_id, request_id) {
        Ok(Some(decision)) => decision,
        Ok(None) | Err(_) => {
            let _ = ledger.clear_pending(turn_id, request_id);
            return RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled);
        }
    };
    let _ = ledger.clear_pending(turn_id, request_id);
    match decision_option(req, decision) {
        Some(option_id) => RequestPermissionResponse::new(RequestPermissionOutcome::Selected(
            SelectedPermissionOutcome::new(option_id),
        )),
        None => RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled),
    }
}

fn auto_allow_permission(
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

fn decision_option(
    req: &RequestPermissionRequest,
    decision: ApprovalDecision,
) -> Option<agent_client_protocol::schema::v1::PermissionOptionId> {
    let preferred = match decision {
        ApprovalDecision::Accept => &[
            PermissionOptionKind::AllowOnce,
            PermissionOptionKind::AllowAlways,
        ][..],
        ApprovalDecision::AcceptForSession => &[
            PermissionOptionKind::AllowAlways,
            PermissionOptionKind::AllowOnce,
        ][..],
        ApprovalDecision::Decline => &[
            PermissionOptionKind::RejectOnce,
            PermissionOptionKind::RejectAlways,
        ][..],
        ApprovalDecision::Cancel => return None,
    };
    preferred.iter().find_map(|want| {
        req.options
            .iter()
            .find(|o| o.kind == *want)
            .map(|o| o.option_id.clone())
    })
}

fn confined(cwd: &Path, path: &Path) -> io::Result<PathBuf> {
    let root = cwd.canonicalize()?;
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let mut out = PathBuf::new();
    for c in joined.components() {
        match c {
            Component::Prefix(p) => out.push(p.as_os_str()),
            Component::RootDir => out.push(c),
            Component::CurDir => {}
            Component::ParentDir => {
                let _ = out.pop();
            }
            Component::Normal(s) => out.push(s),
        }
    }
    if !out.starts_with(&root) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "path outside cwd",
        ));
    }
    Ok(out)
}

fn write_text_file(cwd: &Path, path: &Path, content: &str) -> io::Result<()> {
    let abs = confined(cwd, path)?;
    if let Some(parent) = abs.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(abs, content)
}

fn read_text_file(cwd: &Path, path: &Path) -> io::Result<String> {
    fs::read_to_string(confined(cwd, path)?)
}

fn publish_stop(
    ledger: &Ledger,
    shared: &Arc<Mutex<Shared>>,
    turn_id: &str,
    stop: StopReason,
) -> Result<Turn, AdapterError> {
    let fatal = shared.lock().expect("mapper").fatal.clone();
    if let Some(fatal) = fatal {
        return Ok(ledger.publish_terminal(turn_id, TurnStatus::Failed, "", &fatal)?);
    }
    let stop_s = stop_reason_str(stop);
    let status = map_stop_reason(stop_s);
    let failure = if status == TurnStatus::Failed {
        stop_s
    } else {
        ""
    };
    Ok(ledger.publish_terminal(turn_id, status, stop_s, failure)?)
}

fn finish_turn(
    ledger: &Ledger,
    shared: &Arc<Mutex<Shared>>,
    turn_id: &str,
    stop: StopReason,
    cwd: &Path,
    start_head: Option<&str>,
) -> Result<TurnOutcome, AdapterError> {
    let turn = match ledger.read_turn(turn_id) {
        Ok(turn) if turn.status.is_terminal() => turn,
        _ => publish_stop(ledger, shared, turn_id, stop)?,
    };
    let fatal = shared.lock().expect("mapper").fatal.clone();
    if let Some(fatal) = fatal {
        return Err(AdapterError::ExcludedFatal(format!(
            "turn {} failed: {fatal}",
            turn.id
        )));
    }
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

#[cfg(test)]
mod confined_tests {
    use super::*;
    use std::fs;

    #[test]
    fn relative_and_in_cwd_absolute_ok() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "ok").unwrap();
        let got = confined(dir.path(), Path::new("a.txt")).unwrap();
        assert_eq!(got, dir.path().canonicalize().unwrap().join("a.txt"));
        let abs = dir.path().canonicalize().unwrap().join("b.txt");
        let got = confined(dir.path(), &abs).unwrap();
        assert_eq!(got, abs);
    }

    #[test]
    fn parent_escape_and_foreign_absolute_denied() {
        let dir = tempfile::tempdir().unwrap();
        let err = confined(dir.path(), Path::new("../secret")).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
        let err = confined(dir.path(), Path::new("/etc/passwd")).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
    }
}
