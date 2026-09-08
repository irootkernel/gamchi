//! ACP `session/update` → `source_wire` items ([acp-item-mapping.md]).

use agent_client_protocol::schema::v1::{
    ContentBlock, ContentChunk, Plan, SessionUpdate, ToolCall, ToolCallStatus, ToolCallUpdate,
    ToolKind,
};
use samchi_core::source_wire::{
    known_item_type, Item, ITEM_AGENT_MESSAGE, ITEM_COMMAND_EXECUTION, ITEM_FILE_CHANGE, ITEM_PLAN,
    ITEM_USER_MESSAGE, ITEM_WEB_SEARCH,
};
use serde_json::Value;
use std::collections::HashMap;

pub const ITEM_STATUS_IN_PROGRESS: &str = "inProgress";
pub const ITEM_STATUS_COMPLETED: &str = "completed";
pub const ITEM_STATUS_FAILED: &str = "failed";

const USER_ITEM_ID: &str = "user-message";
const AGENT_ITEM_ID: &str = "agent-message";
const PLAN_ITEM_ID: &str = "plan";

/// Mapping stopped because the adapter would have to emit an excluded-fatal type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExcludedFatal {
    pub item_type: String,
}

impl std::fmt::Display for ExcludedFatal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "excluded-fatal item type {}", self.item_type)
    }
}

/// Accumulates one turn's public items.
#[derive(Debug, Clone)]
pub struct Mapper {
    items: Vec<Item>,
    tools: HashMap<String, usize>,
}

impl Mapper {
    pub fn new(user_text: &str) -> Self {
        Self {
            items: vec![Item {
                id: USER_ITEM_ID.to_string(),
                item_type: ITEM_USER_MESSAGE.to_string(),
                text: user_text.to_string(),
                status: ITEM_STATUS_COMPLETED.to_string(),
            }],
            tools: HashMap::new(),
        }
    }

    pub fn items(&self) -> &[Item] {
        &self.items
    }

    /// Apply one `session/update`. Unknown kinds are skipped. Thought chunks
    /// are discarded. `in_progress` / omitted tool status never completes.
    pub fn apply(&mut self, update: &SessionUpdate) -> Result<Vec<Item>, ExcludedFatal> {
        match update {
            SessionUpdate::AgentThoughtChunk(_) => Ok(Vec::new()),
            SessionUpdate::AgentMessageChunk(ContentChunk {
                content: ContentBlock::Text(text),
                ..
            }) => {
                self.upsert_agent(&text.text);
                Ok(vec![self.item_by_id(AGENT_ITEM_ID).cloned().unwrap()])
            }
            SessionUpdate::Plan(plan) => {
                self.upsert_plan(plan);
                Ok(vec![self.item_by_id(PLAN_ITEM_ID).cloned().unwrap()])
            }
            SessionUpdate::ToolCall(call) => self.apply_tool_call(call),
            SessionUpdate::ToolCallUpdate(update) => self.apply_tool_update(update),
            _ => Ok(Vec::new()),
        }
    }

    /// Mark the accumulated agent message completed when `session/prompt` returns.
    pub fn finish_agent_message(&mut self) -> Option<Item> {
        let idx = self.items.iter().position(|i| i.id == AGENT_ITEM_ID)?;
        self.items[idx].status = ITEM_STATUS_COMPLETED.to_string();
        Some(self.items[idx].clone())
    }

    fn upsert_agent(&mut self, chunk: &str) {
        if let Some(idx) = self.items.iter().position(|i| i.id == AGENT_ITEM_ID) {
            self.items[idx].text.push_str(chunk);
            return;
        }
        self.items.push(Item {
            id: AGENT_ITEM_ID.to_string(),
            item_type: ITEM_AGENT_MESSAGE.to_string(),
            text: chunk.to_string(),
            status: ITEM_STATUS_IN_PROGRESS.to_string(),
        });
    }

    fn upsert_plan(&mut self, plan: &Plan) {
        let text = plan
            .entries
            .iter()
            .map(|e| e.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if let Some(idx) = self.items.iter().position(|i| i.id == PLAN_ITEM_ID) {
            self.items[idx].text = text;
            return;
        }
        self.items.push(Item {
            id: PLAN_ITEM_ID.to_string(),
            item_type: ITEM_PLAN.to_string(),
            text,
            status: ITEM_STATUS_IN_PROGRESS.to_string(),
        });
    }

    fn apply_tool_call(&mut self, call: &ToolCall) -> Result<Vec<Item>, ExcludedFatal> {
        let Some(item_type) = map_kind(call.kind, &call.title, call.raw_input.as_ref()) else {
            return Ok(Vec::new());
        };
        reject_fatal(&item_type)?;
        let id = sanitize_id(&call.tool_call_id.to_string());
        let status = item_status(Some(call.status));
        let item = Item {
            id: id.clone(),
            item_type,
            text: call.title.clone(),
            status,
        };
        Ok(vec![self.store_tool(id, item)])
    }

    fn apply_tool_update(&mut self, update: &ToolCallUpdate) -> Result<Vec<Item>, ExcludedFatal> {
        let id = sanitize_id(&update.tool_call_id.to_string());
        let kind = update.fields.kind.unwrap_or(ToolKind::Other);
        let title = update.fields.title.clone().unwrap_or_default();
        let raw = None;
        if let Some(idx) = self.tools.get(&id).copied() {
            if let Some(status) = update.fields.status {
                self.items[idx].status = item_status(Some(status));
            }
            if let Some(title) = update.fields.title.as_ref() {
                self.items[idx].text = title.clone();
            }
            if let Some(kind) = update.fields.kind {
                if let Some(item_type) = map_kind(kind, &self.items[idx].text, raw) {
                    reject_fatal(&item_type)?;
                    self.items[idx].item_type = item_type;
                }
            }
            return Ok(vec![self.items[idx].clone()]);
        }
        let Some(item_type) = map_kind(kind, &title, raw) else {
            return Ok(Vec::new());
        };
        reject_fatal(&item_type)?;
        let item = Item {
            id: id.clone(),
            item_type,
            text: title,
            status: item_status(update.fields.status),
        };
        Ok(vec![self.store_tool(id, item)])
    }

    fn store_tool(&mut self, id: String, item: Item) -> Item {
        if let Some(idx) = self.tools.get(&id).copied() {
            self.items[idx] = item;
            return self.items[idx].clone();
        }
        let idx = self.items.len();
        self.tools.insert(id, idx);
        self.items.push(item);
        self.items[idx].clone()
    }

    fn item_by_id(&self, id: &str) -> Option<&Item> {
        self.items.iter().find(|i| i.id == id)
    }
}

fn reject_fatal(item_type: &str) -> Result<(), ExcludedFatal> {
    let (_public, fatal) = known_item_type(item_type);
    if fatal {
        return Err(ExcludedFatal {
            item_type: item_type.to_string(),
        });
    }
    Ok(())
}

fn map_kind(kind: ToolKind, title: &str, raw_input: Option<&Value>) -> Option<String> {
    match kind {
        ToolKind::Edit => Some(ITEM_FILE_CHANGE.to_string()),
        ToolKind::Execute => Some(ITEM_COMMAND_EXECUTION.to_string()),
        ToolKind::Other if is_shell(title, raw_input) => Some(ITEM_COMMAND_EXECUTION.to_string()),
        ToolKind::Fetch | ToolKind::Search if is_web(title, raw_input) => {
            Some(ITEM_WEB_SEARCH.to_string())
        }
        ToolKind::Delete | ToolKind::Move => None,
        _ => None,
    }
}

fn is_shell(title: &str, raw_input: Option<&Value>) -> bool {
    let t = title.to_ascii_lowercase();
    t.contains("shell")
        || t.contains("bash")
        || t.contains("zsh")
        || raw_input.is_some_and(|v| v.get("command").is_some())
}

fn is_web(title: &str, raw_input: Option<&Value>) -> bool {
    let raw = raw_input.map(|v| v.to_string()).unwrap_or_default();
    let blob = format!("{title}{raw}");
    blob.contains("http://") || blob.contains("https://") || blob.contains("www.")
}

fn item_status(status: Option<ToolCallStatus>) -> String {
    match status {
        Some(ToolCallStatus::Completed) => ITEM_STATUS_COMPLETED.to_string(),
        Some(ToolCallStatus::Failed) => ITEM_STATUS_FAILED.to_string(),
        Some(ToolCallStatus::Pending | ToolCallStatus::InProgress) | None => {
            ITEM_STATUS_IN_PROGRESS.to_string()
        }
        _ => ITEM_STATUS_IN_PROGRESS.to_string(),
    }
}

pub(crate) fn sanitize_id(raw: &str) -> String {
    let mut s: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() {
        s.push_str("id");
    }
    if s.len() > 128 {
        s.truncate(128);
    }
    s
}

/// Refuse to emit excluded-fatal types even if a caller asked.
pub fn emit_item_type(item_type: &str) -> Result<(), ExcludedFatal> {
    reject_fatal(item_type)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1::{
        PlanEntry, PlanEntryPriority, PlanEntryStatus, TextContent, ToolCallUpdateFields,
    };

    #[test]
    fn in_progress_does_not_complete() {
        let mut m = Mapper::new("hi");
        m.apply(&SessionUpdate::ToolCall(
            ToolCall::new("t1", "edit file")
                .kind(ToolKind::Edit)
                .status(ToolCallStatus::InProgress),
        ))
        .unwrap();
        let item = m.items().iter().find(|i| i.id == "t1").unwrap();
        assert_eq!(item.item_type, ITEM_FILE_CHANGE);
        assert_eq!(item.status, ITEM_STATUS_IN_PROGRESS);

        m.apply(&SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "t1",
            ToolCallUpdateFields::new(),
        )))
        .unwrap();
        let item = m.items().iter().find(|i| i.id == "t1").unwrap();
        assert_eq!(item.status, ITEM_STATUS_IN_PROGRESS);

        m.apply(&SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "t1",
            ToolCallUpdateFields::new().status(ToolCallStatus::Completed),
        )))
        .unwrap();
        let item = m.items().iter().find(|i| i.id == "t1").unwrap();
        assert_eq!(item.status, ITEM_STATUS_COMPLETED);
    }

    #[test]
    fn first_terminal_tool_call_is_completed() {
        let mut m = Mapper::new("hi");
        m.apply(&SessionUpdate::ToolCall(
            ToolCall::new("t1", "done")
                .kind(ToolKind::Edit)
                .status(ToolCallStatus::Completed),
        ))
        .unwrap();
        assert_eq!(
            m.items().iter().find(|i| i.id == "t1").unwrap().status,
            ITEM_STATUS_COMPLETED
        );
    }

    #[test]
    fn unknown_kind_skipped_and_thoughts_discarded() {
        let mut m = Mapper::new("hi");
        m.apply(&SessionUpdate::ToolCall(
            ToolCall::new("r1", "read").kind(ToolKind::Read),
        ))
        .unwrap();
        m.apply(&SessionUpdate::AgentThoughtChunk(ContentChunk::new(
            ContentBlock::Text(TextContent::new("secret")),
        )))
        .unwrap();
        assert!(m.items().iter().all(|i| i.id != "r1"));
        assert!(!m.items().iter().any(|i| i.text.contains("secret")));
    }

    #[test]
    fn shell_other_and_web_fetch() {
        let mut m = Mapper::new("hi");
        m.apply(&SessionUpdate::ToolCall(
            ToolCall::new("sh", "run bash").kind(ToolKind::Other),
        ))
        .unwrap();
        assert_eq!(
            m.items().iter().find(|i| i.id == "sh").unwrap().item_type,
            ITEM_COMMAND_EXECUTION
        );
        m.apply(&SessionUpdate::ToolCall(
            ToolCall::new("w", "https://example.com").kind(ToolKind::Fetch),
        ))
        .unwrap();
        assert_eq!(
            m.items().iter().find(|i| i.id == "w").unwrap().item_type,
            ITEM_WEB_SEARCH
        );
    }

    #[test]
    fn same_tool_id_is_one_item() {
        let mut m = Mapper::new("hi");
        m.apply(&SessionUpdate::ToolCall(
            ToolCall::new("t1", "edit").kind(ToolKind::Edit),
        ))
        .unwrap();
        m.apply(&SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "t1",
            ToolCallUpdateFields::new().status(ToolCallStatus::Completed),
        )))
        .unwrap();
        assert_eq!(m.items().iter().filter(|i| i.id == "t1").count(), 1);
    }

    #[test]
    fn agent_message_completes_on_finish() {
        let mut m = Mapper::new("hi");
        m.apply(&SessionUpdate::AgentMessageChunk(ContentChunk::new(
            ContentBlock::Text(TextContent::new("hel")),
        )))
        .unwrap();
        m.apply(&SessionUpdate::AgentMessageChunk(ContentChunk::new(
            ContentBlock::Text(TextContent::new("lo")),
        )))
        .unwrap();
        let live = m.items().iter().find(|i| i.id == AGENT_ITEM_ID).unwrap();
        assert_eq!(live.text, "hello");
        assert_eq!(live.status, ITEM_STATUS_IN_PROGRESS);
        m.finish_agent_message();
        assert_eq!(
            m.items()
                .iter()
                .find(|i| i.id == AGENT_ITEM_ID)
                .unwrap()
                .status,
            ITEM_STATUS_COMPLETED
        );
    }

    #[test]
    fn plan_and_user_message() {
        let m = Mapper::new("prompt");
        assert_eq!(m.items()[0].item_type, ITEM_USER_MESSAGE);
        assert_eq!(m.items()[0].status, ITEM_STATUS_COMPLETED);
        let mut m = m;
        m.apply(&SessionUpdate::Plan(Plan::new(vec![PlanEntry::new(
            "step",
            PlanEntryPriority::Medium,
            PlanEntryStatus::Pending,
        )])))
        .unwrap();
        assert_eq!(
            m.items()
                .iter()
                .find(|i| i.id == PLAN_ITEM_ID)
                .unwrap()
                .text,
            "step"
        );
    }

    #[test]
    fn excluded_fatal_types() {
        assert!(emit_item_type("subAgentActivity").is_err());
        assert!(emit_item_type("collabAgentToolCall").is_err());
        assert!(emit_item_type(ITEM_FILE_CHANGE).is_ok());
    }
}
