extern crate curator;

use curator::prelude::*;

use bollard::Docker;
use curator::{
    agent::TaskDef,
    docker::{list_running_containers, run_toolchain_discovery, run_toolchain_task},
};

use rstest::*;

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

    let toolchain = "bazhenov.me/curator/toolchain-example:dev";
    let task_defs = run_toolchain_discovery(&docker, &containers[0], toolchain).await?;

    assert_eq!(task_defs.len(), 1);

    Ok(())
}

#[rstest]
#[tokio::test]
async fn run_toolchain(docker: Docker) -> Result<()> {
    let containers = list_running_containers(&docker).await?;
    assert!(!containers.is_empty());

    let toolchain = "bazhenov.me/curator/toolchain-example:dev";
    let task = TaskDef {
        id: "".into(),
        command: "date".into(),
        ..Default::default()
    };
    let (status, artifacts) =
        run_toolchain_task(&docker, &containers[0], toolchain, &task, None, None).await?;

    assert_eq!(status, 0);

    Ok(())
}
