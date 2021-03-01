extern crate clap;
extern crate ctrlc;
extern crate curator;

use clap::App;
use curator::{
    agent::{discover, AgentLoop},
    prelude::*,
};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    process::exit,
    time::Duration,
};
use tokio::time::sleep;

#[tokio::main]
async fn main() {
    env_logger::init();

    ctrlc::set_handler(move || {
        println!("Got Ctrl-C! Shuting down...");
        exit(1);
    })
    .expect("Unable to install Ctrl-C handler");

    match run().await {
        Err(e) => {
            log_errors(&e);
            exit(1);
        }
        Ok(_) => exit(0),
    }
}

async fn run() -> Result<()> {
    let matches = App::new("Curator Agent")
        .version("0.1.0")
        .author("Denis Bazhenov <dotsid@gmail.com>")
        .about("Agent application for Curator server")
        .arg_from_usage("-h, --host=<host> 'Curator server host'")
        .arg_from_usage("-n, --name=<name> 'Agent name'")
        .get_matches();

    let host = matches.value_of("host").unwrap();

    let agent_name = matches.value_of("name").unwrap();

    let mut tasks_hash: Option<_> = None;
    let mut _agent_loop: Option<_> = None;

    loop {
        let tasks = discover("./").await?;

        let hash = Some(hash_values(&tasks));

        let has_changes = tasks_hash != hash;
        tasks_hash = hash;

        if has_changes {
            trace!("Reloading tasks..");
            if tasks.is_empty() {
                warn!("No tasks has been discovered");
            }

            // Need to save loop reference until tasks will be changed, otherwise
            // loop will be closed immediately
            _agent_loop = Some(AgentLoop::run(&host, agent_name, tasks));
        }
        sleep(Duration::from_secs(5)).await;
    }
}

fn hash_values<T: Hash>(values: &[T]) -> u64 {
    let mut hasher = DefaultHasher::new();
    values.iter().for_each(|t| t.hash(&mut hasher));
    hasher.finish()
}
