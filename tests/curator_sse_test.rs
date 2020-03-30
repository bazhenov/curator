extern crate curator;

use curator::client::CuratorClient;
use curator::server::Curator;
use std::thread;
use tokio;
use tokio::time::delay_for;
use std::time::Duration;

#[actix_rt::test]
async fn curator_sse_client() {
    let server = Curator::start();
    let client = CuratorClient::connect("http://127.0.0.1:8080/events").await
        .expect("Unable to connect");
    println!("STARTED");

    delay_for(Duration::from_secs(5)).await;
    //server.stop().await;
    drop(client);
}
