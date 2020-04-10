extern crate clap;
extern crate curator;
use clap::App;
use curator::{
    agent::TaskDef,
    agent::{discover, AgentLoop},
    prelude::*,
};
use std::{collections::HashSet, process::exit, time::Duration};
use tokio::{sync::oneshot, time::delay_for};

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
    let matches = App::new("Curator Agent")
        .version("0.1.0")
        .author("Denis Bazhenov <dotsid@gmail.com>")
        .about("Agent application for Curator server")
        .arg_from_usage("-h, --host=<host> 'Curator server host'")
        .get_matches();

    let host = matches.value_of("host").unwrap();

    let mut tasks: Option<HashSet<TaskDef>> = None;
    let mut close_handle: Option<oneshot::Sender<()>> = None;

    loop {
        let proposed_tasks = discover().await?;
        let has_changes = match tasks {
            Some(ref tasks) => {
                let mut difference = proposed_tasks.symmetric_difference(&tasks).peekable();
                difference.peek().is_some()
            }
            None => true,
        };

        if has_changes {
            trace!("Reloading tasks..");
            if proposed_tasks.is_empty() {
                warn!("No tasks has been discovered");
            }

            if let Some(channel) = close_handle.take() {
                channel.send(()).expect("Agent has been already closed");
            }

            let mut executions = AgentLoop::new("my", "single", &host)?;
            for t in &proposed_tasks {
                executions.register_task(t.clone());
            }

            tasks = Some(proposed_tasks);

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
