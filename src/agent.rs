use crate::prelude::*;
use futures::{future::FutureExt, select};
use hyper::{
    body::HttpBody as _,
    header::{ACCEPT, CONTENT_TYPE},
    Body, Client, Request, Response,
};
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    io::{Cursor, Seek, SeekFrom, Write},
    path::Path,
    path::PathBuf,
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::BufReader,
    prelude::*,
    process::{Child, Command},
    stream::StreamExt,
    sync::{mpsc, Notify},
    time::delay_for,
};
use uuid::Uuid;

/// Sse event is the tuple: event name and event content (fields `event` and `data` respectively)
pub type SseEvent = (Option<String>, String);

pub struct SseClient {
    response: Response<Body>,
    lines: Lines,
}

#[derive(serde::Deserialize, Hash, PartialEq, Eq, Clone, Debug)]
pub struct TaskDef {
    pub id: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl TaskDef {
    fn spawn(&self) -> Result<Child> {
        let mut cmd = Command::new(&self.command);
        for arg in &self.args {
            cmd.arg(arg);
        }
        cmd.stdout(Stdio::piped())
            .spawn()
            .context("Unable to spawn process")
            .map_err(Into::into)
    }
}

impl SseClient {
    pub async fn connect<T: Serialize>(uri: &str, body: T) -> Result<Self> {
        let client = Client::new();

        let json = serde_json::to_string(&body)?;
        let req = Request::builder()
            .method("POST")
            .uri(uri)
            .header(ACCEPT, "text/event-stream")
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(json))?;

        let response = client.request(req).await?;
        if !response.status().is_success() {
            bail!(format_err!(
                "Client connect error: {} HTTP/{}",
                uri,
                response.status().as_u16()
            ));
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

pub async fn discover(path: impl AsRef<Path>) -> Result<HashSet<TaskDef>> {
    use std::os::unix::fs::PermissionsExt;
    let mut tasks = HashSet::new();

    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;

        if let Some(name) = entry.path().file_name() {
            let is_executable = metadata.permissions().mode() & 0o111 > 0;
            let name_match = name.to_str().and_then(|name| name.find(".discovery."));
            trace!("Checking: {:?} exec: {}", entry, is_executable);
            if metadata.is_file() && is_executable && name_match.is_some() {
                tasks = tasks.union(&gather_tasks(entry.path())?).cloned().collect();
            }
        }
    }
    Ok(tasks)
}

fn gather_tasks(script: PathBuf) -> Result<HashSet<TaskDef>> {
    use std::process::Command;

    let output = Command::new(script)
        .stdout(Stdio::piped())
        .spawn()?
        .wait_with_output()?;
    let stdout = String::from_utf8(output.stdout)?;
    Ok(stdout
        .lines()
        .flat_map(|l| serde_json::from_str::<TaskDef>(&l))
        .collect::<HashSet<_>>())
}

pub struct CloseHandle(Arc<Notify>);

impl Drop for CloseHandle {
    fn drop(&mut self) {
        self.0.notify();
    }
}

impl From<Arc<Notify>> for CloseHandle {
    fn from(f: Arc<Notify>) -> Self {
        Self(f)
    }
}

pub struct AgentLoop {
    agent: AgentRef,
    uri: String,
    tasks: HashMap<String, TaskDef>,
    close_handle: Arc<Notify>,
}

impl AgentLoop {
    pub fn run(host: &str, application: &str, instance: &str, tasks: Vec<TaskDef>) -> CloseHandle {
        let application = application.into();
        let instance = instance.into();

        let tasks = tasks.into_iter().map(|i| (i.id.clone(), i)).collect();
        let close_handle = Arc::new(Notify::new());
        let mut agent_loop = Self {
            agent: AgentRef {
                application,
                instance,
            },
            uri: format!("http://{}", host),
            tasks,
            close_handle: close_handle.clone(),
        };

        tokio::spawn(async move {
            if let Err(e) = agent_loop._run().await {
                log_errors(&e);
            }
        });

        close_handle.into()
    }

    async fn _run(&mut self) -> Result<()> {
        let tasks = self
            .tasks
            .values()
            .map(|t| Task {
                id: t.id.clone(),
                description: t.description.clone(),
            })
            .collect::<Vec<_>>();
        let agent = agent::Agent {
            application: self.agent.application.clone(),
            instance: self.agent.instance.clone(),
            tasks,
        };

        let uri = format!("{}/backend/events", self.uri);

        loop {
            let mut client: Option<SseClient> = None;
            while client.is_none() {
                let connection = SseClient::connect(&uri, &agent).await.with_context(|_| {
                    format!("Unable to connect to Curator server: {}", &self.uri)
                });
                client = match connection {
                    Ok(client) => Some(client),
                    Err(e) => {
                        log_errors(&e.into());
                        delay_for(Duration::from_secs(5)).await;
                        None
                    }
                }
            }
            let mut client = client.unwrap();
            let mut on_close = self.close_handle.notified().boxed().fuse();

            loop {
                let mut on_message = client.next_event().boxed().fuse();

                select! {
                    msg = on_message => {
                        match msg {
                            Ok(msg) => {
                                if let Some(msg) = msg {
                                    trace!("Incoming message...");
                                    self.process_message(&msg);
                                } else {
                                    trace!("Stream ended. Trying to reconnect");
                                    delay_for(Duration::from_secs(1)).await;
                                    break;
                                }
                            },
                            Err(e) => {
                                trace!("Error from the upstream: {}", e);
                                delay_for(Duration::from_secs(1)).await;
                                break;
                            }
                        }
                    },
                    _ = on_close => {
                        trace!("Close signal received. Exiting");
                        return Ok(());
                    }
                }
            }
        }
    }

    fn process_message(&self, event: &SseEvent) {
        let (name, event) = event;
        match name {
            Some(s) if s == "run-task" => match serde_json::from_str::<agent::RunTask>(&event) {
                Ok(event) => {
                    self.spawn_and_track_task(&event.task_id, event.execution);
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

    pub fn register_task(&mut self, task: TaskDef) {
        self.tasks.insert(task.id.clone(), task);
    }

    fn spawn_and_track_task(&self, task_id: &str, execution_id: Uuid) {
        let (mut tx, rx) = mpsc::channel(100);
        tokio::spawn(Self::report_execution_back(
            self.uri.clone(),
            execution_id,
            rx,
        ));

        if let Some(task) = self.tasks.get(task_id) {
            match task.spawn() {
                Ok(child) => {
                    tokio::spawn(Self::read_stdout(child, tx));
                }
                Err(e) => {
                    log_errors(&e);
                    tokio::spawn(async move {
                        let report = ChildProgress::FailedToStart(format_error_chain(&e));
                        tx.send(report).await
                    });
                }
            }
        } else {
            warn!("Task {} not found", &task_id);
            let task_id = task_id.to_string();
            tokio::spawn(async move {
                let report = ChildProgress::FailedToStart(format!("Task {} not found", task_id));
                tx.send(report).await
            });
        }
    }

    async fn report_execution_back(uri: String, id: Uuid, mut rx: mpsc::Receiver<ChildProgress>) {
        use ExecutionStatus::*;
        let client = Client::new();

        while let Some(progress) = rx.recv().await {
            let report = match progress {
                ChildProgress::Stdout(line) => agent::ExecutionReport {
                    id,
                    status: RUNNING,
                    stdout_append: Some(line),
                },
                ChildProgress::Finished(status_code) => {
                    let status = if status_code == 0 { COMPLETED } else { FAILED };
                    agent::ExecutionReport {
                        id,
                        status,
                        stdout_append: None,
                    }
                }
                ChildProgress::FailedToStart(reason) => agent::ExecutionReport {
                    id,
                    status: REJECTED,
                    stdout_append: Some(reason),
                },
            };

            let uri = format!("{}/backend/execution/report", uri);

            let json = serde_json::to_string(&report).expect("Unable to serialize JSON");
            let request = Request::builder()
                .uri(&uri)
                .method("POST")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(json))
                .expect("Unable to build request");

            match client.request(request).await {
                Err(e) => {
                    error!("Error while reporting back to Curator: {:?}", e);
                }
                Ok(r) if !r.status().is_success() => {
                    error!("Error while reporting back to Curator HTTP/{}", r.status());
                }
                Ok(_) => {}
            }
        }
    }

    async fn read_stdout(mut child: Child, mut tx: mpsc::Sender<ChildProgress>) {
        if let Some(stdout) = child.stdout.take() {
            let mut reader = BufReader::new(stdout).lines();
            while let Some(line) = reader.next().await {
                match line {
                    Ok(line) => {
                        tx.send(ChildProgress::Stdout(line)).await.unwrap();
                    }
                    Err(_) => {
                        break;
                    }
                }
            }
        }

        let result = child.await.expect("Unable to read process status");
        tx.send(ChildProgress::Finished(result.code().unwrap()))
            .await
            .unwrap();
    }
}

#[derive(Debug)]
enum ChildProgress {
    Stdout(String),
    Finished(i32),
    FailedToStart(String),
}

#[cfg(test)]
mod tests {

    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn init() {
        let _ = env_logger::builder().is_test(true).try_init();
    }

    #[test]
    fn test_read_by_lines() -> Result<()> {
        init();

        let fixture = [
            ("event:", Vec::new()),
            ("data", Vec::new()),
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

    #[tokio::test]
    async fn check_discovery_process() -> Result<()> {
        use std::os::unix::fs::PermissionsExt;
        init();

        let tmp = TempDir::new()?;
        let path = tmp.path().join("test.discovery.sh");
        let mut permissions = {
            let mut file = fs::File::create(&path)?;
            writeln!(file, "#!/usr/bin/env sh")?;
            writeln!(file, r#"echo '{{"id": "date", "command": "date"}}'"#)?;
            file.metadata()?.permissions()
        };
        permissions.set_mode(permissions.mode() | 0o100);
        fs::set_permissions(&path, permissions)?;

        let tasks = discover(tmp.path()).await?;
        assert_eq!(tasks.len(), 1);
        let first_task = tasks.iter().next().unwrap();
        assert_eq!(first_task.id, "date");
        Ok(())
    }
}
