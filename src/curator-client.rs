extern crate curator;
use curator::client::{SseClient, Task};
use curator::errors::*;
use curator::protocol::RunTask;
use std::process::Stdio;
use std::collections::HashMap;
use tokio;
use tokio::io::BufReader;
use tokio::prelude::*;
use tokio::process::Command;
use tokio::stream::StreamExt;
use serde_json;

#[tokio::main]
async fn main() -> Result<()> {
    let mut client = SseClient::connect("http://localhost:8080/events").await?;

    let tasks = vec![
        Task {
            id: String::from("date"),
            command: || Command::new("date")
        },
        Task {
            id: String::from("uptime"),
            command: || Command::new("uptime")
        },
        Task {
            id: String::from("w"),
            command: || Command::new("w")
        },
    ];

    let tasks = tasks.into_iter()
        .map(|i| (i.id.clone(), i))
        .collect::<HashMap<_, _>>();

    while let Some((name, event)) = client.next_event().await? {
        println!("TASK: {:?}", name);
        match name {
            Some(s) if s == "run-task" => {
                match serde_json::from_str::<RunTask>(&event) {
                    Ok(event) => {
                        if let Some(task) = tasks.get(&event.task_id) {
                            let command = (task.command)();
                            tokio::spawn(async move {
                                run_command(command).await;
                            });
                        } else {
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

async fn run_command(mut cmd: Command) {
    let child = cmd.stdout(Stdio::piped())
        .spawn()
        .expect("Unable to spawn child");

    let stdout = child.stdout.unwrap();
    let mut reader = BufReader::new(stdout).lines();

    while let Some(line) = reader.next().await {
        println!("Line: {}", line.expect("No line from process"));
    }
}
