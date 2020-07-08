use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::{self, Write},
    path::Path,
    sync::Mutex,
};

use actix_files::NamedFile;

use actix_web::{
    error::PayloadError, http::header::*, web, App, HttpResponse, HttpServer, Responder,
};
use bytes::Bytes;
use futures::StreamExt;
use serde::Deserialize;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use uuid::Uuid;

use crate::{agent::SseEvent, prelude::*, protocol::*};
use chrono::prelude::*;

#[derive(Error, Debug)]
enum ServerError {
    #[error("Can't read payload from HTTP request")]
    Payload(PayloadError),
}

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
            self.send("event:")?;
            self.send(name.clone())?;
            self.send("\n")?;
        }

        self.send("data:")?;
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
            .context("Failed while send message to SSE-channel")
            .map_err(Into::into)
    }
}

#[derive(Default)]
struct Executions(HashMap<Uuid, client::Execution>);

pub struct Curator {
    server: actix_server::Server,
    pub agents: web::Data<Mutex<HashMap<AgentRef, Agent>>>,
}

impl Curator {
    pub fn start() -> Result<Self> {
        let agents = web::Data::new(Mutex::new(HashMap::new()));
        let executions = web::Data::new(Mutex::new(Executions::default()));

        let app = {
            let agents = agents.clone();
            move || {
                App::new()
                    .app_data(agents.clone())
                    .app_data(executions.clone())
                    .route("/backend/events", web::post().to(new_agent))
                    .route("/backend/task/run", web::post().to(run_task))
                    .route("/backend/execution/report", web::post().to(report_task))
                    .route(
                        "/backend/execution/attach",
                        web::post().to(attach_artifacts),
                    )
                    .route("/backend/agents", web::get().to(list_agents))
                    .route("/backend/executions", web::get().to(list_executions))
                    .route(
                        "/backend/artifacts/{id}.tar.gz",
                        web::get().to(download_artifact),
                    )
            }
        };

        let server = HttpServer::new(app)
            .shutdown_timeout(1)
            .bind("0.0.0.0:8080")?
            .run();

        Ok(Self { server, agents })
    }

    pub fn notify_all(&self, event: &SseEvent) {
        let mut agents = self.agents.lock().unwrap();
        agents.retain(|_, agent| {
            if let Err(e) = agent.send_event(&event) {
                error!("{}", e);

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

async fn new_agent(
    new_agent: web::Json<agent::Agent>,
    agents: web::Data<Mutex<HashMap<AgentRef, Agent>>>,
) -> impl Responder {
    let (tx, rx) = unbounded_channel();
    let mut agents = agents.lock().unwrap();

    let new_agent = new_agent.into_inner();
    let tasks = new_agent.tasks.iter().cloned().collect();

    let agent = Agent {
        agent: new_agent.into(),
        channel: tx,
        tasks,
    };

    info!(
        "New agent connected: {}@{}",
        agent.agent.instance, agent.agent.application
    );
    agents.insert(agent.agent.clone(), agent);

    HttpResponse::Ok()
        .header("Content-Type", "text/event-stream")
        .header("Cache-Control", "no-cache")
        // X-Accel-Buffering required for disable buffering on a nginx reverse proxy
        .header("X-Accel-Buffering", "no")
        .no_chunking()
        .streaming(rx)
}

async fn report_task(
    mut report: web::Json<agent::ExecutionReport>,
    executions: web::Data<Mutex<Executions>>,
) -> impl Responder {
    let mut executions = executions.lock().unwrap();

    if let Some(execution) = executions.0.get_mut(&report.id) {
        if report.status.is_terminal() && !execution.status.is_terminal() {
            execution.finished = Some(Utc::now());
        }
        execution.status = report.status;
        if let Some(lines) = report.stdout_append.take() {
            execution.output.push_str(&lines);
        }
    }
    HttpResponse::Ok().finish()
}

#[derive(Deserialize)]
struct ArtifactParams {
    id: Uuid,
}

async fn attach_artifacts(
    payload: web::Payload,
    query: web::Query<ArtifactParams>,
    request: web::HttpRequest,
) -> impl Responder {
    let content_type = request.headers().get(CONTENT_TYPE).map(HeaderValue::to_str);
    match content_type {
        Some(Ok("application/x-tgz")) => {
            let file_name = format!("./{}.tar.gz", query.id);
            if write_artifact_to_file(payload, file_name).await.is_ok() {
                HttpResponse::Ok().finish()
            } else {
                HttpResponse::InternalServerError().finish()
            }
        }
        _ => HttpResponse::NotAcceptable().finish(),
    }
}

async fn download_artifact(query: web::Path<ArtifactParams>) -> IoResult<NamedFile> {
    let file_name = format!("./{}.tar.gz", query.id);
    let path = Path::new(&file_name);

    NamedFile::open(path)
}

async fn write_artifact_to_file(
    mut payload: web::Payload,
    file_name: impl AsRef<Path>,
) -> Result<()> {
    let mut file = File::create(file_name)?;
    while let Some(chunk) = payload.next().await {
        let bytes: Bytes = chunk.map_err(ServerError::Payload)?;
        file.write_all(&bytes)?;
    }
    Ok(())
}

async fn list_executions(executions: web::Data<Mutex<Executions>>) -> impl Responder {
    let executions = executions.lock().unwrap();

    let body = executions.0.values().cloned().collect::<Vec<_>>();
    HttpResponse::Ok().json(body)
}

async fn list_agents(agents: web::Data<Mutex<HashMap<AgentRef, Agent>>>) -> impl Responder {
    let agents = agents.lock().unwrap();

    let agents = agents
        .values()
        .map(|a| agent::Agent {
            application: a.agent.application.clone(),
            instance: a.agent.instance.clone(),
            tasks: a.tasks.iter().cloned().collect(),
        })
        .collect::<Vec<_>>();

    HttpResponse::Ok().json(agents)
}

async fn run_task(
    run_task: web::Json<client::RunTask>,
    agents: web::Data<Mutex<HashMap<AgentRef, Agent>>>,
    executions: web::Data<Mutex<Executions>>,
) -> impl Responder {
    let mut agents = agents.lock().unwrap();

    if let Some(agent) = agents.get(&run_task.agent) {
        let chosen_task = agent.tasks.iter().find(|t| t.id == run_task.task_id);

        if let Some(task) = chosen_task {
            let execution_id = Uuid::new_v4();
            let result = agent.send_named_event(
                agent::RUN_TASK_EVENT_NAME,
                &agent::RunTask {
                    execution: execution_id,
                    task_id: run_task.task_id.clone(),
                },
            );

            if let Err(e) = result {
                error!(
                    "Failed while sending message to an agent. Removing agent. {}",
                    e
                );
                agents.remove(&run_task.agent);
                HttpResponse::InternalServerError().finish()
            } else {
                let mut executions = executions.lock().unwrap();
                executions.0.insert(
                    execution_id,
                    client::Execution::new(execution_id, task.clone(), agent.agent.clone()),
                );
                HttpResponse::Ok().json(client::ExecutionRef { execution_id })
            }
        } else {
            HttpResponse::NotAcceptable().finish()
        }
    } else {
        HttpResponse::NotAcceptable().finish()
    }
}
