use crate::client::SseEvent;
use crate::errors::*;
use bytes::Bytes;
use error_chain::ChainedError;
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

    pub fn notify_all(&mut self, event: &SseEvent) -> Result<()> {
        self.clients.retain(|client| {
            if let Err(e) = Self::send_client(&client, &event) {
                eprintln!("{}", e.display_chain());

                false
            } else {
                true
            }
        });
        Ok(())
    }

    pub fn send_client(
        client: &UnboundedSender<io::Result<Bytes>>,
        event: &SseEvent,
    ) -> Result<()> {
        let (ref name, ref data) = event;
        if let Some(name) = name {
            Self::send(client, "event: ")?;
            Self::send(client, name.clone())?;
            Self::send(client, "\n")?;
        }

        Self::send(client, "data: ")?;
        Self::send(client, data.clone())?;
        Self::send(client, "\n\n")?;
        Ok(())
    }

    #[inline]
    pub fn send<T>(client: &UnboundedSender<io::Result<Bytes>>, data: T) -> Result<()>
    where
        Bytes: From<T>,
    {
        client
            .send(Ok(Bytes::from(data)))
            .chain_err(|| "Failed while send message to SSE-channel")
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
