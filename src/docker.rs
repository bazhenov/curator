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
    container::{Config, ListContainersOptions, LogOutput, LogsOptions, WaitContainerOptions},
    service::HostConfig,
    Docker,
};
use futures::{stream::Stream, StreamExt};
use hyper::body::Bytes;
use serde::de::DeserializeOwned;
use tokio_util::{
    codec::{FramedRead, LinesCodec},
    io::StreamReader,
};
use std::collections::hash_map::HashMap;

#[derive(Error, Debug)]
enum Errors {
    #[error("Container exit code code is non zero")]
    ContainerNonZeroExitCode(i64),
}

pub struct Container<'a> {
    docker: &'a Docker,
    container_id: String,
}

impl<'a> Container<'a> {
    pub async fn start(
        docker: &'a Docker,
        image: &str,
        command: Option<Vec<&str>>,
    ) -> Result<Container<'a>> {
        let host_config = HostConfig {
            auto_remove: Some(true),
            ..Default::default()
        };
        let config = Config {
            image: Some(image),
            cmd: command,
            host_config: Some(host_config),
            ..Default::default()
        };

        let container = docker.create_container::<&str, _>(None, config);
        let container_id = container.await?.id;
        docker.start_container::<&str>(&container_id, None).await?;

        Ok(Container {
            docker,
            container_id,
        })
    }

    pub fn read_logs(&self) -> impl Stream<Item = Result<LogOutput>> {
        let options = Some(LogsOptions::<String> {
            stdout: true,
            ..Default::default()
        });

        self.docker
            .logs(&self.container_id, options)
            .map(|r| r.context("Failed reading container stdout"))
    }

    pub async fn check_status_code(&self) -> Result<i64> {
        let options = WaitContainerOptions {
            condition: "not-running",
        };
        let response = self
            .docker
            .wait_container(&self.container_id, Some(options))
            .next()
            .await
            .expect("wait_container() failed")?;

        if response.status_code != 0 {
            bail!(Errors::ContainerNonZeroExitCode(response.status_code));
        }

        Ok(response.status_code)
    }
}

/// Running container discovery process.
/// 
/// Discovery process implemented as follows:
/// * each toolchain container `/discover` executable is run;
/// * pid namespace is shared between toolchain and target containers;
/// * executable should return a vector of [TaskDef] in `ndjson` format (each line is it's own json payload)
pub async fn run_docker_discovery(
    docker: &Docker,
    toolchain_images: &[&str],
) -> Result<Vec<TaskDef>> {
    for image in toolchain_images {
        let c = Container::start(docker, &image, Some(vec!["/discover"])).await?;

        c.check_status_code().await?;
    }

    Ok(vec![])
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

pub async fn run_toolchain_discovery(
    docker: &Docker,
    _container: &str,
    toolchain_image: &str,
) -> Result<Vec<TaskDef>> {
    let c = Container::start(docker, &toolchain_image, Some(vec!["/discover"])).await?;

    let stdout_stream = c.read_logs().filter_map(only_stdout);

    let reader = StreamReader::new(stdout_stream);
    let stdout = FramedRead::new(reader, LinesCodec::new());

    let task_defs = stdout
        .map(|e| e.context("Unable to read stdout from upstream container"))
        .map(read_from_json)
        .collect::<Vec<_>>()
        .await;

    c.check_status_code().await?;

    task_defs.into_iter().collect::<Result<Vec<_>>>()
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
