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
    executions.register_task("sleep", || {
        let mut cmd = Command::new("sleep");
        cmd.arg("5");
        cmd
    });
    executions.register_task("invalid", || Command::new("not-existent"));

    executions.run().await
}
