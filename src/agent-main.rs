extern crate curator;
use curator::agent::{discover, AgentLoop};
use curator::prelude::*;
use std::collections::HashSet;
use std::process::exit;
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::time::delay_for;

#[tokio::main]
async fn main() {
    env_logger::init();
    match run().await {
        Err(e) => {
            report_errors(e);
            exit(1);
        }
        Ok(_) => exit(0),
    };
}

async fn run() -> Result<()> {
    let mut tasks = HashSet::new();

    let mut close_handle: Option<oneshot::Sender<()>> = None;

    loop {
        let proposed_tasks = discover().await?;
        let has_changes = {
            let mut difference = proposed_tasks.symmetric_difference(&tasks).peekable();
            difference.peek().is_some()
        };

        if has_changes {
            trace!("Reloading tasks..");
            tasks = proposed_tasks;

            if let Some(channel) = close_handle.take() {
                channel.send(()).expect("Agent has been already closed");
            }

            let mut executions = AgentLoop::new("my", "single");
            for t in &tasks {
                executions.register_task(t.clone());
            }

            close_handle = executions.close_channel();

            tokio::spawn(async move {
                if let Err(e) = executions.run().await {
                    report_errors(e);
                }
            });
        }
        delay_for(Duration::from_secs(5)).await;
    }
}
