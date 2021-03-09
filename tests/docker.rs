extern crate curator;

use curator::prelude::*;

use bollard::Docker;
use curator::{
    agent::TaskDef,
    docker::{list_running_containers, run_toolchain_discovery, run_toolchain_task},
};
use tokio::sync::mpsc;

use rstest::*;

const TOOLCHAIN: &str = "bazhenov.me/curator/toolchain-example:dev";

#[fixture]
fn docker() -> Docker {
    Docker::connect_with_unix_defaults().expect("Unable to get Docker instance")
}

/// This test relies on docker-compose `it-sample-container` service container.
///
/// For this test to work properly on host machine you need to start sample container first:
/// ```
/// docker-compose run it-sample-container
/// ```
#[rstest]
#[tokio::test]
async fn list_containers_and_run_discovery(docker: Docker) -> Result<()> {
    let containers = list_running_containers(&docker).await?;
    assert!(!containers.is_empty());

    let task_defs = run_toolchain_discovery(&docker, &containers[0], TOOLCHAIN).await?;

    assert_eq!(task_defs.len(), 1);

    Ok(())
}

#[rstest]
#[tokio::test]
async fn run_toolchain(docker: Docker) -> Result<()> {
    let containers = list_running_containers(&docker).await?;
    assert!(!containers.is_empty());

    let command = String::from("sh");
    let args = vec!["-c", "echo 'Hello'"]
        .into_iter()
        .map(String::from)
        .collect();
    let task = TaskDef {
        id: String::from(""),
        command,
        args,
        ..Default::default()
    };

    let (sender, receiver) = mpsc::channel(1);
    let stdout_content = tokio::spawn(collect(receiver));

    let (status, _) = run_toolchain_task(
        &docker,
        &containers[0],
        TOOLCHAIN,
        &task,
        Some(sender),
        None,
    )
    .await?;

    assert_eq!(status, 0);
    assert_eq!("Hello\n", stdout_content.await?);

    Ok(())
}

async fn collect<T: AsRef<[u8]>>(mut receiver: mpsc::Receiver<T>) -> String {
    let mut result = String::new();

    while let Some(chunk) = receiver.recv().await {
        result.push_str(&String::from_utf8_lossy(chunk.as_ref()));
    }
    result
}
