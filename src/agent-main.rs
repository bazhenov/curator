extern crate clap;
extern crate ctrlc;
extern crate curator;

use bollard::Docker;
use clap::{App, AppSettings, ArgMatches, SubCommand};
use curator::{
    agent::AgentLoop,
    docker::{discover_tasks, list_running_containers, start_discovery, TaskSet},
    prelude::*,
};
use futures::stream::StreamExt;
use std::{
    collections::hash_map::DefaultHasher,
    fmt,
    hash::{Hash, Hasher},
    process::exit,
};
use termion::{
    color::{Fg, Yellow},
    style::Reset,
};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    ctrlc::set_handler(move || {
        println!("Got Ctrl-C! Shuting down...");
        exit(1);
    })?;

    let matches = App::new("Curator Agent")
        .version("0.1.0")
        .author("Denis Bazhenov <dotsid@gmail.com>")
        .setting(AppSettings::SubcommandRequired)
        .subcommand(
            SubCommand::with_name("run")
                .about("Agent application for Curator server")
                .arg_from_usage("-h, --host=<host> 'Curator server host'")
                .arg_from_usage("-n, --name=<name> 'Agent name'"),
        )
        .subcommand(
            SubCommand::with_name("tasks")
                .about("Run discovery for a given toolchains")
                .arg_from_usage(
                    "<toolchains> -t, --toolchain=<toolchain>... 'Toolchain image name'",
                )
                .arg_from_usage(
                    "-c, --container=[container] 'Container id or name to run discovery on'",
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
    let host = opts.value_of("host").context("No host provided")?;
    let agent_name = opts.value_of("name").context("No name provided")?;

    let mut tasks_hash: Option<_> = None;
    let mut _agent_loop: Option<_> = None;

    let docker = Docker::connect_with_local_defaults()?;
    let toolchains = vec!["bazhenov.me/curator/toolchain-example:dev".into()];
    let (mut stream, loop_handle) = start_discovery(docker, toolchains);

    while let Some(tasks) = stream.next().await {
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
            _agent_loop = Some(AgentLoop::run(&host, agent_name, tasks)?);
        }
    }

    loop_handle.await?
}

async fn tasks_command(opts: &ArgMatches<'_>) -> Result<()> {
    let docker = Docker::connect_with_local_defaults()?;
    let toolchains = opts
        .values_of("toolchains")
        .context("No toolchains were given")?
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    let container_id = opts.value_of("container");

    let task_set = if let Some(container_id) = container_id {
        discover_tasks(&docker, &toolchains, &[container_id]).await?
    } else {
        let container_ids = list_running_containers(&docker).await?;
        discover_tasks(&docker, &toolchains, &container_ids).await?
    };
    print!("{}", TaskSetDisplay(task_set));

    Ok(())
}

fn hash_values<T: Hash>(values: &[T]) -> u64 {
    let mut hasher = DefaultHasher::new();
    values.hash(&mut hasher);
    hasher.finish()
}

type FmtResult<T> = std::result::Result<T, fmt::Error>;

struct TaskSetDisplay(TaskSet);

impl fmt::Display for TaskSetDisplay {
    fn fmt(&self, f: &mut fmt::Formatter) -> FmtResult<()> {
        if !self.0.is_empty() {
            for task in &self.0 {
                writeln!(f, "  - {:?}", task)?;
            }
        } else {
            writeln!(
                f,
                "{color}No tasks found{reset}",
                color = Fg(Yellow),
                reset = Reset,
            )?;
        }
        Ok(())
    }
}
