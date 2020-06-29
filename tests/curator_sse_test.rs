extern crate curator;

use curator::agent::SseClient;
use curator::prelude::*;
use curator::protocol;
use curator::server::Curator;

#[actix_rt::test]
async fn curator_sse_client() -> Result<()> {
    let server = Curator::start()?;
    let agent = protocol::agent::Agent::new("app", "instance", vec![]);
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
