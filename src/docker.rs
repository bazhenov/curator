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
        Config, DownloadFromContainerOptions, ListContainersOptions, LogOutput, LogsOptions,
        RemoveContainerOptions, WaitContainerOptions,
    },
    service::HostConfig,
    Docker,
};
use futures::{stream::Stream, StreamExt};
use hyper::body::Bytes;
use serde::de::DeserializeOwned;
use std::{collections::hash_map::HashMap, io::Write};
use tokio::sync::mpsc;
use tokio_util::{
    codec::{FramedRead, LinesCodec},
    io::StreamReader,
};
use Errors::*;

pub const AGENT_WORKDIR: &str = "/var/run/curator";

#[derive(Error, Debug)]
enum Errors {
    #[error("Container exit code code is non zero")]
    ContainerNonZeroExitCode(i64),

    #[error("Invalid container id given: {0}")]
    InvalidContainerId(String),

    #[error("Unable to start new container")]
    UnableToStartNewContainer,

    #[error("Unable to remove container: {0}")]
    UnableToRemoveContainer(String),
}

pub struct Container<'a> {
    docker: &'a Docker,
    pub id: String,
}

impl<'a> Container<'a> {
    pub async fn start<T: AsRef<str>>(
        docker: &'a Docker,
        image: &str,
        command: Option<&[T]>,
    ) -> Result<Container<'a>> {
        Self::start_with_pid_mode(docker, image, command, None).await
    }

    pub async fn start_with_pid_mode<T: AsRef<str>>(
        docker: &'a Docker,
        image: &str,
        command: Option<&[T]>,
        pid_mode: Option<T>,
    ) -> Result<Container<'a>> {
        let pid_mode = pid_mode.map(|value| value.as_ref().into());
        let command = command.map(|slice| slice.iter().map(|v| v.as_ref()).collect());

        let host_config = HostConfig {
            pid_mode,
            ..Default::default()
        };
        let config = Config {
            image: Some(image),
            cmd: command,
            host_config: Some(host_config),
            working_dir: Some(AGENT_WORKDIR),
            ..Default::default()
        };

        let container = docker.create_container::<&str, _>(None, config);
        let id = container.await?.id;
        docker.start_container::<&str>(&id, None).await?;

        Ok(Container { docker, id })
    }

    /// Download files from a given path.
    ///
    /// Create a tar archive from a files at a given path inside the container and writes this archive to
    /// the target.
    pub async fn download(&self, path: &str, target: &mut impl Write) -> Result<()> {
        let options = DownloadFromContainerOptions { path };
        let mut stream = self.docker.download_from_container(&self.id, Some(options));

        while let Some(batch) = stream.next().await {
            target.write_all(&batch?)?;
        }

        Ok(())
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
            ContainerNonZeroExitCode(response.status_code)
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
            .context(UnableToRemoveContainer(self.id))
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
        InvalidContainerId(container_id.into())
    );
    let pid_mode = format!("container:{}", container_id);
    let container = Container::start_with_pid_mode(
        docker,
        &toolchain_image,
        Some(&["/discover"]),
        Some(&pid_mode),
    );
    let container = container.await.context(UnableToStartNewContainer)?;

    let stdout_stream = container.read_logs().filter_map(only_stdout);

    let reader = StreamReader::new(stdout_stream);
    let stdout = FramedRead::new(reader, LinesCodec::new());

    let task_defs = stdout
        .map(|e| e.context("Unable to read stdout from upstream container"))
        .map(read_from_json)
        .collect::<Vec<_>>()
        .await;

    container.wait().await?;
    container.remove().await?;

    task_defs.into_iter().collect::<Result<Vec<_>>>()
}

pub async fn run_toolchain_task<W: Write>(
    docker: &Docker,
    container_id: &str,
    toolchain_image: &str,
    task: &TaskDef,
    stdout: Option<mpsc::Sender<Bytes>>,
    stderr: Option<mpsc::Sender<Bytes>>,
    artifacts: Option<&mut W>,
) -> Result<i64> {
    ensure!(
        !container_id.is_empty(),
        InvalidContainerId(container_id.into())
    );
    let mut command = vec![&task.command];
    command.extend(&task.args);
    let pid_mode = format!("container:{}", container_id);
    let container =
        Container::start_with_pid_mode(docker, &toolchain_image, Some(&command), Some(&pid_mode));
    let container = container.await.context(UnableToStartNewContainer)?;

    let redirect_process = tokio::spawn(redirect_output(container.read_logs(), stdout, stderr));
    let status_code = container.wait().await?;
    redirect_process.await??;
    if let Some(artifacts) = artifacts {
        container.download(AGENT_WORKDIR, artifacts).await?;
    }
    container.remove().await?;

    Ok(status_code)
}

async fn redirect_output(
    mut logs: impl Stream<Item = Result<LogOutput>> + Unpin,
    stdout: Option<mpsc::Sender<Bytes>>,
    stderr: Option<mpsc::Sender<Bytes>>,
) -> Result<()> {
    while let Some(record) = logs.next().await {
        match record? {
            LogOutput::StdOut { message } => {
                if let Some(stdout) = &stdout {
                    stdout.send(message).await?;
                }
            }
            LogOutput::StdErr { message } => {
                if let Some(stderr) = &stderr {
                    stderr.send(message).await?;
                }
            }
            _ => {}
        }
    }
    Ok(())
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
