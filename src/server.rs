use crate::{client::SseEvent, errors::*, protocol};
use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use bytes::Bytes;
use error_chain::ChainedError;
use std::collections::HashSet;
use std::io;
use std::sync::Mutex;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

pub struct Curator {
    server: actix_server::Server,
    pub agents: web::Data<Mutex<Vec<Agent>>>,
}

impl Curator {
    pub fn start() -> Result<Self> {
        let agents = web::Data::new(Mutex::new(vec![]));
        let agent_clone = agents.clone();

        let app = move || {
            App::new()
                .app_data(agent_clone.clone())
                .route("/events", web::post().to(new_client))
        };

        let server = HttpServer::new(app).bind("127.1:8080")?.run();

        Ok(Self { server, agents })
    }

    pub fn notify_all(&self, event: &SseEvent) {
        let mut agents = self.agents.lock().unwrap();
        agents.retain(|agent| {
            if let Err(e) = Self::send_client(&agent.channel, &event) {
                eprintln!("{}", e.display_chain());

                false
            } else {
                true
            }
        })
    }

    pub async fn stop(&self, graceful: bool) {
        self.server.stop(graceful).await
    }

    fn send_client(client: &UnboundedSender<io::Result<Bytes>>, event: &SseEvent) -> Result<()> {
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
    fn send<T>(client: &UnboundedSender<io::Result<Bytes>>, data: T) -> Result<()>
    where
        Bytes: From<T>,
    {
        client
            .send(Ok(Bytes::from(data)))
            .chain_err(|| "Failed while send message to SSE-channel")
    }
}

async fn new_client(
    new_agent: web::Json<protocol::Agent>,
    agents: web::Data<Mutex<Vec<Agent>>>,
) -> impl Responder {
    let (tx, rx) = unbounded_channel();
    let mut agents = agents.lock().unwrap();

    let new_agent = new_agent.into_inner();

    agents.push(Agent {
        application: new_agent.application,
        instance: new_agent.instance,
        tasks: new_agent.tasks.into_iter().map(|i| i.id).collect(),
        channel: tx,
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
    pub channel: UnboundedSender<io::Result<Bytes>>,
}
