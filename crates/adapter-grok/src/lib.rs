//! Grok ACP adapter crate. TASK-003 is a fake agent plus stdio harness.
//!
//! Do not import Grok, Claude, or GLM SDKs here. Live `grok agent stdio` was
//! captured in TASK-004 (`captures/task-004`). Mapping `session/update` onto
//! `source_wire` items is TASK-007.

mod fake;
pub mod live_capture;

pub use fake::{run_fake_agent, STUB_REPLY, STUB_SESSION_ID, STUB_TOOL_CALL_ID};
