use crate::{client::SseEvent, errors::*, sse};
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use std::sync::Mutex;

pub struct Curator {
    server: actix_server::Server,
    sse: web::Data<Mutex<sse::SseBroker>>,
}

impl Curator {
    pub fn start() -> Result<Self> {
        let broker = Mutex::new(sse::SseBroker::new());
        let broker_data = web::Data::new(broker);
        let sse = broker_data.clone();

        let app = move || {
            App::new()
                .app_data(broker_data.clone())
                .route("/events", web::post().to(new_client))
        };

        let server = HttpServer::new(app).bind("127.1:8080")?.run();

        Ok(Self { server, sse })
    }

    pub fn notify_all(&self, event: &SseEvent) {
        self.sse.lock().unwrap().notify_all(event);
    }

    pub async fn stop(&self, graceful: bool) {
        self.server.stop(graceful).await
    }
}

async fn new_client(req: HttpRequest, sse: web::Data<Mutex<sse::SseBroker>>) -> impl Responder {
    let rx = sse.lock().unwrap().subscribe();
    println!("HOST IS: {host:?}", host = req.connection_info().remote());

    HttpResponse::Ok()
        .header("content-type", "text/event-stream")
        .no_chunking()
        .streaming(rx)
}

#[cfg(test)]
mod tests {

    #[test]
    fn foo() {}
}
