use crate::errors::*;
use crate::client::SseEvent;
use bytes::Bytes;
use std::io;

use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

pub struct SseBroker {
    clients: Vec<UnboundedSender<io::Result<Bytes>>>,
}

impl SseBroker {
    pub fn new() -> Self {
        SseBroker { clients: vec![] }
    }

    pub fn new_channel(
        &mut self,
    ) -> (
        UnboundedSender<io::Result<Bytes>>,
        UnboundedReceiver<io::Result<Bytes>>,
    ) {
        mpsc::unbounded_channel()
    }

    pub fn subscribe(&mut self) -> UnboundedReceiver<io::Result<Bytes>> {
        let (tx, rx) = self.new_channel();
        self.clients.push(tx);
        rx
    }

    pub fn notify_all(&self, event: &SseEvent) -> Result<()> {
        for client in &self.clients {
            let (ref name, ref data) = event;
            if let Some(name) = name {
                client.send(Ok(Bytes::from("event: ")))?;
                client.send(Ok(Bytes::from(name.clone())))?;
                client.send(Ok(Bytes::from("\n")))?;
            }
            
            client.send(Ok(Bytes::from("data: ")))?;
            client.send(Ok(Bytes::from(data.clone())))?;
            client.send(Ok(Bytes::from("\n\n")))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use futures::stream::StreamExt;
    use std::str;
    use std::thread;

    #[tokio::test]
    async fn foo() {
        let mut sse = SseBroker::new();

        let (tx, rx) = sse.new_channel();

        thread::spawn(move || {
            tx.send(Ok(Bytes::from("Hello "))).expect("Oops");
            tx.send(Ok(Bytes::from("to "))).expect("Oops");
            tx.send(Ok(Bytes::from("you"))).expect("Oops");
        });

        let string = rx
            .map(io::Result::ok)
            .map(Option::unwrap)
            .map(|bytes| str::from_utf8(&bytes[..]).unwrap().to_string())
            .collect::<String>()
            .await;
        assert_eq!(string, "Hello to you");
    }

    #[test]
    fn check_actix_compile() {
        use actix_web::web::HttpResponse;

        let mut sse = SseBroker::new();
        let (_, rx) = sse.new_channel();
        HttpResponse::Ok()
            .header("content-type", "text/event-stream")
            .no_chunking()
            .streaming(rx);
    }
}
