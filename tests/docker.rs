extern crate curator;

use bollard::Docker;
use curator::prelude::*;
use curator::{
    agent::TaskDef,
    docker::{list_running_containers, run_toolchain_discovery, run_toolchain_task},
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

macro_rules! str_vec {
    ($($x:expr),*) => (vec![$($x.to_string()),*]);
}

#[fixture]
fn docker() -> Docker {
    Docker::connect_with_local_defaults().expect("Unable to get Docker instance")
}

#[rstest]
#[tokio::test]
async fn list_containers_and_run_discovery(docker: Docker) -> Result<()> {
    let container = get_sample_container(&docker).await?;

    let task_defs = run_toolchain_discovery(&docker, &container, TOOLCHAIN).await?;

    assert_eq!(task_defs.len(), 1);

    Ok(())
}

#[rstest]
#[tokio::test]
async fn run_toolchain_for_exit_code_and_stdout(docker: Docker) -> Result<()> {
    let (status, _, stdout) =
        run_test_toolchain(&docker, str_vec!["sh", "-c", "echo 'Hello'"]).await?;

    assert_eq!(status, 0);
    assert_eq!("Hello\n", stdout);

    Ok(())
}

#[rstest]
#[tokio::test]
async fn run_toolchain_for_artifact(docker: Docker) -> Result<()> {
    let (status, artifacts, _) =
        run_test_toolchain(&docker, str_vec!["sh", "-c", "touch test.txt"]).await?;

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

async fn run_test_toolchain(
    docker: &Docker,
    mut args: Vec<String>,
) -> Result<(i64, Cursor<Vec<u8>>, String)> {
    let container = get_sample_container(&docker).await?;

    let command = args.remove(0);
    let task = TaskDef {
        id: String::from(""),
        command,
        args,
        ..Default::default()
    };

    let (sender, receiver) = mpsc::channel(1);
    let stdout_content = tokio::spawn(collect(receiver));
    let mut artifacts = Cursor::new(vec![]);

    let status_code = run_toolchain_task(
        &docker,
        &container,
        TOOLCHAIN,
        &task,
        Some(sender),
        None,
        Some(&mut artifacts),
    )
    .await?;
    artifacts.set_position(0);

    Ok((status_code, artifacts, stdout_content.await?))
}

async fn get_sample_container(docker: &Docker) -> Result<String> {
    let mut containers = list_running_containers(&docker).await?;
    assert!(!containers.is_empty());
    Ok(containers.remove(0))
}

async fn collect<T: AsRef<[u8]>>(mut receiver: mpsc::Receiver<T>) -> String {
    let mut result = String::new();

    while let Some(chunk) = receiver.recv().await {
        result.push_str(&String::from_utf8_lossy(chunk.as_ref()));
    }
    result
}
