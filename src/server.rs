use crate::sse;
use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use std::error::Error;
use std::sync::Mutex;
use std::{thread, time::Duration};

pub struct Curator {
    server: actix_server::Server,
}

impl Curator {
    pub fn start() -> Result<Self, Box<dyn Error>> {
        let broker = Mutex::new(sse::SseBroker::new());
        let broker_data = web::Data::new(broker);

        let background_broker_ref = broker_data.clone();

        thread::spawn(move || loop {
            thread::sleep(Duration::from_secs(1));
            background_broker_ref.lock().unwrap().notify_all();
        });
        let app = move || {
            App::new()
                .app_data(broker_data.clone())
                .route("/events", web::post().to(new_client))
        };

        let server = HttpServer::new(app).bind("127.1:8080")?.run();

        Ok(Self { server })
    }

    pub async fn stop(&self) {
        self.server.stop(true).await
    }
}

async fn new_client(sse: web::Data<Mutex<sse::SseBroker>>) -> impl Responder {
    let rx = sse.lock().unwrap().subscribe();

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
