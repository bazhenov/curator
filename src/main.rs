extern crate curator;
use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use curator::sse;
use std::sync::Mutex;
use std::task::Poll;
use std::{thread, time::Duration};

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    let broker = Mutex::new(sse::SseBroker::new());
    let broker_data = web::Data::new(broker);

    let background_broker_ref = broker_data.clone();

    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(1));
        let event = (Some("run-task".to_string()), "{}".to_string());
        background_broker_ref
            .lock()
            .unwrap()
            .notify_all(&event)
            .unwrap();
    });

    HttpServer::new(move || {
        App::new()
            .app_data(broker_data.clone())
            .route("/events", web::post().to(new_client))
    })
    .bind("127.1:8080")?
    .run()
    .await
}

async fn new_client(sse: web::Data<Mutex<sse::SseBroker>>) -> impl Responder {
    let rx = sse.lock().unwrap().subscribe();

    HttpResponse::Ok()
        .header("content-type", "text/event-stream")
        .no_chunking()
        .streaming(rx)
}

#[derive(Default)]
struct MyFuture {
    count: u32,
}

use std::pin::Pin;

impl std::future::Future for MyFuture {
    type Output = i32;

    fn poll(mut self: Pin<&mut Self>, ctx: &mut std::task::Context) -> Poll<Self::Output> {
        match self.count {
            3 => Poll::Ready(3),
            _ => {
                self.count += 1;
                ctx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test]
    async fn test_a() {
        let f = MyFuture { count: 0 };
        assert_eq!(f.await, 3);
    }
}
