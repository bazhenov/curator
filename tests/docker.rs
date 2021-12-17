extern crate curator;

use bollard::Docker;
use curator::prelude::*;
use curator::{
    agent::TaskDef,
    docker::{self, list_running_containers, run_discovery},
};
use rstest::*;
use std::{borrow::Cow, io::Cursor, path::PathBuf};
use tar::Archive;
use tokio::sync::mpsc;

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

    let entries = Archive::new(artifacts)
        .entries()?
        .map(|e| e?.path().map(Cow::into_owned))
        .collect::<IoResult<Vec<_>>>()?;

    let expected_paths = ["curator/", "curator/test.txt"]
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
) -> Result<(i32, Cursor<Vec<u8>>, String)> {
    let container_id = get_sample_container(docker).await?;

    let task = TaskDef {
        name: String::from(""),
        command: command.iter().map(|s| String::from(*s)).collect(),
        container_id,
        toolchain: toolchain.into(),
        ..Default::default()
    };

    let (sender, receiver) = mpsc::channel(1);
    let stdout_content = tokio::spawn(collect(receiver));
    let mut artifacts = Cursor::new(vec![]);

    let status_code =
        docker::run_task(docker, &task, Some(sender), None, Some(&mut artifacts)).await?;
    artifacts.set_position(0);

    Ok((status_code, artifacts, stdout_content.await?))
}

async fn run_test_toolchain(
    docker: &Docker,
    command: &[&str],
) -> Result<(i32, Cursor<Vec<u8>>, String)> {
    run_toolchain(docker, command, TOOLCHAIN).await
}

async fn get_sample_container(docker: &Docker) -> Result<String> {
    let mut containers = list_running_containers(docker, &["io.kubernetes.pod.name"]).await?;
    let mut drain = containers.drain();
    Ok(drain.next().unwrap())
}

async fn collect<T: AsRef<[u8]>>(mut receiver: mpsc::Receiver<T>) -> String {
    let mut result = String::new();

    while let Some(chunk) = receiver.recv().await {
        result.push_str(&String::from_utf8_lossy(chunk.as_ref()));
    }
    result
}
