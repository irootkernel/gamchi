//! Subset-derived wire names (methods, policies, statuses) and gamchi emit
//! policy for ThreadItems. The subset is a Dolgorae client requirement, not
//! this module's item allowlist. There is no ad hoc job JSON.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// SHA-256 of docs/protocol/references/dolgorae-codex-0.149.0-required-subset.json.
pub const SUBSET_SHA256: &str = "7d6b33228266826eb5077192867f31f0caf2df1875525c194ee23f179050d409";

/// Repository-relative subset artifact.
pub const SUBSET_REL_PATH: &str =
    "docs/protocol/references/dolgorae-codex-0.149.0-required-subset.json";

/// Transport bounds from Dolgorae src/app_server.rs / CCAS REQ-TRANSPORT-004.
pub const MAX_HTTP_UPGRADE_BYTES: usize = 16 * 1024;
pub const MAX_WEBSOCKET_FRAME_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_WEBSOCKET_MESSAGE_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_SOLICITED_ENVELOPE_BYTES: usize = 64 * 1024;
pub const MAX_CORRELATED_MESSAGES: usize = 4096;
pub const EARLY_TOP_LEVEL_ID_PREFIX_BYTES: usize = 64 * 1024;

/// Client methods Dolgorae sends after initialize/initialized.
pub const METHOD_INITIALIZE: &str = "initialize";
pub const METHOD_ACCOUNT_READ: &str = "account/read";
pub const METHOD_MODEL_LIST: &str = "model/list";
pub const METHOD_THREAD_START: &str = "thread/start";
pub const METHOD_THREAD_RESUME: &str = "thread/resume";
pub const METHOD_THREAD_READ: &str = "thread/read";
pub const METHOD_THREAD_FORK: &str = "thread/fork";
pub const METHOD_TURN_START: &str = "turn/start";
pub const METHOD_TURN_INTERRUPT: &str = "turn/interrupt";
pub const NOTIFICATION_INITIALIZED: &str = "initialized";

/// Server notifications Dolgorae must not have opted out of.
pub const NOTIFY_ITEM_STARTED: &str = "item/started";
pub const NOTIFY_ITEM_COMPLETED: &str = "item/completed";
pub const NOTIFY_ITEM_FILE_CHANGE_PATCH_UPDATED: &str = "item/fileChange/patchUpdated";
pub const NOTIFY_TURN_COMPLETED: &str = "turn/completed";
pub const NOTIFY_THREAD_STARTED: &str = "thread/started";

/// Server requests. Support follows the subset classification.
pub const REQUEST_COMMAND_APPROVAL: &str = "item/commandExecution/requestApproval";
pub const REQUEST_FILE_CHANGE_APPROVAL: &str = "item/fileChange/requestApproval";
pub const REQUEST_USER_INPUT: &str = "item/tool/requestUserInput";
/// recognized-unsupported
pub const REQUEST_PERMISSIONS_APPROVAL: &str = "item/permissions/requestApproval";
/// recognized-unsupported
pub const REQUEST_MCP_ELICITATION: &str = "mcpServer/elicitation/request";

/// Closed method list from the subset.
pub const CLIENT_METHODS: &[&str] = &[
    METHOD_INITIALIZE,
    METHOD_ACCOUNT_READ,
    METHOD_MODEL_LIST,
    METHOD_THREAD_START,
    METHOD_THREAD_RESUME,
    METHOD_THREAD_READ,
    METHOD_THREAD_FORK,
    METHOD_TURN_START,
    METHOD_TURN_INTERRUPT,
];

/// Subset closed thread/turn approval policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalPolicy {
    Untrusted,
    OnRequest,
    Never,
}

impl ApprovalPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Untrusted => "untrusted",
            Self::OnRequest => "on-request",
            Self::Never => "never",
        }
    }
}

impl Serialize for ApprovalPolicy {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ApprovalPolicy {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        parse_approval_policy(&s).map_err(serde::de::Error::custom)
    }
}

/// Subset thread sandbox string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadSandbox {
    ReadOnly,
    WorkspaceWrite,
}

impl ThreadSandbox {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "read-only",
            Self::WorkspaceWrite => "workspace-write",
        }
    }
}

impl Serialize for ThreadSandbox {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ThreadSandbox {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        parse_thread_sandbox(&s).map_err(serde::de::Error::custom)
    }
}

/// MCP spawn defaults (write-capable subagent). App-server omitted fields
/// follow the Dolgorae/CCAS wire, not these.
pub const DEFAULT_APPROVAL_POLICY: ApprovalPolicy = ApprovalPolicy::Never;
pub const DEFAULT_THREAD_SANDBOX: ThreadSandbox = ThreadSandbox::WorkspaceWrite;

/// App-server omitted-field defaults (Dolgorae/CCAS wire).
pub const APP_SERVER_DEFAULT_APPROVAL_POLICY: ApprovalPolicy = ApprovalPolicy::Untrusted;
pub const APP_SERVER_DEFAULT_THREAD_SANDBOX: ThreadSandbox = ThreadSandbox::ReadOnly;

/// Subset turn sandboxPolicy type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnSandboxType {
    ReadOnly,
    WorkspaceWrite,
}

impl TurnSandboxType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "readOnly",
            Self::WorkspaceWrite => "workspaceWrite",
        }
    }

    pub fn to_thread_sandbox(self) -> ThreadSandbox {
        match self {
            Self::ReadOnly => ThreadSandbox::ReadOnly,
            Self::WorkspaceWrite => ThreadSandbox::WorkspaceWrite,
        }
    }
}

/// Subset turn/completed status vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnStatus {
    InProgress,
    Completed,
    Interrupted,
    Failed,
}

impl TurnStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InProgress => "inProgress",
            Self::Completed => "completed",
            Self::Interrupted => "interrupted",
            Self::Failed => "failed",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Interrupted | Self::Failed)
    }
}

impl Serialize for TurnStatus {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for TurnStatus {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        parse_turn_status(&s).map_err(serde::de::Error::custom)
    }
}

/// Dead-generation failure reason. Not a TurnStatus.
pub const FAILURE_WORKER_GONE: &str = "worker_gone";

/// Public ThreadItem type gamchi may emit (ACP-projectable).
pub const ITEM_USER_MESSAGE: &str = "userMessage";
pub const ITEM_AGENT_MESSAGE: &str = "agentMessage";
pub const ITEM_PLAN: &str = "plan";
pub const ITEM_COMMAND_EXECUTION: &str = "commandExecution";
pub const ITEM_FILE_CHANGE: &str = "fileChange";
pub const ITEM_WEB_SEARCH: &str = "webSearch";

/// gamchi emit policy, not a subset-derived allowlist.
pub const PUBLIC_ITEM_TYPES: &[&str] = &[
    ITEM_USER_MESSAGE,
    ITEM_AGENT_MESSAGE,
    ITEM_PLAN,
    ITEM_COMMAND_EXECUTION,
    ITEM_FILE_CHANGE,
    ITEM_WEB_SEARCH,
];

/// Subset schema branches gamchi will not emit.
/// Presence in the subset means Dolgorae requires Codex to have the schema,
/// not that gamchi should produce them.
pub const EXCLUDED_FATAL_ITEM_TYPES: &[&str] = &["collabAgentToolCall", "subAgentActivity"];

/// Subset command/file approval response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalDecision {
    Accept,
    AcceptForSession,
    Decline,
    Cancel,
}

impl ApprovalDecision {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accept => "accept",
            Self::AcceptForSession => "acceptForSession",
            Self::Decline => "decline",
            Self::Cancel => "cancel",
        }
    }
}

/// Durable conversation identity plus resolved defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thread {
    pub id: String,
    pub cwd: String,
    pub model: String,
    pub sandbox: ThreadSandbox,
    pub approval_policy: ApprovalPolicy,
    pub developer_instructions: String,
    pub acp_session_id: String,
    /// Empty when the thread has no inProgress turn.
    #[serde(default)]
    pub in_progress_turn_id: String,
}

/// One admitted prompt on a thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Turn {
    pub id: String,
    pub thread_id: String,
    pub generation_id: String,
    pub status: TurnStatus,
    pub input: Vec<UserInput>,
    pub model: String,
    pub effort: String,
    /// Raw ACP session/prompt stopReason.
    pub stop_reason: String,
    /// e.g. worker_gone; empty on completed.
    pub failure_reason: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub client_request_id: String,
    /// Non-empty while `session/request_permission` is parked for grok_respond.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pending_request_id: String,
    /// Empty while waiting; otherwise an ApprovalDecision string.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pending_decision: String,
    #[serde(default)]
    pub items: Vec<Item>,
}

impl Turn {
    /// In-flight permission that is not a TurnStatus.
    pub fn pending_approval(&self) -> bool {
        !self.status.is_terminal()
            && !self.pending_request_id.is_empty()
            && self.pending_decision.is_empty()
    }
}

/// Owner process for an in-flight turn (`pid` + start epoch).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Generation {
    pub id: String,
    #[serde(default)]
    pub turn_id: String,
    pub owner_pid: u32,
    pub started_epoch: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_started_epoch: Option<u64>,
    #[serde(default)]
    pub child_eof: bool,
}

/// One turn/start input element. Text is the v1 worker path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserInput {
    pub kind: String,
    pub text: String,
}

/// One ordered ThreadItem on a turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    pub item_type: String,
    pub text: String,
    pub status: String,
}

/// Rejects unknown values rather than ignoring them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsupportedValueError {
    pub kind: &'static str,
    pub value: String,
}

impl fmt::Display for UnsupportedValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unsupported {} {:?}", self.kind, self.value)
    }
}

impl std::error::Error for UnsupportedValueError {}

/// Rejects unknown values rather than ignoring them.
pub fn parse_approval_policy(v: &str) -> Result<ApprovalPolicy, UnsupportedValueError> {
    match v {
        "untrusted" => Ok(ApprovalPolicy::Untrusted),
        "on-request" => Ok(ApprovalPolicy::OnRequest),
        "never" => Ok(ApprovalPolicy::Never),
        _ => Err(UnsupportedValueError {
            kind: "approvalPolicy",
            value: v.to_string(),
        }),
    }
}

/// Rejects unknown values rather than ignoring them.
pub fn parse_thread_sandbox(v: &str) -> Result<ThreadSandbox, UnsupportedValueError> {
    match v {
        "read-only" => Ok(ThreadSandbox::ReadOnly),
        "workspace-write" => Ok(ThreadSandbox::WorkspaceWrite),
        _ => Err(UnsupportedValueError {
            kind: "sandbox",
            value: v.to_string(),
        }),
    }
}

/// Rejects unknown `sandboxPolicy.type` values rather than ignoring them.
pub fn parse_turn_sandbox_type(v: &str) -> Result<TurnSandboxType, UnsupportedValueError> {
    match v {
        "readOnly" => Ok(TurnSandboxType::ReadOnly),
        "workspaceWrite" => Ok(TurnSandboxType::WorkspaceWrite),
        _ => Err(UnsupportedValueError {
            kind: "sandboxPolicy.type",
            value: v.to_string(),
        }),
    }
}

/// Rejects unknown values rather than ignoring them.
pub fn parse_approval_decision(v: &str) -> Result<ApprovalDecision, UnsupportedValueError> {
    match v {
        "accept" => Ok(ApprovalDecision::Accept),
        "acceptForSession" => Ok(ApprovalDecision::AcceptForSession),
        "decline" => Ok(ApprovalDecision::Decline),
        "cancel" => Ok(ApprovalDecision::Cancel),
        _ => Err(UnsupportedValueError {
            kind: "approvalDecision",
            value: v.to_string(),
        }),
    }
}

/// Rejects unknown values rather than ignoring them.
pub fn parse_turn_status(v: &str) -> Result<TurnStatus, UnsupportedValueError> {
    match v {
        "inProgress" => Ok(TurnStatus::InProgress),
        "completed" => Ok(TurnStatus::Completed),
        "interrupted" => Ok(TurnStatus::Interrupted),
        "failed" => Ok(TurnStatus::Failed),
        _ => Err(UnsupportedValueError {
            kind: "turnStatus",
            value: v.to_string(),
        }),
    }
}

/// Folds an ACP session/prompt stopReason into a subset TurnStatus.
pub fn map_stop_reason(stop_reason: &str) -> TurnStatus {
    match stop_reason {
        "end_turn" => TurnStatus::Completed,
        "cancelled" => TurnStatus::Interrupted,
        _ => TurnStatus::Failed,
    }
}

/// Reports whether `t` is public or excluded-fatal.
pub fn known_item_type(t: &str) -> (bool, bool) {
    if PUBLIC_ITEM_TYPES.contains(&t) {
        return (true, false);
    }
    if EXCLUDED_FATAL_ITEM_TYPES.contains(&t) {
        return (false, true);
    }
    (false, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use sha2::{Digest, Sha256};
    use std::collections::HashMap;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn find_repo_root() -> PathBuf {
        let start = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut dir = start.clone();
        loop {
            if dir.join("docs/roadmap/README.md").is_file() {
                return dir;
            }
            if !dir.pop() {
                panic!("repo root not found from {}", start.display());
            }
        }
    }

    fn sha256_hex(body: &[u8]) -> String {
        let digest = Sha256::digest(body);
        let mut out = String::with_capacity(64);
        for b in digest {
            use std::fmt::Write;
            write!(&mut out, "{b:02x}").unwrap();
        }
        out
    }

    #[derive(Deserialize)]
    struct SubsetFile {
        client_methods: HashMap<String, Vec<String>>,
        server_requests: HashMap<String, serde_json::Value>,
        command_and_file_approval_decisions: HashMap<String, String>,
        required_shapes: RequiredShapes,
        notifications: Notifications,
    }

    #[derive(Deserialize)]
    struct RequiredShapes {
        approval_policy_values: Vec<String>,
        sandbox: SandboxShape,
    }

    #[derive(Deserialize)]
    struct SandboxShape {
        thread_values: Vec<String>,
    }

    #[derive(Deserialize)]
    struct Notifications {
        terminal_turn: TerminalTurn,
    }

    #[derive(Deserialize)]
    struct TerminalTurn {
        terminal_statuses: Vec<String>,
    }

    fn load_subset() -> SubsetFile {
        let body = fs::read_to_string(find_repo_root().join(SUBSET_REL_PATH)).unwrap();
        serde_json::from_str(&body).unwrap()
    }

    #[test]
    fn subset_digest() {
        let body = fs::read(find_repo_root().join(SUBSET_REL_PATH)).unwrap();
        let got = sha256_hex(&body);
        assert_eq!(
            got, SUBSET_SHA256,
            "subset digest {got}, want {SUBSET_SHA256}"
        );
    }

    #[test]
    fn subset_client_methods_match() {
        let subset = load_subset();
        assert_eq!(
            subset.client_methods.len(),
            CLIENT_METHODS.len(),
            "subset has {} client methods, pin has {}",
            subset.client_methods.len(),
            CLIENT_METHODS.len()
        );
        for name in CLIENT_METHODS {
            assert!(
                subset.client_methods.contains_key(*name),
                "pinned method {name:?} missing from subset"
            );
        }
    }

    #[test]
    fn subset_closed_vocabularies() {
        let subset = load_subset();
        for name in [
            REQUEST_COMMAND_APPROVAL,
            REQUEST_FILE_CHANGE_APPROVAL,
            REQUEST_USER_INPUT,
            REQUEST_PERMISSIONS_APPROVAL,
            REQUEST_MCP_ELICITATION,
        ] {
            assert!(
                subset.server_requests.contains_key(name),
                "server request {name:?} missing from subset"
            );
        }
        let want_decisions = [
            ("accept_once", ApprovalDecision::Accept.as_str()),
            (
                "accept_for_generation",
                ApprovalDecision::AcceptForSession.as_str(),
            ),
            ("decline", ApprovalDecision::Decline.as_str()),
            ("cancel", ApprovalDecision::Cancel.as_str()),
        ];
        for (k, want) in want_decisions {
            let got = subset
                .command_and_file_approval_decisions
                .get(k)
                .map(String::as_str)
                .unwrap_or("");
            assert_eq!(got, want, "decision {k}: subset {got:?} want {want:?}");
        }
        let policies: Vec<&str> = subset
            .required_shapes
            .approval_policy_values
            .iter()
            .map(String::as_str)
            .collect();
        for p in [
            ApprovalPolicy::Untrusted,
            ApprovalPolicy::OnRequest,
            ApprovalPolicy::Never,
        ] {
            assert!(
                policies.contains(&p.as_str()),
                "approvalPolicy {:?} missing from subset",
                p.as_str()
            );
        }
        let sand: Vec<&str> = subset
            .required_shapes
            .sandbox
            .thread_values
            .iter()
            .map(String::as_str)
            .collect();
        assert!(
            sand.contains(&ThreadSandbox::ReadOnly.as_str())
                && sand.contains(&ThreadSandbox::WorkspaceWrite.as_str()),
            "sandbox values {sand:?}"
        );
        let term: Vec<&str> = subset
            .notifications
            .terminal_turn
            .terminal_statuses
            .iter()
            .map(String::as_str)
            .collect();
        for s in [
            TurnStatus::Completed,
            TurnStatus::Interrupted,
            TurnStatus::Failed,
        ] {
            assert!(
                term.contains(&s.as_str()),
                "terminal status {:?} missing from subset",
                s.as_str()
            );
        }
        assert!(
            !term.contains(&TurnStatus::InProgress.as_str()),
            "inProgress must not be listed as terminal"
        );
    }

    #[test]
    fn parse_closed_enums() {
        assert!(
            parse_approval_policy("always").is_err(),
            "expected rejection"
        );
        assert!(
            parse_thread_sandbox("danger-full-access").is_err(),
            "expected rejection"
        );
        let p = parse_approval_policy(ApprovalPolicy::Untrusted.as_str()).unwrap();
        assert_eq!(p, ApprovalPolicy::Untrusted);
    }

    #[test]
    fn item_allowlist() {
        let (public, fatal) = known_item_type(ITEM_AGENT_MESSAGE);
        assert!(public && !fatal, "agentMessage must be public");
        let (public, fatal) = known_item_type("subAgentActivity");
        assert!(!public && fatal, "subAgentActivity must be excluded-fatal");
        let (public, fatal) = known_item_type("mystery");
        assert!(
            !public && !fatal,
            "unknown types are neither public nor excluded-fatal"
        );
        let (public, fatal) = known_item_type("imageView");
        assert!(!public && !fatal, "imageView is not a v1 emit type");
    }

    #[test]
    fn map_stop_reason_and_defaults() {
        assert_eq!(map_stop_reason("end_turn"), TurnStatus::Completed);
        assert_eq!(map_stop_reason("cancelled"), TurnStatus::Interrupted);
        assert_eq!(map_stop_reason("refusal"), TurnStatus::Failed);
        assert_eq!(DEFAULT_APPROVAL_POLICY, ApprovalPolicy::Never);
        assert_eq!(DEFAULT_THREAD_SANDBOX, ThreadSandbox::WorkspaceWrite);
        assert_eq!(
            APP_SERVER_DEFAULT_APPROVAL_POLICY,
            ApprovalPolicy::Untrusted
        );
        assert_eq!(APP_SERVER_DEFAULT_THREAD_SANDBOX, ThreadSandbox::ReadOnly);
        assert_eq!(
            parse_turn_sandbox_type("readOnly")
                .unwrap()
                .to_thread_sandbox(),
            ThreadSandbox::ReadOnly
        );
        assert_eq!(
            parse_turn_sandbox_type("workspaceWrite")
                .unwrap()
                .to_thread_sandbox(),
            ThreadSandbox::WorkspaceWrite
        );
        assert!(parse_turn_sandbox_type("read-only").is_err());
        assert!(parse_turn_status("always").is_err(), "expected rejection");
        assert_eq!(parse_turn_status("failed").unwrap(), TurnStatus::Failed);
        assert!(TurnStatus::Failed.is_terminal());
        assert!(!TurnStatus::InProgress.is_terminal());
        assert_eq!(FAILURE_WORKER_GONE, "worker_gone");
    }

    #[test]
    fn no_job_alias() {
        // The worker identity is Thread/Turn/Item. A "Job" type must not appear.
        let job = find_repo_root().join("crates/core/src/job.rs");
        assert!(!Path::new(&job).is_file(), "ad hoc job.rs is forbidden");
    }
}
