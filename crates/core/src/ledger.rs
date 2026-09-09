//! Durable thread/turn/item ledger and generation liveness.

use crate::source_wire::{
    parse_approval_decision, ApprovalDecision, ApprovalPolicy, Generation, Item, Thread,
    ThreadSandbox, Turn, TurnStatus, UserInput, FAILURE_WORKER_GONE,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const THREADS_DIR: &str = "threads";
const TURNS_DIR: &str = "turns";
const GENERATIONS_DIR: &str = "generations";
const BY_CLIENT_DIR: &str = "by-client";
const ADMIT_LOCK: &str = "admit.lock";

static ID_SEQ: AtomicU64 = AtomicU64::new(1);

/// Disk-backed worker ledger.
pub struct Ledger {
    home: PathBuf,
    inner: Mutex<Inner>,
}

struct Inner {
    waiters: HashMap<String, Arc<(Mutex<()>, Condvar)>>,
    live_files: HashMap<String, LiveHold>,
}

static PROCESS_LIVE: LazyLock<Mutex<HashSet<PathBuf>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

struct LiveHold {
    path: PathBuf,
    _lock: Option<FileLock>,
}

impl Drop for LiveHold {
    fn drop(&mut self) {
        PROCESS_LIVE
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.path);
    }
}

/// Fields for a new durable thread.
pub struct NewThread {
    pub cwd: String,
    pub model: String,
    pub sandbox: ThreadSandbox,
    pub approval_policy: ApprovalPolicy,
    pub developer_instructions: String,
    pub acp_session_id: String,
}

/// Fields for a new admitted turn.
pub struct NewTurn {
    pub thread_id: String,
    pub input: Vec<UserInput>,
    pub model: String,
    pub effort: String,
    pub client_request_id: Option<String>,
}

/// Ledger failure. Fail closed; do not invent wire statuses.
#[derive(Debug)]
pub enum LedgerError {
    Io(io::Error),
    Json(serde_json::Error),
    NotFound { kind: &'static str, id: String },
    TurnInProgress { thread_id: String, turn_id: String },
    AlreadyTerminal { turn_id: String, status: TurnStatus },
    InvalidHome(String),
    InvalidId(String),
    InvalidStatus(String),
}

impl std::fmt::Display for LedgerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::Json(err) => write!(f, "{err}"),
            Self::NotFound { kind, id } => write!(f, "{kind} {id} not found"),
            Self::TurnInProgress { thread_id, turn_id } => {
                write!(
                    f,
                    "thread {thread_id} already has inProgress turn {turn_id}"
                )
            }
            Self::AlreadyTerminal { turn_id, status } => {
                write!(f, "turn {turn_id} already terminal ({})", status.as_str())
            }
            Self::InvalidHome(reason) | Self::InvalidId(reason) | Self::InvalidStatus(reason) => {
                write!(f, "{reason}")
            }
        }
    }
}

impl std::error::Error for LedgerError {}

impl From<io::Error> for LedgerError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<serde_json::Error> for LedgerError {
    fn from(err: serde_json::Error) -> Self {
        Self::Json(err)
    }
}

struct FileLock {
    file: File,
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = unsafe { libc::flock(fd(&self.file), libc::LOCK_UN) };
    }
}

fn fd(file: &File) -> libc::c_int {
    use std::os::unix::io::AsRawFd;
    file.as_raw_fd()
}

fn flock_ex(file: &File, nonblock: bool) -> io::Result<()> {
    let op = if nonblock {
        libc::LOCK_EX | libc::LOCK_NB
    } else {
        libc::LOCK_EX
    };
    loop {
        let rc = unsafe { libc::flock(fd(file), op) };
        if rc == 0 {
            return Ok(());
        }
        let err = io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::EINTR) {
            continue;
        }
        return Err(err);
    }
}

fn lock_exclusive(path: &Path) -> Result<FileLock, LedgerError> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)?;
    flock_ex(&file, false)?;
    Ok(FileLock { file })
}

fn try_lock_exclusive_nb(path: &Path) -> io::Result<bool> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)?;
    match flock_ex(&file, true) {
        Ok(()) => {
            let _lock = FileLock { file };
            Ok(false)
        }
        Err(err) => {
            let blocked = err.raw_os_error() == Some(libc::EWOULDBLOCK)
                || err.raw_os_error() == Some(libc::EAGAIN);
            if blocked {
                Ok(true)
            } else {
                Err(err)
            }
        }
    }
}

fn process_holds_live(path: &Path) -> bool {
    PROCESS_LIVE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains(path)
}

impl Ledger {
    /// Create `threads/`, `turns/`, and `generations/` under an absolute home.
    pub fn open(home: impl Into<PathBuf>) -> Result<Self, LedgerError> {
        let home = home.into();
        if !home.is_absolute() {
            return Err(LedgerError::InvalidHome(
                "ledger home must be an absolute path".to_string(),
            ));
        }
        fs::create_dir_all(home.join(THREADS_DIR))?;
        fs::create_dir_all(home.join(TURNS_DIR))?;
        fs::create_dir_all(home.join(GENERATIONS_DIR))?;
        Ok(Self {
            home,
            inner: Mutex::new(Inner {
                waiters: HashMap::new(),
                live_files: HashMap::new(),
            }),
        })
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn create_thread(&self, spec: &NewThread) -> Result<Thread, LedgerError> {
        let thread = Thread {
            id: new_id("th"),
            cwd: spec.cwd.clone(),
            model: spec.model.clone(),
            sandbox: spec.sandbox,
            approval_policy: spec.approval_policy,
            developer_instructions: spec.developer_instructions.clone(),
            acp_session_id: spec.acp_session_id.clone(),
            in_progress_turn_id: String::new(),
        };
        write_json_atomic(&self.thread_path(&thread.id), &thread)?;
        Ok(thread)
    }

    pub fn admit_turn(&self, spec: &NewTurn) -> Result<Turn, LedgerError> {
        validate_id(&spec.thread_id)?;
        let _admit = lock_exclusive(&self.home.join(ADMIT_LOCK))?;
        if let Some(key) = spec.client_request_id.as_deref() {
            if !key.is_empty() {
                if let Some(existing) = self.in_flight_by_client(key)? {
                    return Ok(existing);
                }
            }
        }
        let _tlock = lock_exclusive(&self.thread_lock_path(&spec.thread_id))?;
        let mut thread = self.read_thread(&spec.thread_id)?;
        if let Some(current) = self.live_turn_on_thread_locked(&thread)? {
            return Err(LedgerError::TurnInProgress {
                thread_id: thread.id.clone(),
                turn_id: current.id,
            });
        }
        thread.in_progress_turn_id.clear();
        let generation_id = new_id("g");
        let turn_id = new_id("tu");
        let generation = Generation {
            id: generation_id.clone(),
            turn_id: turn_id.clone(),
            owner_pid: std::process::id(),
            started_epoch: unix_epoch_secs(),
            child_pid: None,
            child_started_epoch: None,
            child_eof: false,
        };
        let turn = Turn {
            id: turn_id.clone(),
            thread_id: thread.id.clone(),
            generation_id: generation_id.clone(),
            status: TurnStatus::InProgress,
            input: spec.input.clone(),
            model: spec.model.clone(),
            effort: spec.effort.clone(),
            stop_reason: String::new(),
            failure_reason: String::new(),
            client_request_id: spec.client_request_id.clone().unwrap_or_default(),
            pending_request_id: String::new(),
            pending_decision: String::new(),
            items: Vec::new(),
        };
        write_json_atomic(&self.generation_path(&generation.id), &generation)?;
        write_json_atomic(&self.turn_path(&turn.id), &turn)?;
        thread.in_progress_turn_id = turn.id.clone();
        // Turn file is the persist point. Later writes must not convert this into Err.
        let _ = write_json_atomic(&self.thread_path(&thread.id), &thread);
        if let Some(key) = spec.client_request_id.as_deref() {
            if !key.is_empty() {
                let _ = self.store_client(key, &turn.id);
            }
        }
        self.hold_live(&generation.id);
        Ok(turn)
    }

    pub fn read_thread(&self, thread_id: &str) -> Result<Thread, LedgerError> {
        validate_id(thread_id)?;
        read_json(&self.thread_path(thread_id))
            .map_err(|err| map_not_found(err, "thread", thread_id))
    }

    pub fn read_turn(&self, turn_id: &str) -> Result<Turn, LedgerError> {
        validate_id(turn_id)?;
        read_json(&self.turn_path(turn_id)).map_err(|err| map_not_found(err, "turn", turn_id))
    }

    pub fn read_generation(&self, generation_id: &str) -> Result<Generation, LedgerError> {
        validate_id(generation_id)?;
        read_json(&self.generation_path(generation_id))
            .map_err(|err| map_not_found(err, "generation", generation_id))
    }

    /// Load a turn, converging a dead generation to `failed`/`worker_gone`.
    pub fn observe(&self, turn_id: &str) -> Result<Turn, LedgerError> {
        validate_id(turn_id)?;
        let turn = self.read_turn(turn_id)?;
        let _lock = lock_exclusive(&self.thread_lock_path(&turn.thread_id))?;
        self.observe_locked(turn_id)
    }

    fn observe_locked(&self, turn_id: &str) -> Result<Turn, LedgerError> {
        let turn = self.read_turn(turn_id)?;
        if turn.status.is_terminal() {
            return Ok(turn);
        }
        let generation = self.read_generation(&turn.generation_id)?;
        if !self.generation_is_dead(&generation) {
            return Ok(turn);
        }
        self.publish_terminal_locked(&turn, TurnStatus::Failed, "", FAILURE_WORKER_GONE)
    }

    /// Persist the ACP session id after `session/new` (TASK-011/012).
    pub fn set_acp_session_id(
        &self,
        thread_id: &str,
        acp_session_id: &str,
    ) -> Result<Thread, LedgerError> {
        validate_id(thread_id)?;
        let _lock = lock_exclusive(&self.thread_lock_path(thread_id))?;
        let mut thread = self.read_thread(thread_id)?;
        thread.acp_session_id = acp_session_id.to_string();
        write_json_atomic(&self.thread_path(&thread.id), &thread)?;
        Ok(thread)
    }

    /// Publish `interrupted` if the turn is still in progress. Converge a dead
    /// generation to `failed`/`worker_gone` first so crash is not recorded as
    /// cancel. An already terminal record is returned unchanged (idempotent;
    /// first terminal wins).
    pub fn cancel(&self, turn_id: &str) -> Result<Turn, LedgerError> {
        validate_id(turn_id)?;
        let turn = self.read_turn(turn_id)?;
        let _lock = lock_exclusive(&self.thread_lock_path(&turn.thread_id))?;
        let turn = self.observe_locked(turn_id)?;
        if turn.status.is_terminal() {
            return Ok(turn);
        }
        self.publish_terminal_locked(&turn, TurnStatus::Interrupted, "cancelled", "")
    }

    /// Whether `generation.child_pid` still names the child from that start epoch.
    pub fn recorded_child_is_alive(generation: &Generation) -> bool {
        match generation.child_pid {
            Some(pid) => process_alive(pid, generation.child_started_epoch),
            None => false,
        }
    }

    /// Park `session/request_permission` and wake awaiters with pending_approval.
    pub fn park_approval(&self, turn_id: &str) -> Result<Turn, LedgerError> {
        validate_id(turn_id)?;
        let turn = self.read_turn(turn_id)?;
        let _lock = lock_exclusive(&self.thread_lock_path(&turn.thread_id))?;
        let mut turn = self.read_turn(turn_id)?;
        if turn.status.is_terminal() {
            return Err(LedgerError::AlreadyTerminal {
                turn_id: turn.id,
                status: turn.status,
            });
        }
        if turn.pending_approval() {
            return Err(LedgerError::InvalidStatus(format!(
                "turn {} already has pending approval {}",
                turn.id, turn.pending_request_id
            )));
        }
        turn.pending_request_id = new_id("ap");
        turn.pending_decision = String::new();
        write_json_atomic(&self.turn_path(&turn.id), &turn)?;
        self.wake(&turn.id);
        Ok(turn)
    }

    /// Record a grok_respond decision for an exact pending request_id.
    pub fn respond(
        &self,
        request_id: &str,
        decision: ApprovalDecision,
    ) -> Result<Turn, LedgerError> {
        validate_id(request_id)?;
        let Some(turn) = self.turn_by_pending_request(request_id)? else {
            return Err(LedgerError::InvalidStatus(format!(
                "unknown request_id {request_id}"
            )));
        };
        let _lock = lock_exclusive(&self.thread_lock_path(&turn.thread_id))?;
        let mut turn = self.read_turn(&turn.id)?;
        if turn.status.is_terminal() {
            return Err(LedgerError::InvalidStatus(format!(
                "stale request_id {request_id}"
            )));
        }
        if turn.pending_request_id != request_id {
            return Err(LedgerError::InvalidStatus(format!(
                "unknown request_id {request_id}"
            )));
        }
        if !turn.pending_decision.is_empty() {
            return Err(LedgerError::InvalidStatus(format!(
                "duplicate request_id {request_id}"
            )));
        }
        turn.pending_decision = decision.as_str().to_string();
        write_json_atomic(&self.turn_path(&turn.id), &turn)?;
        self.wake(&turn.id);
        Ok(turn)
    }

    /// Wait until a parked request has a decision, or the turn is terminal.
    pub fn wait_pending_decision(
        &self,
        turn_id: &str,
        request_id: &str,
    ) -> Result<Option<ApprovalDecision>, LedgerError> {
        validate_id(turn_id)?;
        validate_id(request_id)?;
        let pair = self.waiter(turn_id);
        let result = (|| loop {
            let turn = self.observe(turn_id)?;
            if turn.status.is_terminal() {
                return Ok(None);
            }
            if turn.pending_request_id == request_id && !turn.pending_decision.is_empty() {
                let decision = parse_approval_decision(&turn.pending_decision)
                    .map_err(|err| LedgerError::InvalidStatus(err.to_string()))?;
                return Ok(Some(decision));
            }
            let (lock, cv) = &*pair;
            let guard = lock.lock().unwrap_or_else(|e| e.into_inner());
            let _ = cv.wait_timeout(guard, Duration::from_millis(20));
        })();
        self.release_waiter(turn_id, &pair);
        result
    }

    /// Clear a consumed pending approval so another request can park.
    pub fn clear_pending(&self, turn_id: &str, request_id: &str) -> Result<Turn, LedgerError> {
        validate_id(turn_id)?;
        let turn = self.read_turn(turn_id)?;
        let _lock = lock_exclusive(&self.thread_lock_path(&turn.thread_id))?;
        let mut turn = self.read_turn(turn_id)?;
        if turn.pending_request_id == request_id {
            turn.pending_request_id.clear();
            turn.pending_decision.clear();
            write_json_atomic(&self.turn_path(&turn.id), &turn)?;
            self.wake(&turn.id);
        }
        Ok(turn)
    }

    fn turn_by_pending_request(&self, request_id: &str) -> Result<Option<Turn>, LedgerError> {
        for turn in self.list_turns(None)? {
            if turn.pending_request_id == request_id {
                return Ok(Some(turn));
            }
        }
        Ok(None)
    }

    pub fn publish_terminal(
        &self,
        turn_id: &str,
        status: TurnStatus,
        stop_reason: &str,
        failure_reason: &str,
    ) -> Result<Turn, LedgerError> {
        validate_id(turn_id)?;
        if !status.is_terminal() {
            return Err(LedgerError::InvalidStatus(
                "publish_terminal requires a terminal TurnStatus".to_string(),
            ));
        }
        let turn = self.read_turn(turn_id)?;
        let _lock = lock_exclusive(&self.thread_lock_path(&turn.thread_id))?;
        let turn = self.read_turn(turn_id)?;
        if turn.status.is_terminal() {
            return Err(LedgerError::AlreadyTerminal {
                turn_id: turn.id,
                status: turn.status,
            });
        }
        self.publish_terminal_locked(&turn, status, stop_reason, failure_reason)
    }

    fn publish_terminal_locked(
        &self,
        turn: &Turn,
        status: TurnStatus,
        stop_reason: &str,
        failure_reason: &str,
    ) -> Result<Turn, LedgerError> {
        if turn.status.is_terminal() {
            return Ok(turn.clone());
        }
        let mut next = turn.clone();
        next.status = status;
        next.stop_reason = stop_reason.to_string();
        next.failure_reason = failure_reason.to_string();
        write_json_atomic(&self.turn_path(&next.id), &next)?;
        if let Ok(mut thread) = self.read_thread(&next.thread_id) {
            if thread.in_progress_turn_id == next.id {
                thread.in_progress_turn_id.clear();
                let _ = write_json_atomic(&self.thread_path(&thread.id), &thread);
            }
        }
        self.release_live(&next.generation_id);
        self.wake_and_drop(&next.id);
        Ok(next)
    }

    pub fn upsert_item(&self, turn_id: &str, item: Item) -> Result<Turn, LedgerError> {
        validate_id(turn_id)?;
        validate_id(&item.id)?;
        let turn = self.read_turn(turn_id)?;
        let _lock = lock_exclusive(&self.thread_lock_path(&turn.thread_id))?;
        let mut turn = self.read_turn(turn_id)?;
        if turn.status.is_terminal() {
            return Err(LedgerError::AlreadyTerminal {
                turn_id: turn.id,
                status: turn.status,
            });
        }
        if let Some(existing) = turn.items.iter_mut().find(|i| i.id == item.id) {
            *existing = item;
        } else {
            turn.items.push(item);
        }
        write_json_atomic(&self.turn_path(&turn.id), &turn)?;
        self.wake(&turn.id);
        Ok(turn)
    }

    pub fn set_child_pid(
        &self,
        generation_id: &str,
        child_pid: u32,
    ) -> Result<Generation, LedgerError> {
        validate_id(generation_id)?;
        let _glock = lock_exclusive(&self.generation_lock_path(generation_id))?;
        let mut generation = self.read_generation(generation_id)?;
        generation.child_pid = Some(child_pid);
        generation.child_started_epoch = Some(unix_epoch_secs());
        generation.child_eof = false;
        write_json_atomic(&self.generation_path(generation_id), &generation)?;
        Ok(generation)
    }

    pub fn mark_child_eof(&self, generation_id: &str) -> Result<Generation, LedgerError> {
        validate_id(generation_id)?;
        let _glock = lock_exclusive(&self.generation_lock_path(generation_id))?;
        let mut generation = self.read_generation(generation_id)?;
        generation.child_eof = true;
        write_json_atomic(&self.generation_path(generation_id), &generation)?;
        if !generation.turn_id.is_empty() {
            self.wake(&generation.turn_id);
        } else {
            self.wake_all();
        }
        Ok(generation)
    }

    /// Block until the turn is terminal, parked for `pending_approval`, or
    /// until `timeout` elapses.
    ///
    /// A timeout returns the latest snapshot (possibly still `inProgress`) and
    /// does not cancel the turn. Owner pid death, child pid death, and ACP EOF
    /// converge immediately via [`Self::observe`]. `pending_approval` is not a
    /// TurnStatus.
    pub fn wait(&self, turn_id: &str, timeout: Option<Duration>) -> Result<Turn, LedgerError> {
        validate_id(turn_id)?;
        let deadline = timeout.map(|d| Instant::now() + d);
        let pair = self.waiter(turn_id);
        let result = (|| loop {
            let turn = self.observe(turn_id)?;
            if turn.status.is_terminal() || turn.pending_approval() {
                return Ok(turn);
            }
            let slice = Duration::from_millis(20);
            let wait_for = match deadline {
                Some(dl) => {
                    let now = Instant::now();
                    if now >= dl {
                        return Ok(turn);
                    }
                    dl.saturating_duration_since(now).min(slice)
                }
                None => slice,
            };
            let (lock, cv) = &*pair;
            let guard = lock.lock().unwrap_or_else(|e| e.into_inner());
            let _ = cv.wait_timeout(guard, wait_for);
        })();
        self.release_waiter(turn_id, &pair);
        result
    }

    pub fn list_turns(&self, cwd: Option<&str>) -> Result<Vec<Turn>, LedgerError> {
        let mut turns = Vec::new();
        let dir = self.home.join(TURNS_DIR);
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if !path.is_file() {
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let turn: Turn = read_json(&path)?;
            let turn = if turn.status.is_terminal() {
                turn
            } else {
                self.observe(&turn.id)?
            };
            if let Some(cwd) = cwd {
                let thread = self.read_thread(&turn.thread_id)?;
                if thread.cwd != cwd {
                    continue;
                }
            }
            turns.push(turn);
        }
        turns.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(turns)
    }

    fn lookup_client(&self, key: &str) -> Result<Option<String>, LedgerError> {
        let path = self.client_path(key);
        match read_json::<String>(&path) {
            Ok(id) => Ok(Some(id)),
            Err(LedgerError::Io(err)) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }

    fn store_client(&self, key: &str, turn_id: &str) -> Result<(), LedgerError> {
        let path = self.client_path(key);
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        write_json_atomic(&path, &turn_id.to_string())?;
        Ok(())
    }

    fn in_flight_by_client(&self, key: &str) -> Result<Option<Turn>, LedgerError> {
        if let Some(id) = self.lookup_client(key)? {
            match self.observe(&id) {
                Ok(turn) if !turn.status.is_terminal() && turn.client_request_id == key => {
                    return Ok(Some(turn));
                }
                Ok(_) | Err(LedgerError::NotFound { .. }) => {}
                Err(err) => return Err(err),
            }
        }
        for path in self.turn_json_paths()? {
            let turn: Turn = read_json(&path)?;
            if turn.client_request_id != key {
                continue;
            }
            let turn = self.observe(&turn.id)?;
            if !turn.status.is_terminal() {
                return Ok(Some(turn));
            }
        }
        Ok(None)
    }

    fn live_turn_on_thread_locked(&self, thread: &Thread) -> Result<Option<Turn>, LedgerError> {
        if !thread.in_progress_turn_id.is_empty() {
            match self.observe_locked(&thread.in_progress_turn_id) {
                Ok(turn) if !turn.status.is_terminal() => return Ok(Some(turn)),
                Ok(_) | Err(LedgerError::NotFound { .. }) => {}
                Err(err) => return Err(err),
            }
        }
        for path in self.turn_json_paths()? {
            let turn: Turn = read_json(&path)?;
            if turn.thread_id != thread.id {
                continue;
            }
            let turn = if turn.status.is_terminal() {
                turn
            } else {
                self.observe_locked(&turn.id)?
            };
            if !turn.status.is_terminal() {
                return Ok(Some(turn));
            }
        }
        Ok(None)
    }

    fn turn_json_paths(&self) -> Result<Vec<PathBuf>, LedgerError> {
        let mut paths = Vec::new();
        for entry in fs::read_dir(self.home.join(TURNS_DIR))? {
            let path = entry?.path();
            if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("json") {
                paths.push(path);
            }
        }
        Ok(paths)
    }

    fn hold_live(&self, generation_id: &str) {
        let path = self.generation_live_path(generation_id);
        let lock = lock_exclusive(&path).ok();
        PROCESS_LIVE
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(path.clone());
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner
            .live_files
            .insert(generation_id.to_string(), LiveHold { path, _lock: lock });
    }

    fn release_live(&self, generation_id: &str) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.live_files.remove(generation_id);
    }

    fn owner_instance_alive(&self, generation: &Generation) -> bool {
        let path = self.generation_live_path(&generation.id);
        if process_holds_live(&path) {
            return true;
        }
        match try_lock_exclusive_nb(&path) {
            Ok(held_by_other) => held_by_other,
            Err(_) => process_alive(generation.owner_pid, Some(generation.started_epoch)),
        }
    }

    fn generation_is_dead(&self, generation: &Generation) -> bool {
        if !self.owner_instance_alive(generation) {
            return true;
        }
        if generation.child_eof {
            return true;
        }
        match generation.child_pid {
            Some(pid) => !process_alive(pid, generation.child_started_epoch),
            None => false,
        }
    }

    fn waiter(&self, turn_id: &str) -> Arc<(Mutex<()>, Condvar)> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner
            .waiters
            .entry(turn_id.to_string())
            .or_insert_with(|| Arc::new((Mutex::new(()), Condvar::new())))
            .clone()
    }

    fn wake(&self, turn_id: &str) {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(pair) = inner.waiters.get(turn_id) {
            let _g = pair.0.lock().unwrap_or_else(|e| e.into_inner());
            pair.1.notify_all();
        }
    }

    fn wake_and_drop(&self, turn_id: &str) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(pair) = inner.waiters.remove(turn_id) {
            let _g = pair.0.lock().unwrap_or_else(|e| e.into_inner());
            pair.1.notify_all();
        }
    }

    fn release_waiter(&self, turn_id: &str, pair: &Arc<(Mutex<()>, Condvar)>) {
        pair.1.notify_all();
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let last = inner.waiters.get(turn_id).is_some_and(|existing| {
            Arc::ptr_eq(existing, pair) && Arc::strong_count(existing) <= 2
        });
        if last {
            inner.waiters.remove(turn_id);
        }
    }

    fn wake_all(&self) {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        for pair in inner.waiters.values() {
            let _g = pair.0.lock().unwrap_or_else(|e| e.into_inner());
            pair.1.notify_all();
        }
    }

    #[cfg(test)]
    fn waiter_count(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .waiters
            .len()
    }

    fn thread_path(&self, id: &str) -> PathBuf {
        self.home.join(THREADS_DIR).join(format!("{id}.json"))
    }

    fn thread_lock_path(&self, id: &str) -> PathBuf {
        self.home.join(THREADS_DIR).join(format!("{id}.lock"))
    }

    fn turn_path(&self, id: &str) -> PathBuf {
        self.home.join(TURNS_DIR).join(format!("{id}.json"))
    }

    fn generation_path(&self, id: &str) -> PathBuf {
        self.home.join(GENERATIONS_DIR).join(format!("{id}.json"))
    }

    fn generation_lock_path(&self, id: &str) -> PathBuf {
        self.home.join(GENERATIONS_DIR).join(format!("{id}.lock"))
    }

    fn generation_live_path(&self, id: &str) -> PathBuf {
        self.home.join(GENERATIONS_DIR).join(format!("{id}.live"))
    }

    fn client_path(&self, key: &str) -> PathBuf {
        self.home
            .join(TURNS_DIR)
            .join(BY_CLIENT_DIR)
            .join(client_filename(key))
    }
}

fn map_not_found(err: LedgerError, kind: &'static str, id: &str) -> LedgerError {
    match err {
        LedgerError::Io(e) if e.kind() == io::ErrorKind::NotFound => LedgerError::NotFound {
            kind,
            id: id.to_string(),
        },
        other => other,
    }
}

fn write_json_atomic(path: &Path, value: &impl Serialize) -> Result<(), LedgerError> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_file_name(format!(
        "{}.{}.{}.tmp",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("tmp"),
        std::process::id(),
        ID_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let written = (|| -> Result<(), LedgerError> {
        let mut file = File::create(&tmp)?;
        serde_json::to_writer(&mut file, value)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::rename(&tmp, path)?;
        Ok(())
    })();
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    written
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, LedgerError> {
    let body = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&body)?)
}

fn validate_id(id: &str) -> Result<(), LedgerError> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(LedgerError::InvalidId(format!("invalid id {id:?}")));
    }
    Ok(())
}

fn new_id(prefix: &str) -> String {
    let n = ID_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}{:x}{:x}{n:x}", std::process::id(), unix_nanos())
}

fn unix_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn unix_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

fn client_filename(id: &str) -> String {
    let mut s = String::with_capacity(id.len() * 2);
    for b in id.as_bytes() {
        s.push_str(&format!("{b:02x}"));
    }
    if s.len() <= 200 {
        return s;
    }
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in id.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("h{h:016x}")
}

#[cfg_attr(not(test), allow(dead_code))]
fn parse_proc_stat(body: &str) -> Option<(char, u64)> {
    let rest = body.rsplit_once(')')?.1.trim_start();
    let mut fields = rest.split_whitespace();
    let state = fields.next()?.chars().next()?;
    let start_ticks = fields.nth(18)?.parse().ok()?;
    Some((state, start_ticks))
}

#[cfg_attr(not(test), allow(dead_code))]
fn proc_stat_alive(
    body: &str,
    start_epoch_secs: Option<u64>,
    not_started_after: Option<u64>,
) -> bool {
    let Some((state, _)) = parse_proc_stat(body) else {
        return false;
    };
    if state == 'Z' {
        return false;
    }
    if let (Some(epoch), Some(started)) = (not_started_after, start_epoch_secs) {
        if started > epoch {
            return false;
        }
    }
    true
}

#[cfg(not(target_os = "macos"))]
fn linux_start_epoch_secs(start_ticks: u64) -> Option<u64> {
    let ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if ticks <= 0 {
        return None;
    }
    let btime = fs::read_to_string("/proc/stat").ok().and_then(|body| {
        body.lines().find_map(|line| {
            line.strip_prefix("btime ")
                .and_then(|v| v.trim().parse::<u64>().ok())
        })
    })?;
    Some(btime.saturating_add(start_ticks / ticks as u64))
}

const PROC_PIDTBSDINFO: i32 = 3;
const SZOMB: u32 = 5;

#[repr(C)]
struct ProcBsdInfo {
    pbi_flags: u32,
    pbi_status: u32,
    pbi_xstatus: u32,
    pbi_pid: u32,
    pbi_ppid: u32,
    pbi_uid: u32,
    pbi_gid: u32,
    pbi_ruid: u32,
    pbi_rgid: u32,
    pbi_svuid: u32,
    pbi_svgid: u32,
    rfu_1: u32,
    pbi_comm: [u8; 16],
    pbi_name: [u8; 32],
    pbi_nfiles: u32,
    pbi_pgid: u32,
    pbi_pjobc: u32,
    e_tdev: u32,
    e_tpgid: u32,
    pbi_nice: i32,
    pbi_start_tvsec: u64,
    pbi_start_tvusec: u64,
}

fn proc_bsdinfo(pid: u32) -> Option<ProcBsdInfo> {
    #[cfg(target_os = "macos")]
    {
        extern "C" {
            fn proc_pidinfo(
                pid: libc::c_int,
                flavor: libc::c_int,
                arg: u64,
                buffer: *mut libc::c_void,
                buffersize: libc::c_int,
            ) -> libc::c_int;
        }
        let mut info = unsafe { std::mem::zeroed::<ProcBsdInfo>() };
        let size = std::mem::size_of::<ProcBsdInfo>() as i32;
        let n = unsafe {
            proc_pidinfo(
                pid as i32,
                PROC_PIDTBSDINFO,
                0,
                &mut info as *mut ProcBsdInfo as *mut libc::c_void,
                size,
            )
        };
        if n == size {
            Some(info)
        } else {
            None
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = pid;
        None
    }
}

fn process_alive(pid: u32, not_started_after: Option<u64>) -> bool {
    if pid == 0 {
        return false;
    }
    if let Some(info) = proc_bsdinfo(pid) {
        if info.pbi_status == SZOMB {
            return false;
        }
        if let Some(epoch) = not_started_after {
            if info.pbi_start_tvsec > epoch {
                return false;
            }
        }
        return true;
    }
    // macOS: proc_pidinfo fails for unreaped zombies; kill(0) still succeeds.
    #[cfg(target_os = "macos")]
    {
        false
    }
    #[cfg(not(target_os = "macos"))]
    {
        let body = match fs::read_to_string(format!("/proc/{pid}/stat")) {
            Ok(body) => body,
            Err(_) => return false,
        };
        let start_epoch =
            parse_proc_stat(&body).and_then(|(_, ticks)| linux_start_epoch_secs(ticks));
        proc_stat_alive(&body, start_epoch, not_started_after)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_wire::{ITEM_AGENT_MESSAGE, ITEM_FILE_CHANGE};
    use std::sync::Arc;
    use std::thread;

    fn sample_thread(cwd: &str) -> NewThread {
        NewThread {
            cwd: cwd.to_string(),
            model: "grok".to_string(),
            sandbox: ThreadSandbox::WorkspaceWrite,
            approval_policy: ApprovalPolicy::Never,
            developer_instructions: String::new(),
            acp_session_id: String::new(),
        }
    }

    fn sample_turn(thread_id: &str, client: Option<&str>) -> NewTurn {
        NewTurn {
            thread_id: thread_id.to_string(),
            input: vec![UserInput {
                kind: "text".to_string(),
                text: "do the work".to_string(),
            }],
            model: "grok".to_string(),
            effort: "low".to_string(),
            client_request_id: client.map(str::to_string),
        }
    }

    fn open_tmp() -> (tempfile::TempDir, Ledger) {
        let dir = tempfile::tempdir().unwrap();
        let ledger = Ledger::open(dir.path()).unwrap();
        (dir, ledger)
    }

    #[test]
    fn open_creates_layout_and_persists_records() {
        let (dir, ledger) = open_tmp();
        let home = dir.path();
        assert!(home.join(THREADS_DIR).is_dir());
        assert!(home.join(TURNS_DIR).is_dir());
        assert!(home.join(GENERATIONS_DIR).is_dir());
        let thread = ledger.create_thread(&sample_thread("/work")).unwrap();
        let turn = ledger.admit_turn(&sample_turn(&thread.id, None)).unwrap();
        let thread_path = home.join(THREADS_DIR).join(format!("{}.json", thread.id));
        let turn_path = home.join(TURNS_DIR).join(format!("{}.json", turn.id));
        let gen_path = home
            .join(GENERATIONS_DIR)
            .join(format!("{}.json", turn.generation_id));
        assert!(thread_path.is_file(), "missing {}", thread_path.display());
        assert!(turn_path.is_file(), "missing {}", turn_path.display());
        assert!(gen_path.is_file(), "missing {}", gen_path.display());
        let loaded_turn: Turn = read_json(&turn_path).unwrap();
        assert_eq!(loaded_turn, ledger.read_turn(&turn.id).unwrap());
        assert_eq!(loaded_turn.status, TurnStatus::InProgress);
        let loaded_thread: Thread = read_json(&thread_path).unwrap();
        assert_eq!(loaded_thread.in_progress_turn_id, turn.id);
        let loaded_gen: Generation = read_json(&gen_path).unwrap();
        assert_eq!(loaded_gen.owner_pid, std::process::id());
        assert_eq!(loaded_gen.id, turn.generation_id);
    }

    #[test]
    fn one_in_progress_turn_per_thread() {
        let (_dir, ledger) = open_tmp();
        let thread = ledger.create_thread(&sample_thread("/work")).unwrap();
        let first = ledger.admit_turn(&sample_turn(&thread.id, None)).unwrap();
        let err = ledger
            .admit_turn(&sample_turn(&thread.id, None))
            .unwrap_err();
        match err {
            LedgerError::TurnInProgress { thread_id, turn_id } => {
                assert_eq!(thread_id, thread.id);
                assert_eq!(turn_id, first.id);
            }
            other => panic!("expected TurnInProgress, got {other}"),
        }
        ledger
            .publish_terminal(&first.id, TurnStatus::Completed, "end_turn", "")
            .unwrap();
        let second = ledger.admit_turn(&sample_turn(&thread.id, None)).unwrap();
        assert_ne!(second.id, first.id);
        assert_eq!(second.status, TurnStatus::InProgress);
    }

    #[test]
    fn waiter_wakes_on_terminal_publish_and_does_not_miss() {
        let (_dir, ledger) = open_tmp();
        let ledger = Arc::new(ledger);
        let thread = ledger.create_thread(&sample_thread("/work")).unwrap();
        let turn = ledger.admit_turn(&sample_turn(&thread.id, None)).unwrap();

        let already = Arc::clone(&ledger);
        let done_id = turn.id.clone();
        already
            .publish_terminal(&turn.id, TurnStatus::Completed, "end_turn", "")
            .unwrap();
        let snapshot = already.wait(&done_id, None).unwrap();
        assert_eq!(snapshot.status, TurnStatus::Completed);

        let turn = ledger.admit_turn(&sample_turn(&thread.id, None)).unwrap();
        let waiter_ledger = Arc::clone(&ledger);
        let wait_id = turn.id.clone();
        let handle = thread::spawn(move || {
            waiter_ledger
                .wait(&wait_id, Some(Duration::from_secs(3)))
                .unwrap()
        });
        thread::sleep(Duration::from_millis(30));
        let published = ledger
            .publish_terminal(&turn.id, TurnStatus::Interrupted, "cancelled", "")
            .unwrap();
        assert_eq!(published.status, TurnStatus::Interrupted);
        let waited = handle.join().expect("waiter thread");
        assert_eq!(waited.status, TurnStatus::Interrupted);
        assert_eq!(waited.id, turn.id);
    }

    #[test]
    fn child_death_and_eof_resolve_waiters() {
        let (_dir, ledger) = open_tmp();
        let thread = ledger.create_thread(&sample_thread("/work")).unwrap();
        let turn = ledger.admit_turn(&sample_turn(&thread.id, None)).unwrap();
        let mut child = std::process::Command::new("sleep")
            .arg("60")
            .spawn()
            .unwrap();
        ledger
            .set_child_pid(&turn.generation_id, child.id())
            .unwrap();
        child.kill().unwrap();
        let got = ledger.wait(&turn.id, Some(Duration::from_secs(2))).unwrap();
        let _ = child.wait();
        assert_eq!(got.status, TurnStatus::Failed);
        assert_eq!(got.failure_reason, FAILURE_WORKER_GONE);

        let turn = ledger.admit_turn(&sample_turn(&thread.id, None)).unwrap();
        ledger.mark_child_eof(&turn.generation_id).unwrap();
        let got = ledger.wait(&turn.id, Some(Duration::from_secs(2))).unwrap();
        assert_eq!(got.status, TurnStatus::Failed);
        assert_eq!(got.failure_reason, FAILURE_WORKER_GONE);
    }

    #[test]
    fn terminal_record_is_not_overwritten_with_worker_gone() {
        let (_dir, ledger) = open_tmp();
        let thread = ledger.create_thread(&sample_thread("/work")).unwrap();
        let turn = ledger.admit_turn(&sample_turn(&thread.id, None)).unwrap();
        let mut child = std::process::Command::new("sleep")
            .arg("60")
            .spawn()
            .unwrap();
        ledger
            .set_child_pid(&turn.generation_id, child.id())
            .unwrap();
        ledger
            .publish_terminal(&turn.id, TurnStatus::Completed, "end_turn", "")
            .unwrap();
        child.kill().unwrap();
        child.wait().unwrap();
        ledger.mark_child_eof(&turn.generation_id).unwrap();
        let got = ledger.observe(&turn.id).unwrap();
        assert_eq!(got.status, TurnStatus::Completed);
        assert_eq!(got.failure_reason, "");
        assert_eq!(got.stop_reason, "end_turn");
    }

    #[test]
    fn cancel_publishes_interrupted_and_is_idempotent() {
        let (_dir, ledger) = open_tmp();
        let thread = ledger.create_thread(&sample_thread("/work")).unwrap();
        let turn = ledger.admit_turn(&sample_turn(&thread.id, None)).unwrap();
        let first = ledger.cancel(&turn.id).unwrap();
        assert_eq!(first.status, TurnStatus::Interrupted);
        assert_eq!(first.stop_reason, "cancelled");
        let second = ledger.cancel(&turn.id).unwrap();
        assert_eq!(second.status, TurnStatus::Interrupted);
        assert_eq!(second.stop_reason, "cancelled");
        assert_eq!(second.failure_reason, "");
    }

    #[test]
    fn cancel_does_not_overwrite_worker_gone() {
        let (_dir, ledger) = open_tmp();
        let thread = ledger.create_thread(&sample_thread("/work")).unwrap();
        let turn = ledger.admit_turn(&sample_turn(&thread.id, None)).unwrap();
        let mut child = std::process::Command::new("sleep")
            .arg("60")
            .spawn()
            .unwrap();
        ledger
            .set_child_pid(&turn.generation_id, child.id())
            .unwrap();
        child.kill().unwrap();
        child.wait().unwrap();
        let gone = ledger.observe(&turn.id).unwrap();
        assert_eq!(gone.status, TurnStatus::Failed);
        assert_eq!(gone.failure_reason, FAILURE_WORKER_GONE);
        let after = ledger.cancel(&turn.id).unwrap();
        assert_eq!(after.status, TurnStatus::Failed);
        assert_eq!(after.failure_reason, FAILURE_WORKER_GONE);
    }

    #[test]
    fn cancel_observes_dead_child_as_worker_gone() {
        let (_dir, ledger) = open_tmp();
        let thread = ledger.create_thread(&sample_thread("/work")).unwrap();
        let turn = ledger.admit_turn(&sample_turn(&thread.id, None)).unwrap();
        let mut child = std::process::Command::new("sleep")
            .arg("60")
            .spawn()
            .unwrap();
        ledger
            .set_child_pid(&turn.generation_id, child.id())
            .unwrap();
        child.kill().unwrap();
        child.wait().unwrap();
        let cancelled = ledger.cancel(&turn.id).unwrap();
        assert_eq!(cancelled.status, TurnStatus::Failed);
        assert_eq!(cancelled.failure_reason, FAILURE_WORKER_GONE);
        assert_ne!(cancelled.status, TurnStatus::Interrupted);
    }

    #[test]
    fn recorded_child_is_alive_uses_start_epoch() {
        let (_dir, ledger) = open_tmp();
        let thread = ledger.create_thread(&sample_thread("/work")).unwrap();
        let turn = ledger.admit_turn(&sample_turn(&thread.id, None)).unwrap();
        let mut child = std::process::Command::new("sleep")
            .arg("60")
            .spawn()
            .unwrap();
        let generation = ledger
            .set_child_pid(&turn.generation_id, child.id())
            .unwrap();
        assert!(Ledger::recorded_child_is_alive(&generation));
        child.kill().unwrap();
        child.wait().unwrap();
        let generation = ledger.read_generation(&turn.generation_id).unwrap();
        assert!(!Ledger::recorded_child_is_alive(&generation));
    }

    #[test]
    fn park_and_respond_and_reject_bad_ids() {
        use crate::source_wire::ApprovalDecision;
        let (_dir, ledger) = open_tmp();
        let thread = ledger.create_thread(&sample_thread("/work")).unwrap();
        let turn = ledger.admit_turn(&sample_turn(&thread.id, None)).unwrap();
        let parked = ledger.park_approval(&turn.id).unwrap();
        assert!(parked.pending_approval());
        let request_id = parked.pending_request_id.clone();
        let err = ledger
            .respond("missing-id", ApprovalDecision::Accept)
            .unwrap_err();
        assert!(err.to_string().contains("unknown request_id"));
        let accepted = ledger
            .respond(&request_id, ApprovalDecision::Accept)
            .unwrap();
        assert_eq!(accepted.pending_decision, "accept");
        let dup = ledger
            .respond(&request_id, ApprovalDecision::Accept)
            .unwrap_err();
        assert!(dup.to_string().contains("duplicate request_id"));
        ledger
            .publish_terminal(&turn.id, TurnStatus::Completed, "end_turn", "")
            .unwrap();
        let stale = ledger
            .respond(&request_id, ApprovalDecision::Accept)
            .unwrap_err();
        assert!(
            stale.to_string().contains("stale request_id")
                || stale.to_string().contains("unknown request_id")
        );
    }

    #[test]
    fn wait_returns_on_pending_approval() {
        let (_dir, ledger) = open_tmp();
        let ledger = Arc::new(ledger);
        let thread = ledger.create_thread(&sample_thread("/work")).unwrap();
        let turn = ledger.admit_turn(&sample_turn(&thread.id, None)).unwrap();
        let waiter = {
            let ledger = ledger.clone();
            let id = turn.id.clone();
            std::thread::spawn(move || ledger.wait(&id, Some(Duration::from_secs(2))).unwrap())
        };
        std::thread::sleep(Duration::from_millis(20));
        let parked = ledger.park_approval(&turn.id).unwrap();
        let got = waiter.join().expect("waiter");
        assert!(got.pending_approval());
        assert_eq!(got.status, TurnStatus::InProgress);
        assert_eq!(got.pending_request_id, parked.pending_request_id);
        assert_ne!(got.status.as_str(), "pending_approval");
    }

    #[test]
    fn set_acp_session_id_persists() {
        let (_dir, ledger) = open_tmp();
        let thread = ledger.create_thread(&sample_thread("/work")).unwrap();
        assert!(thread.acp_session_id.is_empty());
        let stored = ledger.set_acp_session_id(&thread.id, "sess-live").unwrap();
        assert_eq!(stored.acp_session_id, "sess-live");
        assert_eq!(
            ledger.read_thread(&thread.id).unwrap().acp_session_id,
            "sess-live"
        );
    }

    #[test]
    fn client_request_id_dedup_in_flight_only() {
        let (_dir, ledger) = open_tmp();
        let thread = ledger.create_thread(&sample_thread("/work")).unwrap();
        let first = ledger
            .admit_turn(&sample_turn(&thread.id, Some("req-1")))
            .unwrap();
        let again = ledger
            .admit_turn(&sample_turn(&thread.id, Some("req-1")))
            .unwrap();
        assert_eq!(first.id, again.id);
        ledger
            .publish_terminal(&first.id, TurnStatus::Completed, "end_turn", "")
            .unwrap();
        let next = ledger
            .admit_turn(&sample_turn(&thread.id, Some("req-1")))
            .unwrap();
        assert_ne!(next.id, first.id);
    }

    #[test]
    fn items_survive_reopen() {
        let (dir, ledger) = open_tmp();
        let thread = ledger.create_thread(&sample_thread("/work")).unwrap();
        let turn = ledger.admit_turn(&sample_turn(&thread.id, None)).unwrap();
        ledger
            .upsert_item(
                &turn.id,
                Item {
                    id: "item1".to_string(),
                    item_type: ITEM_AGENT_MESSAGE.to_string(),
                    text: "hello".to_string(),
                    status: "completed".to_string(),
                },
            )
            .unwrap();
        ledger
            .upsert_item(
                &turn.id,
                Item {
                    id: "item1".to_string(),
                    item_type: ITEM_FILE_CHANGE.to_string(),
                    text: "patched".to_string(),
                    status: "completed".to_string(),
                },
            )
            .unwrap();
        drop(ledger);
        let reopened = Ledger::open(dir.path()).unwrap();
        let loaded = reopened.read_turn(&turn.id).unwrap();
        assert_eq!(loaded.items.len(), 1);
        assert_eq!(loaded.items[0].item_type, ITEM_FILE_CHANGE);
        assert_eq!(loaded.items[0].text, "patched");
    }

    #[test]
    fn list_turns_filters_cwd_and_observes() {
        let (_dir, ledger) = open_tmp();
        let a = ledger.create_thread(&sample_thread("/a")).unwrap();
        let b = ledger.create_thread(&sample_thread("/b")).unwrap();
        let ta = ledger.admit_turn(&sample_turn(&a.id, None)).unwrap();
        let _tb = ledger.admit_turn(&sample_turn(&b.id, None)).unwrap();
        ledger
            .publish_terminal(&ta.id, TurnStatus::Completed, "end_turn", "")
            .unwrap();
        let listed = ledger.list_turns(Some("/a")).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, ta.id);
        assert_eq!(listed[0].status, TurnStatus::Completed);
    }

    #[test]
    fn wait_timeout_does_not_cancel() {
        let (_dir, ledger) = open_tmp();
        let thread = ledger.create_thread(&sample_thread("/work")).unwrap();
        let turn = ledger.admit_turn(&sample_turn(&thread.id, None)).unwrap();
        let got = ledger
            .wait(&turn.id, Some(Duration::from_millis(40)))
            .unwrap();
        assert_eq!(got.status, TurnStatus::InProgress);
        assert_eq!(
            ledger.read_turn(&turn.id).unwrap().status,
            TurnStatus::InProgress
        );
        assert_eq!(ledger.waiter_count(), 0);
        match ledger.wait("tuMissingTurn1", Some(Duration::from_millis(20))) {
            Err(LedgerError::NotFound { .. }) => {}
            other => panic!("expected NotFound, got {other:?}"),
        }
        assert_eq!(ledger.waiter_count(), 0);
    }

    #[test]
    fn wait_prunes_waiter_slot() {
        let (_dir, ledger) = open_tmp();
        let ledger = Arc::new(ledger);
        let thread = ledger.create_thread(&sample_thread("/work")).unwrap();
        let turn = ledger.admit_turn(&sample_turn(&thread.id, None)).unwrap();
        assert_eq!(ledger.waiter_count(), 0);
        let waiter_ledger = Arc::clone(&ledger);
        let wait_id = turn.id.clone();
        let handle = thread::spawn(move || {
            waiter_ledger
                .wait(&wait_id, Some(Duration::from_secs(3)))
                .unwrap()
        });
        let deadline = Instant::now() + Duration::from_secs(2);
        while ledger.waiter_count() != 1 {
            assert!(
                Instant::now() < deadline,
                "waiter thread never registered a slot"
            );
            thread::sleep(Duration::from_millis(5));
        }
        ledger
            .publish_terminal(&turn.id, TurnStatus::Completed, "end_turn", "")
            .unwrap();
        handle.join().expect("waiter thread");
        assert_eq!(ledger.waiter_count(), 0);
    }

    #[test]
    fn list_turns_fails_closed_on_corrupt_json() {
        let (dir, ledger) = open_tmp();
        let thread = ledger.create_thread(&sample_thread("/work")).unwrap();
        let _ = ledger.admit_turn(&sample_turn(&thread.id, None)).unwrap();
        fs::write(
            dir.path().join(TURNS_DIR).join("broken.json"),
            "{not json\n",
        )
        .unwrap();
        match ledger.list_turns(None) {
            Err(LedgerError::Json(_)) => {}
            other => panic!("expected Json error, got {other:?}"),
        }
    }

    #[test]
    fn stale_client_index_does_not_dedup_wrong_turn() {
        let (_dir, ledger) = open_tmp();
        let thread_a = ledger.create_thread(&sample_thread("/a")).unwrap();
        let first = ledger
            .admit_turn(&sample_turn(&thread_a.id, Some("req-1")))
            .unwrap();
        write_json_atomic(&ledger.client_path("req-2"), &first.id).unwrap();
        let thread_b = ledger.create_thread(&sample_thread("/b")).unwrap();
        let second = ledger
            .admit_turn(&sample_turn(&thread_b.id, Some("req-2")))
            .unwrap();
        assert_ne!(second.id, first.id);
        assert_eq!(second.thread_id, thread_b.id);
        assert_eq!(second.client_request_id, "req-2");
    }

    #[test]
    fn missing_thread_pointer_still_blocks_second_admit() {
        let (_dir, ledger) = open_tmp();
        let thread = ledger.create_thread(&sample_thread("/work")).unwrap();
        let first = ledger.admit_turn(&sample_turn(&thread.id, None)).unwrap();
        let mut rec = ledger.read_thread(&thread.id).unwrap();
        rec.in_progress_turn_id.clear();
        write_json_atomic(&ledger.thread_path(&thread.id), &rec).unwrap();
        let err = ledger
            .admit_turn(&sample_turn(&thread.id, None))
            .unwrap_err();
        match err {
            LedgerError::TurnInProgress { turn_id, .. } => assert_eq!(turn_id, first.id),
            other => panic!("expected TurnInProgress, got {other}"),
        }
    }

    fn fake_linux_stat(comm: &str, state: char, start_ticks: u64) -> String {
        let pad = ["0"; 18].join(" ");
        format!("1 ({comm}) {state} {pad} {start_ticks}")
    }

    #[test]
    fn proc_stat_zombie_and_pid_reuse() {
        let running = fake_linux_stat("init", 'S', 10);
        assert!(parse_proc_stat(&running).is_some());
        assert!(proc_stat_alive(&running, Some(50), Some(100)));
        let zombie = fake_linux_stat("sleep", 'Z', 10);
        assert!(!proc_stat_alive(&zombie, Some(50), Some(100)));
        let reused = fake_linux_stat("init", 'S', 10);
        assert!(!proc_stat_alive(&reused, Some(200), Some(100)));
        let spaced = fake_linux_stat("some name", 'R', 1);
        assert_eq!(parse_proc_stat(&spaced).unwrap().0, 'R');
        assert!(!proc_stat_alive("not a stat line", None, None));
    }

    #[test]
    fn admit_registers_process_live_owner() {
        let (_dir, ledger) = open_tmp();
        let thread = ledger.create_thread(&sample_thread("/work")).unwrap();
        let turn = ledger.admit_turn(&sample_turn(&thread.id, None)).unwrap();
        let gen = ledger.read_generation(&turn.generation_id).unwrap();
        assert!(process_holds_live(
            &ledger.generation_live_path(&turn.generation_id)
        ));
        assert!(ledger.owner_instance_alive(&gen));
    }

    #[test]
    fn proc_bsdinfo_reads_self() {
        let info = proc_bsdinfo(std::process::id()).expect("proc_pidinfo self");
        assert_ne!(info.pbi_status, SZOMB);
        assert!(info.pbi_start_tvsec > 0);
        assert!(info.pbi_start_tvsec <= unix_epoch_secs());
        assert!(process_alive(std::process::id(), Some(unix_epoch_secs())));
        assert!(!process_alive(std::process::id(), Some(0)));
    }

    #[test]
    fn relative_home_is_rejected() {
        match Ledger::open("relative-home") {
            Err(LedgerError::InvalidHome(reason)) => {
                assert!(reason.contains("absolute"), "{reason}")
            }
            Ok(_) => panic!("expected InvalidHome, opened relative home"),
            Err(other) => panic!("expected InvalidHome, got {other}"),
        }
    }
}
