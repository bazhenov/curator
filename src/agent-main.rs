extern crate clap;
extern crate curator;
use clap::App;
use curator::{
    agent::TaskDef,
    agent::{discover, AgentLoop},
    prelude::*,
};
use std::{collections::HashSet, process::exit, time::Duration};
use tokio::time::delay_for;

#[tokio::main]
async fn main() {
    env_logger::init();

    match run().await {
        Err(e) => {
            log_errors(&e);
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

            let mut agent_loop = AgentLoop::new("my", "single", &host);
            for t in &proposed_tasks {
                agent_loop.register_task(t.clone());
            }

            tasks = Some(proposed_tasks);

            // Seems like at the moment when agent is recreated old one is closed because of
            // transport link is destroyed. It doesn't seems like graceful closing strategy.
            tokio::spawn(async move {
                if let Err(e) = agent_loop.run().await {
                    log_errors(&e);
                }
            });
        }
        delay_for(Duration::from_secs(5)).await;
    }
}
