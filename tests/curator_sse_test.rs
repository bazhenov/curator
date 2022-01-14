extern crate curator;

use curator::agent::SseClient;
use curator::prelude::*;
use curator::protocol;
use curator::server::Curator;

#[actix_rt::test]
async fn curator_sse_client() -> Result<()> {
    let (acceptor, curator) = Curator::start()?;
    let handle = acceptor.handle();
    tokio::spawn(acceptor);

    let agent = protocol::agent::Agent::new("app", vec![]);
    let mut client = SseClient::connect("http://127.0.0.1:8080/backend/events", agent).await?;

    let event_name = Some("run-task".to_string());
    let event_content = "{}".to_string();
    let expected_event = (event_name, event_content);
    curator.notify_all(&expected_event);

    if let Some(event) = client.next_event().await? {
        assert_eq!(event, expected_event);
    } else {
        panic!("No events from server");
    }
    handle.stop(false).await;
    Ok(())
}
