extern crate curator;
use curator::client::{Executions, SseClient};
use curator::errors::*;
use curator::protocol;
use serde_json;
use tokio;
use tokio::process::Command;

#[tokio::main]
async fn main() -> Result<()> {
    let agent = protocol::Agent::new(
        "my",
        "single",
        vec![
            protocol::Task {
                id: "w".to_string(),
            },
            protocol::Task {
                id: "uptime".to_string(),
            },
        ],
    );

    let mut client = SseClient::connect("http://localhost:8080/events", agent).await?;

    let mut executions = Executions::new();
    executions.register_task("date", || Command::new("date"));
    executions.register_task("uptime", || Command::new("uptime"));
    executions.register_task("w", || Command::new("w"));

    while let Some((name, event)) = client.next_event().await? {
        println!("TASK: {:?}", name);
        match name {
            Some(s) if s == "run-task" => match serde_json::from_str::<protocol::RunTask>(&event) {
                Ok(event) => {
                    if !executions.run(&event.task_id, event.execution) {
                        eprintln!("Task {} not found", event.task_id);
                    }
                }
                Err(e) => {
                    eprintln!("Unablet to interprent {} event: {}. {}", s, event, e);
                }
            },
            Some(s) if s == "stop-task" => {
                unimplemented!();
            }
            _ => {}
        }
    }
    Ok(())
}
