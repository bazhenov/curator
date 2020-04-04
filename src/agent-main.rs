extern crate curator;
use curator::agent::{Executions, SseClient};
use curator::errors::*;
use curator::protocol;
use serde_json;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use termion::clear;
use tokio;
use tokio::process::Command;
use tokio::time::delay_for;

#[tokio::main]
async fn main() -> Result<()> {
    let agent = protocol::agent::Agent::new(
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

    let executions = Arc::new(Mutex::new(executions));

    tokio::spawn(monitor_executions(executions.clone()));
    tokio::spawn(report_executions_back(executions.clone()));

    while let Some((name, event)) = client.next_event().await? {
        match name {
            Some(s) if s == "run-task" => {
                match serde_json::from_str::<protocol::agent::RunTask>(&event) {
                    Ok(event) => {
                        let mut executions = executions.lock().unwrap();
                        if !executions.run(&event.task_id, event.execution) {
                            eprintln!("Task {} not found", event.task_id);
                        }
                    }
                    Err(e) => {
                        eprintln!("Unable to interpret {} event: {}. {}", s, event, e);
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

async fn monitor_executions(executions: Arc<Mutex<Executions>>) {
    loop {
        delay_for(Duration::from_millis(1000)).await;

        println!("{}", clear::All);
        let executions = executions.lock().unwrap();
        for e in executions.list() {
            let e = e.lock().unwrap();
            println!(" - {} - {:?}", e.id, e.status);
        }
    }
}

async fn report_executions_back(executions: Arc<Mutex<Executions>>) {
    loop {
        delay_for(Duration::from_millis(100)).await;

        let mut executions = executions.lock().unwrap();
        for e in executions.list_mut() {
            let mut e = e.lock().unwrap();
            e.stdout = String::new();
        }
    }
}
