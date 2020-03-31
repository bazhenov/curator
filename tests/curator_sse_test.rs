extern crate curator;

use curator::client::SseClient;
use curator::server::Curator;
use std::error::Error;

#[actix_rt::test]
async fn curator_sse_client() -> Result<(), Box<dyn Error>> {
    let server = Curator::start()?;
    let mut client = SseClient::connect("http://127.0.0.1:8080/events")
        .await
        .expect("Unable to connect");

    if let Some(event) = client.next().await? {
        assert_eq!(event.0, Some("start-task".to_string()));
        assert_eq!(event.1, "{}");
    } else {
        panic!("No events from server");
    }

    // remove client first, otherwise server will wait for graceful shutdown
    drop(client);

    server.stop().await;
    Ok(())
}
