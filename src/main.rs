use actix::prelude::*;
use actix_web::{
    http::header, http::Method, web, web::Bytes, App, Error, HttpRequest, HttpResponse, HttpServer,
    Responder,
};
use std::task::Poll;
use std::sync::Mutex;
use std::thread;

mod sse;

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    let broker = Mutex::new(sse::SseBroker::new());
    let broker_data = web::Data::new(broker);

    let background_broker_ref = broker_data.clone();

    thread::spawn(move || {
        loop {
            thread::sleep_ms(1000);
            background_broker_ref.lock().unwrap().notify_all();
        }
    });

    HttpServer::new(move || App::new()
            .app_data(broker_data.clone())
            .route("/events", web::post().to(new_client)))
        .bind("127.1:8080")?
        .run()
        .await
}

async fn new_client(r: HttpRequest, sse: web::Data<Mutex<sse::SseBroker>>) -> impl Responder {
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
