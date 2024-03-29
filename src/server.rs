use std::{
    collections::HashMap,
    fs::File,
    io::{self, Write},
    path::Path,
    sync::Mutex,
    time::{Duration, Instant},
};

use actix_files::NamedFile;

use actix_web::{
    error::PayloadError,
    error::ResponseError,
    http::header::{self, HeaderValue},
    http::StatusCode,
    middleware::Logger,
    web, App, HttpResponse, HttpResponseBuilder, HttpServer, Responder,
};
use bytes::Bytes;
use futures::{
    channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender},
    StreamExt,
};
use serde::Deserialize;
use tokio::time::sleep;
use uuid::Uuid;

use crate::{agent::SseEvent, prelude::*, protocol::*};
use chrono::prelude::*;

#[derive(Error, Debug)]
enum ServerError {
    #[error("Can't read payload from HTTP request")]
    Payload(PayloadError),

    #[error("Unable to create artifact file")]
    UnableToCreateArtifact(io::Error),

    #[error("Execution not found: {}", .0)]
    ExecutionNotFound(Uuid),

    #[error("Invalid artifact type. Only application/x-tgz is supported")]
    InvalidArtifactType,
}

impl ResponseError for ServerError {
    fn error_response(&self) -> HttpResponse {
        HttpResponseBuilder::new(self.status_code())
            .insert_header(header::ContentType::plaintext())
            .body(format!("{}", self))
    }

    fn status_code(&self) -> StatusCode {
        use ServerError::*;

        match self {
            Payload(_) => StatusCode::INTERNAL_SERVER_ERROR,
            UnableToCreateArtifact(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ExecutionNotFound(_) => StatusCode::CONFLICT,
            InvalidArtifactType => StatusCode::NOT_ACCEPTABLE,
        }
    }
}

/// Controller specific Result-type. Error type can be turned into HTTP error
type ServerResult<T> = std::result::Result<T, ServerError>;
type AgentMap = HashMap<String, Agent>;
type Shared<T> = web::Data<Mutex<T>>;

fn shared<T>(obj: T) -> Shared<T> {
    web::Data::new(Mutex::new(obj))
}

struct Agent {
    info: agent::Agent,
    tx: UnboundedSender<io::Result<Bytes>>,
    // last heartbeat timestamp
    hb: Instant,
}

impl Agent {
    fn new(info: agent::Agent) -> (Self, UnboundedReceiver<io::Result<Bytes>>) {
        let (tx, rx) = unbounded();
        (
            Self {
                info,
                tx,
                hb: Instant::now(),
            },
            rx,
        )
    }

    fn find_task(&self, name: &str, container_id: &str) -> Option<&Task> {
        self.info
            .tasks
            .iter()
            .find(|t| t.name == name && t.container_id == container_id)
    }

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

    /// Updates heartbeat time on an agent
    ///
    /// Hearbeats required to remove stalled agents because SSE doesn't allow to track client disconnection
    /// without sending messages to client. Moreover even sending messages to client doesn't provide time bounds
    /// for stale agent detection because local TCP stack can buffer outgoing messages for quite a while.
    fn heartbeat_recevied(&mut self) {
        self.hb = Instant::now();
    }

    #[inline]
    fn send<T>(&self, data: T) -> Result<()>
    where
        Bytes: From<T>,
    {
        self.tx
            .unbounded_send(Ok(Bytes::from(data)))
            .context("Failed while send message to SSE-channel")
            .map_err(Into::into)
    }
}

#[derive(Default)]
struct Executions(HashMap<Uuid, client::Execution>);

const CLEANUP_INTERVAL_SEC: u64 = 2;
const HEARTBEAT_TIMEOUT_SEC: u64 = 6;

pub struct Curator {
    agents: Shared<AgentMap>,
}

impl Curator {
    pub fn start() -> Result<(actix_server::Server, Self)> {
        let agents: AgentMap = HashMap::new();
        let agents = shared(agents);
        let executions = shared(Executions::default());

        let app = {
            let agents = agents.clone();
            move || {
                // We need custom JSON config because `/backend/events` input payloads
                // can be quite large in size (imagine 1000+ tasks on a host node)
                let json_config = web::JsonConfig::default().limit(8_000_000);

                App::new()
                    .wrap(Logger::default())
                    .app_data(agents.clone())
                    .app_data(executions.clone())
                    .app_data(json_config)
                    .route("/backend/events", web::post().to(agent_connected))
                    .route("/backend/hb", web::post().to(agent_heartbeat))
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

        {
            let agents = agents.clone();
            tokio::spawn(cleanup_agents(agents));
        }

        Ok((server, Self { agents }))
    }

    pub fn notify_all(&self, event: &SseEvent) {
        let mut agents = self.agents.lock().unwrap();
        agents.retain(|_, agent| {
            if let Err(e) = agent.send_event(event) {
                error!("{}", e);
                false
            } else {
                true
            }
        })
    }
}

/// Remove stale agents
///
/// Stae agents are agents which doesn't confirm presence using heartbeat in a predefined
/// timeout
async fn cleanup_agents(agents: Shared<AgentMap>) {
    loop {
        sleep(Duration::from_secs(CLEANUP_INTERVAL_SEC)).await;
        trace!("Running cleanup...");
        if let Ok(mut agents) = agents.lock() {
            agents.retain(|_, agent| agent.hb.elapsed().as_secs() < HEARTBEAT_TIMEOUT_SEC);
        }
    }
}

async fn agent_heartbeat(
    agent: web::Json<agent::Agent>,
    agents: web::Data<Mutex<AgentMap>>,
) -> impl Responder {
    let mut agents = agents.lock().unwrap();
    if let Some(agent) = agents.get_mut(&agent.name) {
        agent.heartbeat_recevied();
    }
    HttpResponse::Ok()
}

async fn agent_connected(
    new_agent: web::Json<agent::Agent>,
    agents: web::Data<Mutex<AgentMap>>,
) -> impl Responder {
    let mut agents = agents.lock().unwrap();

    let new_agent = new_agent.into_inner();

    let (agent, rx) = Agent::new(new_agent);

    info!("New agent connected: {}", agent.info.name);
    agents.insert(agent.info.name.clone(), agent);

    HttpResponse::Ok()
        .content_type("text/event-stream")
        .insert_header(("Cache-Control", "no-cache"))
        // X-Accel-Buffering required for disable buffering on a nginx reverse proxy
        .insert_header(("X-Accel-Buffering", "no"))
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
    executions: web::Data<Mutex<Executions>>,
) -> ServerResult<HttpResponse> {
    let content_type = request
        .headers()
        .get(header::CONTENT_TYPE)
        .map(HeaderValue::to_str);
    use ServerError::*;

    match content_type {
        Some(Ok("application/x-tgz")) => {
            let mut executions = executions.lock().unwrap();

            if let Some(execution) = executions.0.get_mut(&query.id) {
                let file_name = format!("./{}.tar.gz", query.id);
                let bytes_written = save_artifact(payload, file_name).await?;
                execution.artifact_size = Some(bytes_written);
                debug!(
                    "New artifact uploaded. Execution: {}, size: {}",
                    query.id, bytes_written
                );
                Ok(HttpResponse::Ok().finish())
            } else {
                Err(ExecutionNotFound(query.id))
            }
        }
        _ => Err(InvalidArtifactType),
    }
}

async fn download_artifact(query: web::Path<ArtifactParams>) -> IoResult<NamedFile> {
    let file_name = format!("./{}.tar.gz", query.id);
    let path = Path::new(&file_name);

    NamedFile::open(path)
}

async fn save_artifact(
    mut payload: web::Payload,
    file_name: impl AsRef<Path>,
) -> ServerResult<usize> {
    use ServerError::*;

    let mut file = File::create(file_name).map_err(UnableToCreateArtifact)?;
    let mut bytes_written = 0;
    while let Some(chunk) = payload.next().await {
        let bytes = chunk.map_err(ServerError::Payload)?;
        file.write_all(&bytes).map_err(UnableToCreateArtifact)?;
        bytes_written += bytes.len();
    }
    Ok(bytes_written)
}

async fn list_executions(executions: web::Data<Mutex<Executions>>) -> impl Responder {
    let executions = executions.lock().unwrap();

    let body = executions.0.values().cloned().collect::<Vec<_>>();
    HttpResponse::Ok().json(body)
}

async fn list_agents(agents: web::Data<Mutex<AgentMap>>) -> impl Responder {
    let agents = agents.lock().unwrap();

    let agents = agents.values().map(|a| a.info.clone()).collect::<Vec<_>>();

    HttpResponse::Ok().json(agents)
}

async fn run_task(
    run_task: web::Json<client::RunTask>,
    agents: web::Data<Mutex<AgentMap>>,
    executions: web::Data<Mutex<Executions>>,
) -> impl Responder {
    let mut agents = agents.lock().unwrap();

    if let Some(agent) = agents.get(&run_task.agent) {
        if let Some(task) = agent.find_task(&run_task.task_name, &run_task.container_id) {
            let execution_id = Uuid::new_v4();
            let result = agent.send_named_event(
                agent::RUN_TASK_EVENT_NAME,
                &agent::RunTask {
                    execution_id,
                    container_id: run_task.container_id.clone(),
                    task_name: run_task.task_name.clone(),
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
                    client::Execution::new(execution_id, task.clone(), run_task.agent.clone()),
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
