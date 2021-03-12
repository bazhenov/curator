use bollard::Docker;
use curator::{
    agent::TaskDef,
    docker::{list_running_containers, run_toolchain_discovery},
    prelude::*,
};
use futures::stream::{Stream, StreamExt};
use std::marker::Unpin;
use termion::{clear, color, style};
use tokio::time::{self, Duration};
use tokio_stream::wrappers::IntervalStream;

type TaskSet = Vec<(String, Vec<TaskDef>)>;
const TOOLCHAIN: &str = "bazhenov.me/curator/toolchain-example:dev";

#[tokio::main]
async fn main() -> Result<()> {
    let docker = Docker::connect_with_local_defaults()?;

    let mut discovery = start_discovery_polling_loop(&docker);
    while let Some(task_set) = discovery.next().await {
        let task_set = task_set?;
        println!("{}", clear::All);
        if !task_set.is_empty() {
            println!("Running containers:");
            for (container_id, tasks) in task_set {
                println!(
                    " + {green}{bold}{id}{reset}",
                    id = container_id,
                    bold = style::Bold,
                    green = color::Fg(color::Green),
                    reset = style::Reset,
                );
                if !tasks.is_empty() {
                    for task in tasks {
                        println!("    - {:?}", task);
                    }
                } else {
                    println!(
                        "      {yellow}No tasks found{reset}",
                        yellow = color::Fg(color::Yellow),
                        reset = style::Reset,
                    )
                }
            }
        } else {
            println!(
                "{yellow}No containers found{reset}",
                yellow = color::Fg(color::Yellow),
                reset = style::Reset,
            )
        }
    }
    Ok(())
}

async fn build_task_set(docker: &Docker) -> Result<TaskSet> {
    let mut result = vec![];
    for id in list_running_containers(&docker).await? {
        let tasks = run_toolchain_discovery(&docker, &id, TOOLCHAIN).await?;
        let tuple = (id, tasks);
        result.push(tuple);
    }
    Ok(result)
}

fn start_discovery_polling_loop<'a>(
    docker: &'a Docker,
) -> impl Stream<Item = Result<TaskSet>> + Unpin + 'a {
    let interval = time::interval(Duration::from_secs(1));
    let stream = IntervalStream::new(interval);

    let stream = stream.then(move |_| async move { build_task_set(docker).await });
    Box::pin(stream)
}
