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
        .arg_from_usage("-a, --application=<application> 'Application name of an agent'")
        .arg_from_usage("-i, --instance=<instance> 'Instance name of an agent'")
        .get_matches();

    let host = matches.value_of("host").unwrap();

    let application_name = matches.value_of("application").unwrap();
    let instance_name = matches.value_of("instance").unwrap();

    let mut tasks: Option<HashSet<TaskDef>> = None;

    let mut _current_loop: Option<_> = None;

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

            let agent_tasks = proposed_tasks.iter().cloned().collect::<Vec<_>>();
            // Need to save loop reference until tasks will be changed
            _current_loop = Some(AgentLoop::run(
                &host,
                application_name,
                instance_name,
                agent_tasks,
            ));

            tasks = Some(proposed_tasks);
        }
        delay_for(Duration::from_secs(5)).await;
    }
}
