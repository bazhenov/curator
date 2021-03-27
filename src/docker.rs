//! Docker integration module
//!
//! Docker integration module provides following capabilities:
//!
//! * listing target container;
//! * discovery tasks;
//! * running task for a given container
use crate::{
    agent::{TaskDef, TaskSet},
    prelude::*,
};
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
use std::{collections::hash_map::HashMap, convert::TryFrom, io::Write, time::Duration};
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
    time::sleep,
};
use tokio_stream::wrappers::WatchStream;
use tokio_util::{
    codec::{FramedRead, LinesCodec},
    io::StreamReader,
};
use Errors::*;

type BollardError = bollard::errors::Error;

/// Workdir for toolchain tasks.
///
/// All toolchain tasks will be executed with this path as a workid and all files
/// left in this directory will be treated as a task artifacts.
///
/// This path should be created in toolchain container upfront when building toolchain container
pub const TOOLCHAIN_WORKDIR: &str = "/var/run/curator";

/// Fully qualified path to the discovery script
pub const DISCOVER_SCRIPT_PATH: &str = "/discover";

#[derive(Error, Debug, PartialEq)]
pub enum Errors {
    #[error("Container exit code code is non zero")]
    ContainerNonZeroExitCode(i64),

    #[error("Invalid container id given: {0}")]
    InvalidContainerId(String),

    #[error("Unable to remove container: {0}")]
    UnableToRemoveContainer(String),

    #[error("Discovery failed on container: {0}")]
    DiscoveryFailed(String),

    #[error("Incorrect TaskDef json-payload")]
    InvalidTaskDefJson,

    #[error("Docker image not found")]
    ImageNotFound,
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
            working_dir: Some(TOOLCHAIN_WORKDIR),
            ..Default::default()
        };

        let container = docker.create_container::<&str, _>(None, config);
        let id = container.await.map_err(map_bollard_errors)?.id;
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
    trace!("Running discovery. Toolchain: {}, container: {}", toolchain_image, &container_id[0..12]);
    let pid_mode = format!("container:{}", container_id);
    let container = Container::start_with_pid_mode(
        docker,
        &toolchain_image,
        Some(&[DISCOVER_SCRIPT_PATH]),
        Some(&pid_mode),
    );
    let container = container.await?;

    let stdout_stream = container.read_logs().filter_map(only_stdout);

    let reader = StreamReader::new(stdout_stream);
    let stdout = FramedRead::new(reader, LinesCodec::new())
        .map(|e| e.context("Unable to read stdout from upstream container"));

    let task_defs = stdout
        .map(parse_json)
        .map(|json| build_task_def(json, container_id, toolchain_image))
        .collect::<Vec<_>>()
        .await;

    container.wait().await?;
    container.remove().await?;

    task_defs.into_iter().collect::<Result<Vec<_>>>()
}

pub async fn run_toolchain_task<W: Write>(
    docker: &Docker,
    task: &TaskDef,
    stdout: Option<mpsc::Sender<Bytes>>,
    stderr: Option<mpsc::Sender<Bytes>>,
    artifacts: Option<&mut W>,
) -> Result<i32> {
    let container_id = &task.container_id;
    ensure!(
        !container_id.is_empty(),
        InvalidContainerId(container_id.into())
    );
    let pid_mode = format!("container:{}", container_id);
    let container = Container::start_with_pid_mode(
        docker,
        &task.toolchain,
        Some(&task.command),
        Some(pid_mode),
    );
    let container = container.await?;

    let redirect_process = tokio::spawn(redirect_output(container.read_logs(), stdout, stderr));
    let status_code = container.wait().await?;
    redirect_process.await??;
    if let Some(artifacts) = artifacts {
        container.download(TOOLCHAIN_WORKDIR, artifacts).await?;
    }
    container.remove().await?;

    Ok(i32::try_from(status_code)?)
}

/// Maps some of the bollard errors to more meaningful error codes that
/// application can handle explicitly
fn map_bollard_errors(error: BollardError) -> AnyhowError {
    if let BollardError::DockerResponseNotFoundError { message } = &error {
        if message.contains("No such image") {
            return AnyhowError::new(error).context(ImageNotFound);
        }
    }
    error.into()
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

fn parse_json(line: Result<String>) -> Result<serde_json::Value> {
    line.and_then(|l| serde_json::from_str(&l).context("Unable to read JSON"))
}

/// Adds correct container and toolchain to provided JSON
///
/// Each [TaskDef] instance should have `container_id` and `toolchain` properties. This
/// methods adds those properties to json returned from discovery script. So there is no way
/// discovery script can abuse or accidentally provide incorrect values for those properties.
fn build_task_def(
    json: Result<serde_json::Value>,
    container_id: &str,
    toolchain: &str,
) -> Result<TaskDef> {
    use serde_json::{json, Value};

    if let Value::Object(mut payload) = json? {
        payload.insert("container_id".to_owned(), json!(container_id));
        payload.insert("toolchain".to_owned(), json!(toolchain));
        serde_json::from_value(Value::Object(payload)).context(InvalidTaskDefJson)
    } else {
        bail!(InvalidTaskDefJson)
    }
}

async fn only_stdout(batch: Result<LogOutput>) -> Option<IoResult<Bytes>> {
    use std::io;
    batch
        .map(|i| match i {
            LogOutput::StdOut { message } => Some(message),
            _ => None,
        })
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
        .transpose()
}

pub fn start_discovery(
    docker: Docker,
    toolchains: Vec<String>,
) -> (impl Stream<Item = TaskSet>, JoinHandle<Result<()>>) {
    let (tx, rx) = watch::channel(vec![]);
    let join_handle = tokio::spawn(discovery_loop(docker, tx, toolchains));
    (WatchStream::new(rx), join_handle)
}

async fn discovery_loop(
    docker: Docker,
    tx: watch::Sender<TaskSet>,
    toolchains: Vec<String>,
) -> Result<()> {
    loop {
        let task_set = build_task_set(&docker, &toolchains).await?;
        tx.send(task_set)?;
        sleep(Duration::from_secs(1)).await;
    }
}

pub async fn build_task_set(docker: &Docker, toolchains: &[impl AsRef<str>]) -> Result<TaskSet> {
    let mut result = vec![];
    for id in &list_running_containers(&docker).await? {
        for toolchain in toolchains {
            let tasks = run_toolchain_discovery(&docker, &id, toolchain.as_ref())
                .await
                .context(DiscoveryFailed(id.clone()));
            result.extend(tasks?);
        }
    }
    Ok(result)
}
