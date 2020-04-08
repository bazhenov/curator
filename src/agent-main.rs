extern crate curator;
use curator::agent::{CuratorAgent, TaskDef};
use curator::prelude::*;

#[tokio::main]
async fn main() -> Result<()> {
    let mut executions = CuratorAgent::new("my", "single");
    executions.register_task(TaskDef::new("date", "date"));
    executions.register_task(TaskDef::new("uptime", "uptime"));
    executions.register_task(TaskDef::new("w", "w"));
    executions.register_task(TaskDef::new_with_args("sleep.5", "sleep", vec!["5".into()]));
    executions.register_task(TaskDef::new("invalid", "not-existent-command"));

    executions.run().await
}
