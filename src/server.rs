use std::collections::{HashMap, HashSet};
use std::io;
use std::sync::Mutex;

use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use bytes::Bytes;
use error_chain::ChainedError;
use serde;
use serde_json;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use uuid::Uuid;

use crate::{agent::SseEvent, errors::*, protocol::*};

pub struct Agent {
    pub agent: AgentRef,
    pub tasks: HashSet<Task>,
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

#[derive(Default)]
struct Executions(HashMap<Uuid, client::Execution>);

pub struct Curator {
    server: actix_server::Server,
    pub agents: web::Data<Mutex<Vec<Agent>>>,
}

impl Curator {
    pub fn start() -> Result<Self> {
        let agents = web::Data::new(Mutex::new(vec![]));
        let executions = web::Data::new(Mutex::new(Executions::default()));

        let app = {
            let agents = agents.clone();
            move || {
                App::new()
                    .app_data(agents.clone())
                    .app_data(executions.clone())
                    .route("/events", web::post().to(new_client))
                    .route("/task/run", web::post().to(run_task))
                    .route("/execution/report", web::post().to(report_task))
                    .route("/agents", web::get().to(list_agents))
                    .route("/executions", web::get().to(list_executions))
            }
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
    new_agent: web::Json<agent::Agent>,
    agents: web::Data<Mutex<Vec<Agent>>>,
) -> impl Responder {
    let (tx, rx) = unbounded_channel();
    let mut agents = agents.lock().unwrap();

    let new_agent = new_agent.into_inner();
    let tasks = new_agent.tasks.iter().cloned().collect();

    agents.push(Agent {
        agent: new_agent.into(),
        channel: tx,
        tasks,
    });

    HttpResponse::Ok()
        .header("content-type", "text/event-stream")
        .no_chunking()
        .streaming(rx)
}

async fn report_task(
    mut report: web::Json<agent::ExecutionReport>,
    executions: web::Data<Mutex<Executions>>,
) -> impl Responder {
    let mut executions = executions.lock().unwrap();

    if let Some(execution) = executions.0.get_mut(&report.id) {
        if let Some(lines) = report.stdout_append.take() {
            execution.output = format!("{}\n{}", execution.output, lines);
        }
    }
    HttpResponse::Ok().finish()
}

async fn list_executions(executions: web::Data<Mutex<Executions>>) -> impl Responder {
    let executions = executions.lock().unwrap();

    let body = executions.0.values().cloned().collect::<Vec<_>>();
    HttpResponse::Ok().json(body)
}

async fn list_agents(agents: web::Data<Mutex<Vec<Agent>>>) -> impl Responder {
    let agents = agents.lock().unwrap();

    let agents = agents
        .iter()
        .map(|a| agent::Agent {
            application: a.agent.application.clone(),
            instance: a.agent.instance.clone(),
            tasks: a.tasks.iter().cloned().collect(),
        })
        .collect::<Vec<_>>();

    HttpResponse::Ok().json(agents)
}

async fn run_task(
    task: web::Json<client::RunTask>,
    agents: web::Data<Mutex<Vec<Agent>>>,
    executions: web::Data<Mutex<Executions>>,
) -> impl Responder {
    let mut agents = agents.lock().unwrap();

    let choosed_agent = agents
        .iter()
        .enumerate()
        .find(|(_, a)| a.agent == task.agent);

    if let Some((idx, agent)) = choosed_agent {
        let execution_id = Uuid::new_v4();

        let result = agent.send_named_event(
            "run-task",
            &agent::RunTask {
                execution: execution_id,
                task_id: task.task_id.clone(),
            },
        );

        if let Err(e) = result {
            eprintln!("{}", e);
            agents.swap_remove(idx);
            HttpResponse::InternalServerError().finish()
        } else {
            let mut executions = executions.lock().unwrap();
            executions.0.insert(
                execution_id,
                client::Execution::new(execution_id, agent.agent.clone()),
            );
            HttpResponse::Ok().json(client::ExecutionRef { execution_id })
        }
    } else {
        HttpResponse::NotAcceptable().finish()
    }
}
