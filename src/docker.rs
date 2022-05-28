//! Docker integration module
//!
//! Docker integration module provides following capabilities:
//!
//! * listing target container;
//! * discovery tasks;
//! * running task for a given container
use crate::{agent::TaskDef, prelude::*};
use bollard::{
    container::{
        Config, DownloadFromContainerOptions, ListContainersOptions, LogOutput, LogsOptions,
        RemoveContainerOptions, WaitContainerOptions,
    },
    image::{CreateImageOptions, ListImagesOptions},
    service::HostConfig,
    Docker,
};
use futures::{stream::Stream, StreamExt};
use hyper::body::Bytes;
use std::{
    collections::{hash_map::HashMap, hash_set::HashSet},
    convert::TryFrom,
    fs::File,
    io::{Read, Write},
    path::Path,
    time::Duration,
};
use tar::{Archive, Builder, Header};
use tempfile::tempfile;
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

/// Task set is the set of all the tasks found in the system
/// on a given round of discovery
pub type TaskSet = Vec<TaskDef>;

/// Workdir for toolchain tasks.
///
/// All toolchain tasks will be executed with this path as a workdir and all files
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
            follow: true,
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
/// Returns the list of running container ids.
pub async fn list_running_containers(docker: &Docker, labels: &[&str]) -> Result<HashSet<String>> {
    let mut filters = HashMap::new();
    filters.insert("label", labels.to_vec());
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
pub async fn run_discovery(
    docker: &Docker,
    container_id: &str,
    toolchain_image: &str,
) -> Result<Vec<TaskDef>> {
    ensure!(
        !container_id.is_empty(),
        InvalidContainerId(container_id.into())
    );

    let target_container = docker.inspect_container(container_id, None).await?;
    let labels = target_container.config.and_then(|c| c.labels);

    let pid_mode = format!("container:{}", container_id);
    let toolchain_container = Container::start_with_pid_mode(
        docker,
        toolchain_image,
        Some(&[DISCOVER_SCRIPT_PATH]),
        Some(&pid_mode),
    );
    let toolchain_container = toolchain_container.await?;

    let stdout = toolchain_container.read_logs().filter_map(only_stdout);
    let stdout = StreamReader::new(stdout);
    let stdout = FramedRead::new(stdout, LinesCodec::new())
        .map(|e| e.context("Unable to read stdout from upstream container"));

    let task_defs = stdout
        .map(parse_json)
        .map(|json| build_task_def(json, container_id, toolchain_image, &labels))
        .collect::<Vec<_>>()
        .await;

    toolchain_container.wait().await?;
    toolchain_container.remove().await?;

    let tasks = task_defs.into_iter().collect::<Result<Vec<_>>>()?;
    trace!(
        "Toolchain: {}, container: {}. Found {} tasks",
        toolchain_image,
        &container_id[0..12],
        tasks.len()
    );
    Ok(tasks)
}

pub async fn run_task<W: Write>(
    docker: &Docker,
    task: &TaskDef,
    stdout: mpsc::Sender<Bytes>,
    stderr: mpsc::Sender<Bytes>,
    artifacts: Option<W>,
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

    let redirect_process = tokio::spawn(redirect_output(
        container.read_logs(),
        tempfile()?,
        tempfile()?,
        stdout,
        stderr,
    ));
    let status_code = container.wait().await?;
    let (stdout_file, stderr_file) = redirect_process.await??;
    if let Some(mut artifacts) = artifacts {
        container
            .download(TOOLCHAIN_WORKDIR, &mut artifacts)
            .await?;
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
    mut stdout_file: File,
    mut stderr_file: File,
    stdout: mpsc::Sender<Bytes>,
    stderr: mpsc::Sender<Bytes>,
) -> Result<(File, File)> {
    while let Some(record) = logs.next().await {
        match record? {
            LogOutput::StdOut { message } => {
                stdout_file.write_all(&message)?;
                stdout.send(message).await?;
            }
            LogOutput::StdErr { message } => {
                stderr_file.write_all(&message)?;
                stderr.send(message).await?;
            }
            _ => {}
        }
    }
    Ok((stdout_file, stderr_file))
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
    labels: &Option<HashMap<String, String>>,
) -> Result<TaskDef> {
    use serde_json::{json, Value};

    if let Value::Object(mut payload) = json? {
        payload.insert("container_id".to_owned(), json!(container_id));
        payload.insert("toolchain".to_owned(), json!(toolchain));
        if let Some(labels) = labels {
            payload.insert("labels".to_owned(), json!(labels));
        }
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

/// Starts discovery loop and returns watch handle
///
/// Each time watch loop registers change in the list of running containers,
/// watch channel will be notified with full list of running containers.
pub fn start_discovery(
    docker: Docker,
    toolchains: Vec<String>,
    labels: Vec<String>,
) -> (impl Stream<Item = TaskSet>, JoinHandle<Result<()>>) {
    let (tx, rx) = watch::channel(vec![]);
    let join_handle = tokio::spawn(discovery_loop(docker, tx, toolchains, labels));
    (WatchStream::new(rx), join_handle)
}

async fn discovery_loop(
    docker: Docker,
    tx: watch::Sender<TaskSet>,
    toolchains: Vec<String>,
    labels: Vec<String>,
) -> Result<()> {
    let labels = labels.iter().map(String::as_str).collect::<Vec<_>>();

    let mut containers = HashSet::new();
    let mut tasks: TaskSet = vec![];
    loop {
        let alive_containers = list_running_containers(&docker, &labels).await?;
        let new_containers = alive_containers.difference(&containers).collect::<Vec<_>>();

        // Running discovery for "newborn" containers
        if !new_containers.is_empty() {
            let new_tasks = discover_tasks(&docker, &toolchains, &new_containers, &labels).await?;
            tasks.extend(new_tasks);
        }

        // Retaining tasks from alive containers only
        tasks.retain(|task| alive_containers.contains(&task.container_id));

        containers = alive_containers;
        tx.send(tasks.clone())?;
        sleep(Duration::from_secs(1)).await;
    }
}

/// Build [TaskSet] for a given list of containers and toolchains.
///
/// Receive 2 iterables: toolchains and containers. Execute each toolchain discovery script
/// for each container and builds [TaskSet].
pub async fn discover_tasks<'a, T, St, C, Sc>(
    docker: &Docker,
    toolchains: &'a T,
    containers: &'a C,
    _labels: &[&str],
) -> Result<TaskSet>
where
    St: AsRef<str>,
    Sc: AsRef<str>,
    &'a T: IntoIterator<Item = St>,
    &'a C: IntoIterator<Item = Sc>,
{
    let mut result = vec![];
    for container in containers {
        for toolchain in toolchains {
            let tasks = run_discovery(docker, container.as_ref(), toolchain.as_ref())
                .await
                .with_context(|| DiscoveryFailed(container.as_ref().to_string()));
            result.extend(tasks?);
        }
    }
    Ok(result)
}

/// Pulls toolchain images if they are not locally available.
pub async fn ensure_toolchain_images_exists(
    docker: &Docker,
    toolchains: &[impl AsRef<str>],
) -> Result<()> {
    let mut filters = HashMap::new();
    filters.insert("dangling", vec!["false"]);
    let options = ListImagesOptions {
        filters,
        ..Default::default()
    };
    let present_images = docker.list_images(Some(options)).await?;

    let present_images = present_images
        .iter()
        .flat_map(|i| &i.repo_tags)
        .map(|s| s.as_str())
        .collect::<HashSet<_>>();

    let toolchains = toolchains
        .iter()
        .map(|t| t.as_ref())
        .collect::<HashSet<_>>();

    for missing_image in toolchains.difference(&present_images) {
        pull_image(docker, missing_image).await?;
    }

    Ok(())
}

/// Pulls given image from remote repository
pub async fn pull_image(docker: &Docker, from_image: &str) -> Result<()> {
    info!("Pulling image: {}", from_image);
    let options = CreateImageOptions {
        from_image,
        ..Default::default()
    };
    let mut stream = docker.create_image(Some(options), None, None);

    while let Some(info) = stream.next().await {
        info?;
    }

    Ok(())
}

/// Creating new tar archive appending given entries
///
/// Takes input tar-archive and cloning all it's content to the output one and then appending given entries.
fn tar_append(
    input: impl Read,
    output: impl Write,
    files: &[(&str, impl AsRef<Path>)],
) -> Result<()> {
    let mut input = Archive::new(input);
    let mut output = Builder::new(output);

    for entry in input.entries()? {
        let entry = entry?;
        if let Some(path) = entry.path()?.to_str() {
            let mut header = Header::new_gnu();
            header.set_size(entry.size());
            header.set_path(path)?;
            header.set_cksum();

            output.append(&header, entry)?;
        }
    }

    for (path, file) in files {
        output.append_path_with_name(file.as_ref(), path)?;
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use std::io::Cursor;

    use tar::{Builder, Header};
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn append_files_to_a_tar_archive() -> Result<()> {
        let dir = tempdir()?;
        let baz_file = dir.path().join("baz");
        write!(File::create(&baz_file)?, "baz content")?;

        let archive = tar_create(&[("foo", "foo contents"), ("foo/bar", "foo/bar contents")])?;
        assert_eq!(tar_list(&archive)?, ["foo", "foo/bar"]);

        let mut new_archive = vec![];
        tar_append(
            Cursor::new(&archive),
            &mut new_archive,
            &[("baz", baz_file)],
        )?;

        assert_eq!(tar_list(&new_archive)?, ["foo", "foo/bar", "baz"]);

        Ok(())
    }

    type PathAndContents<'a> = (&'a str, &'a str);
    fn tar_create(entries: &[PathAndContents]) -> Result<Vec<u8>> {
        let mut archive = Builder::new(Vec::new());

        for (path, contents) in entries {
            let mut header = Header::new_gnu();
            header.set_size(contents.len() as u64);
            header.set_cksum();

            archive.append_data(&mut header, path, contents.as_bytes())?;
        }

        Ok(archive.into_inner()?)
    }

    fn tar_list(input: &[u8]) -> Result<Vec<String>> {
        let mut archive = Archive::new(Cursor::new(input));

        let mut result = vec![];
        for entry in archive.entries()? {
            let entry = entry?;
            if let Some(path) = entry.path()?.to_str() {
                result.push(path.to_string());
            }
        }
        Ok(result)
    }
}
