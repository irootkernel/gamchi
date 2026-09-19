//! Grok ACP adapter crate. TASK-003 is a fake agent plus stdio harness.
//!
//! Do not import Grok, Claude, or GLM SDKs here. Live `grok agent stdio` was
//! captured in TASK-004 (`captures/task-004`). TASK-005 recorded a go
//! (ADR-0002). TASK-007 maps `session/update` onto `source_wire` items and
//! spawns parent-owned `grok agent stdio`.

mod defaults;
mod fake;
mod launch;
pub mod live_capture;
mod map;
mod teardown;
mod turn;

pub use defaults::{
    load_home_defaults, resolve_first_turn, resolve_follow_up, HomeDefaults, ResolveError,
    BUILTIN_EFFORT, BUILTIN_MODEL, GROK_ALIAS,
};
pub use fake::{run_fake_agent, STUB_REPLY, STUB_SESSION_ID, STUB_TOOL_CALL_ID};
pub use launch::{
    plan_launch, ExtraSpawnFields, LaunchError, LaunchPlan, LaunchRequest,
    UNENFORCEABLE_EXTRA_FIELD_NAMES,
};
pub use map::{emit_item_type, ExcludedFatal, Mapper};
pub use teardown::{
    install_signal_teardown, parent_exit_teardown, shutting_down, teardown_process_group,
    track_child,
};
pub use turn::{run_turn, run_turn_on_admit, AdapterError, AgentCommand, TurnOutcome, TurnRequest};
