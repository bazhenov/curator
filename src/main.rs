use actix::prelude::*;
use actix_web::{
    http::header, http::Method, web, web::Bytes, App, Error, HttpRequest, HttpResponse, HttpServer,
    Responder,
};
use std::task::Poll;

mod sse;

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(move || App::new().route("/events", web::get().to(new_client)))
        .bind("127.1:8080")?
        .run()
        .await
}

async fn new_client(r: HttpRequest) -> impl Responder {
    use futures::stream::poll_fn;
    let mut counter: usize = 5;

    let server_events = poll_fn(move |_cx| -> Poll<Option<Result<Bytes, Error>>> {
        if counter == 0 {
            return Poll::Ready(None);
        }
        let payload = format!("data: {}\n\n", counter);
        counter -= 1;
        _cx.waker().wake_by_ref();
        Poll::Ready(Some(Ok(Bytes::from(payload))))
    });

    HttpResponse::Ok()
        .header("content-type", "text/event-stream")
        .no_chunking()
        .streaming(server_events)
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
