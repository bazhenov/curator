use crate::{client::SseEvent, errors::*, protocol};
use actix_web::{http::header::*, web, App, HttpResponse, HttpServer, Responder};
use bytes::Bytes;
use error_chain::ChainedError;
use serde;
use serde_json;
use std::collections::HashSet;
use std::io;
use std::sync::Mutex;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use uuid;

pub struct Agent {
    pub agent: protocol::AgentRef,
    pub tasks: HashSet<String>,
    channel: UnboundedSender<io::Result<Bytes>>,
}

impl Agent {
    fn send_named_event<T>(&self, name: &str, event: &T) -> Result<()>
    where
        T: serde::Serialize,
    {
        self.send_event(&(Some(name.into()), serde_json::to_string(event)?))
    }

    fn send_event(&self, event: &SseEvent) -> Result<()> {
        let (ref name, ref data) = event;
        if let Some(name) = name {
            self.send("event: ")?;
            self.send(name.clone())?;
            self.send("\n")?;
        }

        self.send("data: ")?;
        self.send(data.clone())?;
        self.send("\n\n")?;
        Ok(())
    }

    #[inline]
    fn send<T>(&self, data: T) -> Result<()>
    where
        Bytes: From<T>,
    {
        self.channel
            .send(Ok(Bytes::from(data)))
            .chain_err(|| "Failed while send message to SSE-channel")
    }
}

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
                .route("/task/run", web::post().to(run_task))
        };

        let server = HttpServer::new(app).bind("127.1:8080")?.run();

        Ok(Self { server, agents })
    }

    pub fn notify_all(&self, event: &SseEvent) {
        let mut agents = self.agents.lock().unwrap();
        agents.retain(|agent| {
            if let Err(e) = agent.send_event(&event) {
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
}

async fn new_client(
    new_agent: web::Json<protocol::agent::Agent>,
    agents: web::Data<Mutex<Vec<Agent>>>,
) -> impl Responder {
    let (tx, rx) = unbounded_channel();
    let mut agents = agents.lock().unwrap();

    let new_agent = new_agent.into_inner();
    let tasks = new_agent.tasks.iter().map(|i| i.id.clone()).collect();

    agents.push(Agent {
        agent: new_agent.into(),
        tasks: tasks,
        channel: tx,
    });

    HttpResponse::Ok()
        .header("content-type", "text/event-stream")
        .no_chunking()
        .streaming(rx)
}

async fn run_task(
    task: web::Json<protocol::client::RunTask>,
    agents: web::Data<Mutex<Vec<Agent>>>,
) -> impl Responder {
    let agents = agents.lock().unwrap();

    if let Some(agent) = agents.iter().find(|a| a.agent == task.agent) {
        let execution_id = uuid::Uuid::new_v4();

        let result = agent.send_named_event(
            "run-task",
            &protocol::agent::RunTask {
                execution: execution_id,
                task_id: task.task_id.clone(),
            },
        );
        
        if let Err(e) = result {
            eprintln!("{}", e);
            HttpResponse::InternalServerError()
                .finish()
        } else {
            HttpResponse::Ok()
                .header(CONTENT_TYPE, "application/json")
                .json(protocol::client::ExecutionRef { execution_id })
        }
    } else {
        HttpResponse::NotAcceptable().finish()
    }
}
