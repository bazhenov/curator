use crate::{docker::run_toolchain_task, prelude::*};
use bollard::Docker;
use futures::{future::FutureExt, select};
use hyper::{
    body::{Bytes, HttpBody as _},
    client::{Client, HttpConnector},
    header::{ACCEPT, CONTENT_TYPE},
    http, Body, Request, Response,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashMap},
    env::temp_dir,
    fs::{read, File},
    io::{Cursor, Seek, SeekFrom, Write},
    path::Path,
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use tokio::{
    sync::{mpsc, Notify},
    time::sleep,
};
use uuid::Uuid;

type HttpClient = Client<HttpConnector, Body>;

/// Sse event is the tuple: event name and event content (fields `event` and `data` respectively)
pub type SseEvent = (Option<String>, String);

pub struct SseClient {
    response: Response<Body>,
    lines: Lines,
}

#[derive(Error, Debug)]
enum Errors {
    #[error("Error performing HTTP request to a Curator server")]
    HttpClient(http::Error),

    #[error("Unexpected status code: {}", .0)]
    UnexpectedStatusCode(http::StatusCode),
}

#[derive(Deserialize, Hash, PartialEq, Eq, Clone, Debug, Default)]
pub struct TaskDef {
    /// Task id
    ///
    /// Task id is the global identifier used to address task in the system (across all running agents)
    pub id: String,

    /// Command to be executed in toolchain container
    ///
    /// First part is the absolute path to executable and all following are the arguments:
    ///
    /// ```
    /// vec!["/sbin/lsof", "-p", "1"]
    /// ```
    pub command: Vec<String>,

    /// Target container id
    pub container_id: String,

    /// Toolchain container image name
    pub toolchain: String,

    /// Human readable task description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(skip_serializing_if = "BTreeSet::is_empty", default)]
    pub tags: BTreeSet<String>,
}

/// Task set is the set of all the tasks found in the system
/// on a given round of discovery
pub type TaskSet = Vec<TaskDef>;

impl SseClient {
    pub async fn connect<T: Serialize>(uri: &str, body: T) -> Result<Self> {
        let client = HttpClient::new();

        let json = serde_json::to_string(&body)?;
        let req = Request::builder()
            .method("POST")
            .uri(uri)
            .header(ACCEPT, "text/event-stream")
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(json))?;

        let response = client.request(req).await?;
        if !response.status().is_success() {
            bail!(Errors::UnexpectedStatusCode(response.status()));
        }

        Ok(Self {
            response,
            lines: Lines::new(),
        })
    }

    pub async fn next_event(&mut self) -> Result<Option<SseEvent>> {
        let mut event_name = None;
        loop {
            let bytes = self.response.body_mut().data().await;

            if let Some(bytes) = bytes {
                let bytes = bytes?;
                for line in self.lines.feed(bytes)? {
                    if line.starts_with("event:") {
                        let name = line.chars().skip(6).collect::<String>();
                        event_name = Some(name.trim().to_string());
                    }
                    if line.starts_with("data:") {
                        let data = line.chars().skip(5).collect::<String>();
                        return Ok(Some((event_name, data.trim().to_string())));
                    }
                }
            } else {
                return Ok(None);
            }
        }
    }
}

/// Lines buffered iterator
///
/// Accepts `&[u8]` slices, buffers them and returns lines if newline character is in
/// input bytes.
struct Lines(Cursor<Vec<u8>>);

impl Lines {
    fn new() -> Self {
        Self(Cursor::new(vec![0]))
    }

    fn feed(&mut self, bytes: impl AsRef<[u8]>) -> Result<Vec<String>> {
        Write::write(&mut self.0, bytes.as_ref())
            .context("Unable to write data to in-memory cursor. Should never happen")?;

        let mut vec = Vec::new();
        while let Some(line) = self.take_next_line()? {
            vec.push(line);
        }

        Ok(vec)
    }

    fn take_next_line(&mut self) -> Result<Option<String>> {
        let buffer = self.0.get_ref();
        let newline_position = buffer.iter().position(|x| *x == b'\n');

        if let Some(position) = newline_position {
            let (line_bytes, tail) = buffer.split_at(position);
            let line = String::from_utf8(line_bytes.to_vec()).context("Invalid UTF sequence")?;

            // skipping newline character and replace cursor with leftover data
            self.0 = Cursor::new(tail[1..].to_vec());
            Seek::seek(&mut self.0, SeekFrom::End(0))?;

            Ok(Some(line))
        } else {
            Ok(None)
        }
    }
}

pub struct CloseHandle(Arc<Notify>);

impl Drop for CloseHandle {
    fn drop(&mut self) {
        self.0.notify_one();
    }
}

impl From<Arc<Notify>> for CloseHandle {
    fn from(f: Arc<Notify>) -> Self {
        Self(f)
    }
}

pub struct AgentLoop {
    name: String,
    uri: String,
    tasks: HashMap<String, TaskDef>,
    close_handle: Arc<Notify>,
    docker: Docker,
}

impl AgentLoop {
    pub fn run(host: &str, name: &str, tasks: Vec<TaskDef>) -> Result<CloseHandle> {
        let name = name.into();

        let tasks = tasks.into_iter().map(|i| (i.id.clone(), i)).collect();
        let close_handle = Arc::new(Notify::new());
        let mut agent_loop = Self {
            name,
            uri: format!("http://{}", host),
            tasks,
            close_handle: close_handle.clone(),
            docker: Docker::connect_with_local_defaults()?,
        };

        tokio::spawn(async move { agent_loop._run().await });

        Ok(close_handle.into())
    }

    #[logfn(ok = "Trace", err = "Error")]
    async fn _run(&mut self) -> Result<()> {
        let tasks = self
            .tasks
            .values()
            .map(|t| Task {
                id: t.id.clone(),
                description: t.description.clone(),
                tags: t.tags.clone(),
            })
            .collect::<Vec<_>>();
        let agent = agent::Agent {
            name: self.name.clone(),
            tasks,
        };

        let uri = format!("{}/backend/events", self.uri);

        loop {
            let mut client: Option<SseClient> = None;
            while client.is_none() {
                let connection = SseClient::connect(&uri, &agent)
                    .await
                    .with_context(|| format!("Unable to connect to Curator server: {}", &self.uri));
                client = match connection {
                    Ok(client) => Some(client),
                    Err(e) => {
                        log_errors(&e);
                        sleep(Duration::from_secs(5)).await;
                        None
                    }
                }
            }
            let mut client = client.unwrap();
            let mut on_close = self.close_handle.notified().boxed().fuse();

            loop {
                let mut on_message = client.next_event().boxed().fuse();
                let mut heartbeat_timeout = sleep(Duration::from_secs(2)).boxed().fuse();

                select! {
                    // Handling new incoming message
                    msg = on_message => {
                        match msg {
                            Ok(Some(msg)) => {
                                self.process_message(&msg);
                            },
                            Ok(None) => {
                                trace!("Stream ended. Trying to reconnect");
                                break;
                            }
                            Err(e) => {
                                trace!("Error from the upstream: {}", e);
                                break;
                            }
                        }
                    },

                    // ping server back to report agent is alive
                    _ = heartbeat_timeout => {
                        if let Err(e) = self.heartbeat_server(&agent).await {
                            warn!("Unable to ping server: {}", e);
                            break;
                        }
                    },

                    _ = on_close => {
                        trace!("Close signal received. Exiting");
                        return Ok(());
                    }
                }
            }
            sleep(Duration::from_secs(1)).await;
        }
    }

    async fn heartbeat_server(&self, agent: &agent::Agent) -> Result<()> {
        trace!("Sending heartbeat");
        let client = HttpClient::new();
        let json = serde_json::to_string(&agent)?;
        let report = Request::post(format!("{}/backend/hb", self.uri))
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(json));
        let response = client.request(report?).await?;
        if !response.status().is_success() {
            bail!(Errors::UnexpectedStatusCode(response.status()));
        }
        Ok(())
    }

    fn process_message(&self, event: &SseEvent) {
        trace!("Incoming message...");
        let (name, event) = event;
        match name {
            Some(s) if s == "run-task" => match serde_json::from_str::<agent::RunTask>(&event) {
                Ok(event) => {
                    self.spawn_and_track_task(event.task_id, event.execution);
                }
                Err(e) => {
                    warn!("Unable to interpret {} event: {}. {}", s, event, e);
                }
            },
            Some(s) if s == "stop-task" => {
                unimplemented!();
            }
            _ => {
                error!("Invalid event: {}", event);
            }
        }
    }

    fn spawn_and_track_task(&self, task_id: String, execution_id: Uuid) {
        use TaskProgress::*;

        trace!("Task requested: {}, execution: {}", task_id, execution_id);

        let (tx, rx) = mpsc::channel(100);
        tokio::spawn(Self::report_execution_back(
            self.uri.to_owned(),
            execution_id,
            rx,
        ));

        if let Some(task) = self.tasks.get(&task_id) {
            let task = task.clone();
            let docker = self.docker.clone();

            tokio::spawn(Self::do_run_toolchain_task(docker, task, tx, execution_id));
        } else {
            warn!("Task {} not found", &task_id);
            let reason = format!("Task {} not found", &task_id);
            tx.try_send(FailedToStart(reason)).unwrap();
        }
    }

    #[logfn(ok = "Trace", err = "Error")]
    async fn do_run_toolchain_task(
        docker: Docker,
        task: TaskDef,
        tx: mpsc::Sender<TaskProgress>,
        execution_id: Uuid,
    ) -> Result<()> {
        use TaskProgress::*;

        let (stdout_tx, stdout_rx) = mpsc::channel::<Bytes>(1);
        let (stderr_tx, stderr_rx) = mpsc::channel::<Bytes>(1);

        let stdout_handle = tokio::spawn(Self::copy_bytes(stdout_rx, tx.clone()));
        let stderr_handle = tokio::spawn(Self::copy_bytes(stderr_rx, tx.clone()));
        let artifact_path = temp_dir().join(format!("{}.tar", execution_id));
        let status = run_toolchain_task(
            &docker,
            &task,
            Some(stdout_tx),
            Some(stderr_tx),
            Some(File::create(&artifact_path)?),
        )
        .await?;
        stdout_handle.await??;
        stderr_handle.await??;
        tx.send(Finished(status)).await?;
        tx.send(ArtifactsReady(Artifact(artifact_path))).await?;

        Ok(())
    }

    async fn copy_bytes(
        mut rx: mpsc::Receiver<Bytes>,
        tx: mpsc::Sender<TaskProgress>,
    ) -> Result<()> {
        use TaskProgress::*;

        while let Some(msg) = rx.recv().await {
            let string = String::from_utf8(msg.to_vec())?;
            tx.send(Stdout(string)).await?;
        }
        Ok(())
    }

    #[logfn(ok = "Trace", err = "Error")]
    async fn report_execution_back(
        uri: String,
        id: Uuid,
        mut rx: mpsc::Receiver<TaskProgress>,
    ) -> Result<()> {
        use ExecutionStatus::*;
        use TaskProgress::*;
        let client = HttpClient::new();

        while let Some(progress) = rx.recv().await {
            let report = match &progress {
                Stdout(line) => report_request(
                    &uri,
                    agent::ExecutionReport {
                        id,
                        status: RUNNING,
                        stdout_append: Some(line.to_string()),
                    },
                ),
                Finished(exit_code) => report_request(
                    &uri,
                    agent::ExecutionReport {
                        id,
                        status: ExecutionStatus::from_unix_exit_code(*exit_code),
                        stdout_append: None,
                    },
                ),
                FailedToStart(reason) => report_request(
                    &uri,
                    agent::ExecutionReport {
                        id,
                        status: REJECTED,
                        stdout_append: Some(reason.to_string()),
                    },
                ),
                ArtifactsReady(path) => attach_artifacts_request(&uri, id, path),
            };

            let response = client.request(report?).await?;
            if !response.status().is_success() {
                bail!(format_err!(
                    "Error while reporting back to Curator HTTP/{}",
                    response.status()
                ));
            }

            // progress-message could have additional resources allocated (artifact files on a FS)
            // which should be dropped only after reporting to server is finished
            drop(progress);
        }

        Ok(())
    }
}

fn report_request(uri: &str, report: agent::ExecutionReport) -> Result<Request<Body>> {
    let json = serde_json::to_string(&report).expect("Unable to serialize JSON");
    Request::post(format!("{}/backend/execution/report", uri))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json))
        .map_err(|e| Errors::HttpClient(e).into())
}

fn attach_artifacts_request(uri: &str, id: Uuid, path: impl AsRef<Path>) -> Result<Request<Body>> {
    Request::post(format!("{}/backend/execution/attach?id={}", uri, id))
        .header(CONTENT_TYPE, "application/x-tgz")
        .body(Body::from(read(path)?))
        .map_err(|e| Errors::HttpClient(e).into())
}

#[derive(Debug)]
struct Artifact(PathBuf);

impl Drop for Artifact {
    fn drop(&mut self) {
        use std::fs;

        if let Err(e) = fs::remove_file(&self.0) {
            warn!(
                "Unable to remove artifact file: {}. Reason: {}",
                self.0.display(),
                e
            );
        }
    }
}

impl AsRef<Path> for Artifact {
    fn as_ref(&self) -> &Path {
        &self.0.as_ref()
    }
}

#[derive(Debug)]
enum TaskProgress {
    Stdout(String),
    Finished(i32),
    FailedToStart(String),

    /// Generated when artifact is ready. Artifact implement `Drop`, so
    /// underlying file will be removed when variable is leaving scope.
    ArtifactsReady(Artifact),
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::tests::*;
    use maplit::btreeset;
    use serde_json::json;

    fn init() {
        let _ = env_logger::builder().is_test(true).try_init();
    }

    #[test]
    fn test_read_by_lines() -> Result<()> {
        init();

        let fixture = [
            ("event:", vec![]),
            ("data", vec![]),
            ("\nevent:", vec!["event:data"]),
            ("\n", vec!["event:"]),
            ("\n1\n2\n", vec!["", "1", "2"]),
        ];

        let mut lines = Lines::new();
        for (input, output) in fixture.iter() {
            assert_eq!(lines.feed(input.as_bytes())?, *output);
        }

        Ok(())
    }

    #[test]
    fn check_taskdef_read_write() -> Result<()> {
        assert_json_reads(
            TaskDef {
                id: "foo".into(),
                container_id: "e2adfa57360d".into(),
                toolchain: "toolchain:dev".into(),
                command: vec!["who".into(), "-a".into()],
                description: Some("Calling who command".into()),
                tags: btreeset! {"who".into(), "unix".into()},
                ..Default::default()
            },
            json!({
                "id": "foo",
                "container_id": "e2adfa57360d",
                "toolchain": "toolchain:dev",
                "command": ["who", "-a"],
                "tags": ["who", "unix"],
                "description": "Calling who command"
            }),
        )
    }
}
