use bytes::Bytes;
use std::io::Result;

use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

pub struct SseBroker {
  clients: Vec<UnboundedSender<Result<Bytes>>>
}

impl SseBroker {
    pub fn new() -> Self {
        SseBroker {
            clients: vec![]
        }
    }
    
    pub fn new_channel(
        &mut self,
    ) -> (
        UnboundedSender<Result<Bytes>>,
        UnboundedReceiver<Result<Bytes>>,
    ) {
        mpsc::unbounded_channel()
    }

    pub fn subscribe(&mut self) -> UnboundedReceiver<Result<Bytes>> {
        let (tx, rx) = self.new_channel();
        self.clients.push(tx);
        rx
    }

    pub fn notify_all(&self) {
        for client in &self.clients {
            #![allow(warnings)]
            client.send(Ok(Bytes::from("event: ")));
            client.send(Ok(Bytes::from("Data")));
            client.send(Ok(Bytes::from("\n\n")));
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use std::str;
    use futures::stream::StreamExt;
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
            .map(Result::ok)
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
