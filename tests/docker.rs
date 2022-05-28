extern crate curator;

use bollard::Docker;
use curator::agent::Artifact;
use curator::prelude::*;
use curator::{
    agent::{self, TaskDef, TaskProgress},
    docker::{list_running_containers, run_discovery},
};
use rstest::*;
use std::fs;
use std::{borrow::Cow, path::PathBuf};
use tar::Archive;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Those test relies on docker-compose `it-sample-container` service container.
///
/// For tests to work properly on host machine you need to start sample container first:
/// ```
/// docker-compose run it-sample-container
/// ```

const TOOLCHAIN: &str = "bazhenov.me/curator/toolchain-example:dev";

#[fixture]
fn dock() -> Docker {
    Docker::connect_with_local_defaults().expect("Unable to get Docker instance")
}

#[rstest]
#[tokio::test]
async fn list_containers_and_run_discovery(dock: Docker) -> Result<()> {
    let container = get_sample_container(&dock).await?;

    let task_defs = run_discovery(&dock, &container, TOOLCHAIN).await?;

    assert!(!task_defs.is_empty());

    Ok(())
}

#[rstest]
#[tokio::test]
async fn run_toolchain_for_exit_code_and_stdout(dock: Docker) -> Result<()> {
    let (status, _, stdout) = run_test_toolchain(&dock, &["sh", "-c", "echo 'Hello'"]).await?;

    assert_eq!(status, 0);
    assert_eq!("Hello\n", stdout);

    Ok(())
}

#[rstest]
#[tokio::test]
async fn run_toolchain_for_artifact(dock: Docker) -> Result<()> {
    let (status, artifacts, _) = run_test_toolchain(&dock, &["sh", "-c", "touch test.txt"]).await?;

    assert_eq!(status, 0);

    let artifacts = artifacts.expect("Empty artifacts");

    let entries = Archive::new(fs::File::open(artifacts.as_ref())?)
        .entries()?
        .map(|e| e?.path().map(Cow::into_owned))
        .collect::<IoResult<Vec<_>>>()?;

    let expected_paths = ["curator/", "curator/test.txt", "stdout.txt", "stderr.txt"]
        .iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    assert_eq!(entries, expected_paths);

    Ok(())
}

#[rstest]
#[tokio::test]
async fn not_existent_toolchain_image(dock: Docker) -> Result<()> {
    use curator::docker::Errors as DockerErrors;

    let result = run_toolchain(&dock, &["date"], "intentionally-not-existent-toolchain").await;

    if let Err(cause) = result {
        if let Ok(cause) = cause.downcast::<DockerErrors>() {
            assert_eq!(cause, DockerErrors::ImageNotFound);
            return Ok(());
        }
    }

    panic!("ImageNotFound error should be generated");
}

async fn run_toolchain(
    docker: &Docker,
    command: &[&str],
    toolchain: &str,
) -> Result<(i32, Option<Artifact>, String)> {
    use TaskProgress::*;

    let container_id = get_sample_container(docker).await?;

    let task = TaskDef {
        name: String::from(""),
        command: command.iter().map(|s| String::from(*s)).collect(),
        container_id,
        toolchain: toolchain.into(),
        ..Default::default()
    };

    let (progress_sender, mut progress_receiver) = mpsc::channel(1);

    let join_handle = tokio::spawn(agent::execute_task(
        docker.clone(),
        task,
        progress_sender,
        Uuid::new_v4(),
    ));

    let mut artifacts = None;
    let mut stdout = String::new();
    let mut exit_code = None;

    while let Some(progress) = progress_receiver.recv().await {
        match progress {
            Stdout(line) => stdout.push_str(line.as_str()),
            ArtifactsReady(artifact) => artifacts = Some(artifact),
            Finished(code) => exit_code = Some(code),
            _ => {}
        }
    }

    join_handle.await??;
    let exit_code = exit_code.expect("No exit code reported");

    Ok((exit_code, artifacts, stdout))
}

async fn run_test_toolchain(
    docker: &Docker,
    command: &[&str],
) -> Result<(i32, Option<Artifact>, String)> {
    run_toolchain(docker, command, TOOLCHAIN).await
}

async fn get_sample_container(docker: &Docker) -> Result<String> {
    let mut containers = list_running_containers(docker, &["io.kubernetes.pod.name"]).await?;
    let mut drain = containers.drain();
    Ok(drain
        .next()
        .expect("No running containers with 'io.kubernetes.pod.name' label"))
}
