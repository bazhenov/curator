extern crate curator;

use curator::agent::SseClient;
use curator::prelude::*;
use curator::protocol;
use curator::server::Curator;

use bollard::Docker;
use curator::agent::docker::run_toolchain_discovery;

use rstest::*;

#[tokio::test]
async fn curator_sse_client() -> Result<()> {
    let server = Curator::start()?;
    let agent = protocol::agent::Agent::new("app", vec![]);
    {
        let mut client = SseClient::connect("http://127.0.0.1:8080/backend/events", agent).await?;

        let event_name = Some("run-task".to_string());
        let event_content = "{}".to_string();
        let expected_event = (event_name, event_content);
        server.notify_all(&expected_event);

        if let Some(event) = client.next_event().await? {
            assert_eq!(event, expected_event);
        } else {
            panic!("No events from server");
        }
        // remove client first, otherwise server will wait for graceful shutdown
    }

    server.stop(true).await;
    Ok(())
}

#[fixture]
fn docker() -> Docker {
    Docker::connect_with_unix_defaults().expect("Unable to get Docker instance")
}

#[rstest]
#[tokio::test]
async fn docker_discovery_test(docker: Docker) -> Result<()> {
    let toolchain = "bazhenov.me/curator/toolchain-example:dev";

    // let container = Container::start(&docker, "openjdk:11.0-jdk", Some(vec!["date"])).await?;
    // container.check_status_code_and_remove().await?;

    let task_defs = run_toolchain_discovery(&docker, "", toolchain).await?;
    assert!(task_defs.len() > 0);

    Ok(())
}
