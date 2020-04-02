use crate::{client::SseEvent, errors::*, sse, protocol};
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use bytes::Bytes;
use std::collections::HashSet;
use std::io;
use std::sync::Mutex;
use tokio::sync::mpsc::UnboundedSender;

pub struct Curator {
    server: actix_server::Server,
    sse: web::Data<Mutex<sse::SseBroker>>,
    pub agents: web::Data<Mutex<Vec<Agent>>>,
}

impl Curator {
    pub fn start() -> Result<Self> {
        let broker = Mutex::new(sse::SseBroker::new());
        let broker_data = web::Data::new(broker);
        let sse = broker_data.clone();

        let agents = web::Data::new(Mutex::new(vec![]));
        let agent_clone = agents.clone();

        let app = move || {
            App::new()
                .app_data(broker_data.clone())
                .app_data(agent_clone.clone())
                .route("/events", web::post().to(new_client))
        };

        let server = HttpServer::new(app).bind("127.1:8080")?.run();

        Ok(Self { server, sse, agents })
    }

    pub fn notify_all(&self, event: &SseEvent) {
        self.sse.lock().unwrap().notify_all(event);
    }

    pub async fn stop(&self, graceful: bool) {
        self.server.stop(graceful).await
    }
}

async fn new_client(
    sse: web::Data<Mutex<sse::SseBroker>>,
    new_agent: web::Json<protocol::Agent>,
    agents: web::Data<Mutex<Vec<Agent>>>,
) -> impl Responder {
    let rx = sse.lock().unwrap().subscribe();
    let mut agents = agents.lock().unwrap();

    let new_agent = new_agent.into_inner();

    agents.push(Agent {
        application: new_agent.application,
        instance: new_agent.instance,
        tasks: new_agent.tasks.into_iter().map(|i| i.id).collect()
    });
    

    HttpResponse::Ok()
        .header("content-type", "text/event-stream")
        .no_chunking()
        .streaming(rx)
}

pub struct Agent {
    pub application: String,
    pub instance: String,
    pub tasks: HashSet<String>,
    //pub channel: UnboundedSender<io::Result<Bytes>>,
}
