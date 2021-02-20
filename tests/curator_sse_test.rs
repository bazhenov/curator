extern crate curator;

use curator::agent::SseClient;
use curator::prelude::*;
use curator::protocol;
use curator::server::Curator;

use bollard::{
    container::{Config, RemoveContainerOptions},
    Docker,
};
use curator::agent::{run_docker_discovery, Toolchain};

use rstest::*;

#[actix_rt::test]
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
#[actix_rt::test]
async fn docker_discovery_test(docker: Docker) -> Result<()> {
    let toolchains = vec![Toolchain::from(("alpine", "3.12"))];

    let container = Container::start(&docker).await?;
    container.stop().await?;

    //run_docker_discovery(&docker, &toolchains).await?;

    Ok(())
}

struct Container<'a> {
    docker: &'a Docker,
    container_id: String,
}

impl<'a> Container<'a> {
    async fn start(docker: &'a Docker) -> Result<Container<'a>> {
        let config = Config {
            image: Some("openjdk:11.0-jdk"),
            cmd: Some(vec!["date"]),
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

    async fn stop(&self) -> Result<()> {
        let options = RemoveContainerOptions {
            force: true,
            ..Default::default()
        };
        self.docker
            .remove_container(&self.container_id, Some(options))
            .await?;
        Ok(())
    }
}

impl<'a> Drop for Container<'a> {
    fn drop(&mut self) {
        // let options = RemoveContainerOptions {
        //     force: true,
        //     ..Default::default()
        // };
        // let future = self.docker
        //     .remove_container(&self.container_id, Some(options));
        // block_on(future).expect("Unable to stop container");
    }
}
