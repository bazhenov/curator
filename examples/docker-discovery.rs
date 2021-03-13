use bollard::Docker;
use curator::{
    agent::TaskDef,
    docker::{list_running_containers, run_toolchain_discovery},
    prelude::*,
};
use futures::stream::{Stream, StreamExt};
use log::warn;
use std::{fmt, marker::Unpin};
use termion::{
    clear,
    color::{Fg, Green, Yellow},
    style::{Bold, Reset},
};
use tokio::time::{interval, Duration};
use tokio_stream::wrappers::IntervalStream;

const TOOLCHAIN: &str = "bazhenov.me/curator/toolchain-example:dev";

#[derive(Error, Debug)]
enum Errors {
    #[error("Discovery failed on container: {0}")]
    DiscoveryFailed(String),
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let docker = Docker::connect_with_local_defaults()?;

    let mut task_set_stream = start_discovery(&docker);
    while let Some(task_set) = task_set_stream.next().await {
        print!("{}{}", clear::All, task_set?);
    }
    Ok(())
}

/// Task set is the set of all the tasks found in the system
/// on a given round of discovery
struct TaskSet(Vec<(String, Vec<TaskDef>)>);

impl TaskSet {
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<'a> IntoIterator for &'a TaskSet {
    type Item = &'a (String, Vec<TaskDef>);
    type IntoIter = std::slice::Iter<'a, (String, Vec<TaskDef>)>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl fmt::Display for TaskSet {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::result::Result<(), fmt::Error> {
        if !self.is_empty() {
            writeln!(f, "Running containers:")?;
            for (container_id, tasks) in self {
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

async fn build_task_set(docker: &Docker) -> Result<TaskSet> {
    let mut result = vec![];
    for id in list_running_containers(&docker).await? {
        let tasks = run_toolchain_discovery(&docker, &id, TOOLCHAIN)
            .await
            .context(Errors::DiscoveryFailed(id.clone()));
        match tasks {
            Ok(tasks) => result.push((id, tasks)),
            Err(e) => warn!("{:?}", e),
        }
    }
    Ok(TaskSet(result))
}

fn start_discovery<'a>(docker: &'a Docker) -> impl Stream<Item = Result<TaskSet>> + Unpin + 'a {
    let interval = interval(Duration::from_secs(1));
    let stream = IntervalStream::new(interval);

    let stream = stream.then(move |_| async move { build_task_set(docker).await });
    Box::pin(stream)
}
