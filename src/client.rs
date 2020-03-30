pub struct CuratorClient {
    client: Client<HttpConnector, Body>,
}
use hyper::{body::HttpBody as _, Client, Uri, client::HttpConnector, Body, Request, header};
use std::error::Error;

impl CuratorClient {
    pub async fn connect(host: &str) -> Result<Self, Box<dyn Error>> {
        let client = Client::new();
        
        let req = Request::builder()
          .method("POST")
          .uri(host)
          .header(header::ACCEPT, "text/event-stream")
          .body(Body::empty())?;

        let mut response = client.request(req).await?;

        tokio::spawn(async move {
          while let Some(chunk) = response.body_mut().data().await {
            println!("Chunk: {:?}", chunk);
          }
        });
        

        Ok(CuratorClient { client })
    }
}
