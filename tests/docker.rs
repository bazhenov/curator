extern crate curator;

use curator::prelude::*;

use bollard::Docker;
use curator::docker::{list_running_containers, run_toolchain_discovery};

use rstest::*;

#[fixture]
fn docker() -> Docker {
    Docker::connect_with_unix_defaults().expect("Unable to get Docker instance")
}

#[rstest]
#[tokio::test]
async fn list_containers(docker: Docker) -> Result<()> {
    let containers = list_running_containers(&docker).await?;

    assert!(containers.len() > 0);
    Ok(())
}

#[rstest]
#[tokio::test]
async fn docker_discovery_test(docker: Docker) -> Result<()> {
    let toolchain = "bazhenov.me/curator/toolchain-example:dev";

    let task_defs = run_toolchain_discovery(&docker, "", toolchain).await?;
    assert!(task_defs.len() > 0);

    Ok(())
}
