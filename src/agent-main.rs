extern crate curator;
use curator::agent::CuratorAgent;
use curator::prelude::*;
use tokio::process::Command;

#[tokio::main]
async fn main() -> Result<()> {
    let mut executions = CuratorAgent::new("my", "single");
    executions.register_task("date", || Command::new("date"));
    executions.register_task("uptime", || Command::new("uptime"));
    executions.register_task("w", || Command::new("w"));

    executions.agent_loop().await
}
