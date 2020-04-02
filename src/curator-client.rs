extern crate curator;
use curator::client::{SseClient, Executions};
use curator::errors::*;
use curator::protocol::RunTask;
use tokio;
use tokio::process::Command;
use serde_json;

#[tokio::main]
async fn main() -> Result<()> {
    let mut client = SseClient::connect("http://localhost:8080/events").await?;

    let mut executions = Executions::new();
    executions.register_task("date", || Command::new("date"));
    executions.register_task("uptime", || Command::new("uptime"));
    executions.register_task("w", || Command::new("w"));

    while let Some((name, event)) = client.next_event().await? {
        println!("TASK: {:?}", name);
        match name {
            Some(s) if s == "run-task" => {
                match serde_json::from_str::<RunTask>(&event) {
                    Ok(event) => {
                        if !executions.run(&event.task_id, event.execution) {
                            eprintln!("Task {} not found", event.task_id);
                        }
                    },
                    Err(e) => {
                        eprintln!("Unablet to interprent {} event: {}. {}", s, event, e);
                    }
                }
            }
            Some(s) if s == "stop-task" => {
                unimplemented!();
            }
            _ => {}
        }
    }
    Ok(())
}
