extern crate curator;

use curator::client::SseClient;
use curator::server::Curator;
use std::error::Error;
use std::thread;
use std::time::Duration;
use tokio;
use tokio::time::delay_for;

#[actix_rt::test]
async fn curator_sse_client() -> Result<(), Box<dyn Error>> {
    let server = Curator::start()?;
    let mut client = SseClient::connect("http://127.0.0.1:8080/events")
        .await
        .expect("Unable to connect");
    println!("STARTED");

    let mut i = 0;
    while let Some(chunk) = client.next().await {
        let chunk = chunk?;
        i += 1;
        if i >= 5 {
            break;
        }
        println!("CHUNK: {:?}", chunk);
    }

    drop(client);
    server.stop().await;
    Ok(())
}
