//! Stdio test double for `grok agent stdio`. Not a product command.

use samchi_adapter_grok::run_fake_agent;

#[tokio::main]
async fn main() -> agent_client_protocol::Result<()> {
    run_fake_agent(agent_client_protocol::Stdio::new()).await
}
