//! Grok ACP adapter crate. TASK-003 is a fake agent plus stdio harness.
//!
//! Do not import Grok, Claude, or GLM SDKs here. Live `grok agent stdio` is
//! TASK-004. Mapping `session/update` onto `ggwire` items is TASK-007.

mod fake;

pub use fake::{run_fake_agent, STUB_REPLY, STUB_SESSION_ID, STUB_TOOL_CALL_ID};
