extern crate curator;
use curator::client::SseClient;
use curator::errors::*;
use std::process::Stdio;
use tokio;
use tokio::io::BufReader;
use tokio::prelude::*;
use tokio::process::Command;
use tokio::stream::StreamExt;

#[tokio::main]
async fn main() -> Result<()> {
    let mut client = SseClient::connect("http://localhost:8080/events").await?;

    while let Some((name, _)) = client.next_event().await? {
        println!("TASK: {:?}", name);
        match name {
            Some(s) if s == "run-task" => {
                println!("RUN TASK");
                tokio::spawn(async move {
                    run_command().await;
                });
            }
            Some(s) if s == "stop-task" => {
                unimplemented!();
            }
            _ => {}
        }
    }
    Ok(())
}

async fn run_command() {
    let child = Command::new("date")
        .stdout(Stdio::piped())
        .spawn()
        .expect("Unable to spawn child");

    let stdout = child.stdout.unwrap();
    let mut reader = BufReader::new(stdout).lines();

    while let Some(line) = reader.next().await {
        println!("Line: {}", line.expect("No line from process"));
    }
}
