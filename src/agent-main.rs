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
    if report_errors(run().await) {
        exit(0);
    } else {
        exit(1);
    }
}

async fn run() -> Result<()> {
    let mut tasks = HashSet::new();

    let mut loop_close_handle: Option<oneshot::Sender<()>> = None;

    loop {
        let proposed_tasks = discover().await?;
        let has_changes = {
            let mut difference = proposed_tasks.symmetric_difference(&tasks).peekable();
            difference.peek().is_some()
        };
        if has_changes {
            println!("Reloading tasks..");
            tasks = proposed_tasks;

            if let Some(channel) = loop_close_handle.take() {
                channel
                    .send(())
                    .map_err(|_| "Agent has been already closed")?;
            }

            let mut executions = AgentLoop::new("my", "single");
            for t in &tasks {
                executions.register_task(t.clone());
            }

            loop_close_handle = executions.close_channel();

            tokio::spawn(async move { report_errors(executions.run().await) });
        }
        delay_for(Duration::from_secs(5)).await;
    }
}

fn report_errors<T>(result: Result<T>) -> bool {
    match result {
        Err(e) => {
            eprintln!("{}", e.display_chain());
            false
        }
        Ok(_) => true,
    }
}
