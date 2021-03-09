//! Docker integration module
//!
//! Docker integration module provides following capabilities:
//!
//! * listing target container;
//! * discovery tasks;
//! * running task for a given container
use crate::agent::TaskDef;
use crate::prelude::*;
use bollard::{
    container::{
        Config, ListContainersOptions, LogOutput, LogsOptions, RemoveContainerOptions,
        WaitContainerOptions,
    },
    service::HostConfig,
    Docker,
};
use futures::{stream::Stream, StreamExt};
use hyper::body::Bytes;
use serde::de::DeserializeOwned;
use std::{collections::hash_map::HashMap, fs::File};
use tokio::sync::mpsc;
use tokio_util::{
    codec::{FramedRead, LinesCodec},
    io::StreamReader,
};

#[derive(Error, Debug)]
enum Errors {
    #[error("Container exit code code is non zero")]
    ContainerNonZeroExitCode(i64),

    #[error("Invalid container id given: {0}")]
    InvalidContainerId(String),
}

pub struct Container<'a> {
    docker: &'a Docker,
    pub id: String,
}

impl<'a> Container<'a> {
    pub async fn start(
        docker: &'a Docker,
        image: &str,
        command: Option<Vec<&str>>,
    ) -> Result<Container<'a>> {
        Self::start_with_pid_mode(docker, image, command, None).await
    }

    pub async fn start_with_pid_mode(
        docker: &'a Docker,
        image: &str,
        command: Option<Vec<&str>>,
        pid_mode: Option<String>,
    ) -> Result<Container<'a>> {
        let host_config = HostConfig {
            pid_mode,
            ..Default::default()
        };
        let config = Config {
            image: Some(image),
            cmd: command,
            host_config: Some(host_config),
            ..Default::default()
        };

        let container = docker.create_container::<&str, _>(None, config);
        let id = container.await?.id;
        docker.start_container::<&str>(&id, None).await?;

        Ok(Container { docker, id })
    }

    pub fn read_logs(&self) -> impl Stream<Item = Result<LogOutput>> {
        let options = Some(LogsOptions::<String> {
            stdout: true,
            ..Default::default()
        });

        self.docker
            .logs(&self.id, options)
            .map(|r| r.context("Failed reading container stdout"))
    }

    pub async fn wait(&self) -> Result<i64> {
        let options = WaitContainerOptions {
            condition: "not-running",
        };
        let response = self
            .docker
            .wait_container(&self.id, Some(options))
            .next()
            .await
            .expect("wait_container() failed")?;

        ensure!(
            response.status_code == 0,
            Errors::ContainerNonZeroExitCode(response.status_code)
        );
        Ok(response.status_code)
    }

    pub async fn remove(self) -> Result<()> {
        let options = Some(RemoveContainerOptions {
            force: true,
            ..Default::default()
        });

        self.docker
            .remove_container(&self.id, options)
            .await
            .context("Unable to remove container")
    }
}

/// Lists running containers.
///
/// Only containers with `io.kubernetes.pod.name` labels are listed.
pub async fn list_running_containers(docker: &Docker) -> Result<Vec<String>> {
    let mut filters = HashMap::new();
    filters.insert("label", vec!["io.kubernetes.pod.name"]);
    let options = ListContainersOptions {
        filters,
        ..Default::default()
    };

    let container_ids = docker
        .list_containers(Some(options))
        .await?
        .into_iter()
        .filter_map(|c| c.id)
        .collect();

    Ok(container_ids)
}

/// Running container discovery process.
///
/// Discovery process implemented as follows:
/// * toolchain container `/discover` executable is run;
/// * pid namespace is shared between toolchain and target containers;
/// * executable should return a vector of [TaskDef] in `ndjson` format (each line is it's own json payload)
pub async fn run_toolchain_discovery(
    docker: &Docker,
    container_id: &str,
    toolchain_image: &str,
) -> Result<Vec<TaskDef>> {
    ensure!(
        !container_id.is_empty(),
        Errors::InvalidContainerId(container_id.into())
    );
    let c = Container::start_with_pid_mode(
        docker,
        &toolchain_image,
        Some(vec!["/discover"]),
        Some(format!("container:{}", container_id)),
    )
    .await?;

    let stdout_stream = c.read_logs().filter_map(only_stdout);

    let reader = StreamReader::new(stdout_stream);
    let stdout = FramedRead::new(reader, LinesCodec::new());

    let task_defs = stdout
        .map(|e| e.context("Unable to read stdout from upstream container"))
        .map(read_from_json)
        .collect::<Vec<_>>()
        .await;

    c.wait().await?;
    c.remove().await?;

    task_defs.into_iter().collect::<Result<Vec<_>>>()
}

pub async fn run_toolchain_task(
    docker: &Docker,
    container_id: &str,
    toolchain_image: &str,
    task: &TaskDef,
    stdout: Option<mpsc::Sender<Bytes>>,
    stderr: Option<mpsc::Sender<Bytes>>,
) -> Result<(i64, Option<File>)> {
    ensure!(
        !container_id.is_empty(),
        Errors::InvalidContainerId(container_id.into())
    );
    let container = Container::start_with_pid_mode(
        docker,
        &toolchain_image,
        Some(vec![&task.command]),
        Some(format!("container:{}", container_id)),
    )
    .await?;

    let mut logs = container.read_logs();
    
    // TODO correct Result handling here
    tokio::spawn(async move {
        while let Some(record) = logs.next().await {
            match record.unwrap() {
                LogOutput::StdOut { message } => {
                    if let Some(stdout) = &stdout {
                        stdout.send(message).await;
                    }
                }
                LogOutput::StdErr { message } => {
                    if let Some(stderr) = &stderr {
                        stderr.send(message).await;
                    }
                }
                _ => {}
            }
        }
    });

    let status_code = container.wait().await?;
    container.remove().await?;

    Ok((status_code, None))
}

fn read_from_json<T: DeserializeOwned>(line: Result<String>) -> Result<T> {
    line.and_then(|l| serde_json::from_str::<T>(&l).context("Unable to read JSON"))
}

async fn only_stdout(batch: Result<LogOutput>) -> Option<IoResult<Bytes>> {
    use std::io;
    batch
        .map(|i| match i {
            LogOutput::StdOut { message } => Some(message),
            _ => None,
        })
        .map_err(|_e| io::Error::new(io::ErrorKind::Other, "oh no!"))
        .transpose()
}
