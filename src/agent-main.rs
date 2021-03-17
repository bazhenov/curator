extern crate clap;
extern crate ctrlc;
extern crate curator;

use bollard::Docker;
use clap::{App, AppSettings, ArgMatches, SubCommand};
use curator::{
    agent::{build_task_set, discover, AgentLoop, TaskSet},
    prelude::*,
};
use std::{
    collections::hash_map::DefaultHasher,
    fmt,
    hash::{Hash, Hasher},
    process::exit,
    time::Duration,
};
use termion::{
    color::{Fg, Green, Yellow},
    style::{Bold, Reset},
};
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    ctrlc::set_handler(move || {
        println!("Got Ctrl-C! Shuting down...");
        exit(1);
    })?;

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
        .setting(AppSettings::SubcommandRequired)
        .subcommand(
            SubCommand::with_name("run")
                .about("Agent application for Curator server")
                .arg_from_usage("<host> -h, --host=<host> 'Curator server host'")
                .arg_from_usage("<name> -n, --name=<name> 'Agent name'"),
        )
        .subcommand(
            SubCommand::with_name("tasks")
                .about("Run discovery for a given toolchains")
                .arg_from_usage(
                    "<toolchains> -t, --toolchain=<toolchain>... 'Toolchain image name'",
                ),
        )
        .get_matches();

    match matches.subcommand() {
        ("run", Some(opts)) => run_command(opts).await,
        ("tasks", Some(opts)) => tasks_command(opts).await,
        _ => unimplemented!(),
    }
}

async fn run_command(opts: &ArgMatches<'_>) -> Result<()> {
    let host = opts.value_of("host").unwrap();
    let agent_name = opts.value_of("name").unwrap();

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

async fn tasks_command(opts: &ArgMatches<'_>) -> Result<()> {
    let docker = Docker::connect_with_local_defaults()?;
    let toolchains = opts
        .values_of("toolchains")
        .context("No toolchains were given")?
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();

    let task_set = build_task_set(&docker, &toolchains).await?;
    print!("{}", TaskSetDisplay(task_set));
    Ok(())
}

fn hash_values<T: Hash>(values: &[T]) -> u64 {
    let mut hasher = DefaultHasher::new();
    values.iter().for_each(|t| t.hash(&mut hasher));
    hasher.finish()
}

struct TaskSetDisplay(TaskSet);

impl fmt::Display for TaskSetDisplay {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::result::Result<(), fmt::Error> {
        if !self.0.is_empty() {
            writeln!(f, "Running containers:")?;
            for (container_id, tasks) in &self.0 {
                writeln!(
                    f,
                    " + {color}{style}{id}{reset}",
                    id = container_id,
                    style = Bold,
                    color = Fg(Green),
                    reset = Reset,
                )?;
                if !tasks.is_empty() {
                    for task in tasks {
                        writeln!(f, "    - {:?}", task)?;
                    }
                } else {
                    writeln!(
                        f,
                        "      {color}No tasks found{reset}",
                        color = Fg(Yellow),
                        reset = Reset,
                    )?;
                }
            }
        } else {
            writeln!(
                f,
                "{color}No containers found{reset}",
                color = Fg(Yellow),
                reset = Reset,
            )?;
        }
        Ok(())
    }
}
