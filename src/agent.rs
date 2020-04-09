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
    path::PathBuf,
    process::Stdio,
    sync::{Arc, Mutex},
};
use tokio::{
    io::BufReader,
    prelude::*,
    process::{Child, Command},
    stream::StreamExt,
    sync::{mpsc, oneshot},
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
}

impl TaskDef {
    pub fn new(id: impl AsRef<str>, command: impl AsRef<str>) -> Self {
        Self::new_with_args(id, command, vec![])
    }

    pub fn new_with_args(id: impl AsRef<str>, command: impl AsRef<str>, args: Vec<String>) -> Self {
        Self {
            id: id.as_ref().to_string(),
            command: command.as_ref().to_string(),
            args,
        }
    }

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
    pub async fn connect<T: Serialize>(host: &str, body: T) -> Result<Self> {
        let client = Client::new();

        let json = serde_json::to_string(&body)?;
        let req = Request::builder()
            .method("POST")
            .uri(host)
            .header(ACCEPT, "text/event-stream")
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(json))?;

        let response = client.request(req).await?;
        if !response.status().is_success() {
            bail!(format_err!(
                "Client connect error: {} HTTP/{}",
                host,
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

pub async fn discover() -> Result<HashSet<TaskDef>> {
    use std::os::unix::fs::PermissionsExt;
    let mut tasks = HashSet::new();
    for entry in std::fs::read_dir("./")? {
        let entry = entry?;
        let metadata = entry.metadata()?;

        if let Some(name) = entry.path().file_name() {
            let is_executable = metadata.permissions().mode() & 0o111 > 0;
            let name_match = name.to_str().and_then(|name| name.find(".discovery."));
            if metadata.is_file() && is_executable && name_match.is_some() {
                tasks = tasks.union(&gather_tasks(entry.path())?).cloned().collect();
            }
        }
    }
    Ok(tasks)
}

fn gather_tasks(script: PathBuf) -> Result<HashSet<TaskDef>> {
    use std::io::{BufRead, BufReader};
    use std::process::Command;

    let child = Command::new(script).stdout(Stdio::piped()).spawn()?;

    if let Some(stdout) = child.stdout {
        let lines = BufReader::new(stdout).lines();
        Ok(lines
            .flatten()
            .flat_map(|l| serde_json::from_str::<TaskDef>(&l))
            .collect::<HashSet<_>>())
    } else {
        Ok(HashSet::new())
    }
}

pub struct AgentLoop {
    agent: AgentRef,
    tasks: HashMap<String, TaskDef>,
    executions: HashMap<Uuid, Arc<Mutex<Execution>>>,
    close_channel: (Option<oneshot::Sender<()>>, Option<oneshot::Receiver<()>>),
}

impl AgentLoop {
    pub fn new(application: &str, instance: &str) -> Self {
        let application = application.into();
        let instance = instance.into();
        let (tx, rx) = oneshot::channel();
        AgentLoop {
            agent: AgentRef {
                application,
                instance,
            },
            tasks: HashMap::new(),
            executions: HashMap::new(),
            close_channel: (Some(tx), Some(rx)),
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        let tasks = self
            .tasks
            .keys()
            .map(|t| Task { id: t.into() })
            .collect::<Vec<_>>();
        let agent = agent::Agent {
            application: self.agent.application.clone(),
            instance: self.agent.instance.clone(),
            tasks,
        };
        let mut client = SseClient::connect("http://localhost:8080/events", &agent)
            .await
            .context("Unable to connect to Curator server")?;

        let mut on_close = self.close_channel.1.take().unwrap().fuse();

        loop {
            let mut on_message = client.next_event().boxed().fuse();

            select! {
                msg = on_message => {
                    if let Some(msg) = msg? {
                        self.process_message(&msg);
                    } else {
                        break;
                    }
                },
                _ = on_close => {
                    println!("Stopping agent loop");
                    break;
                }
            }
        }

        Ok(())
    }

    pub fn close_channel(&mut self) -> Option<oneshot::Sender<()>> {
        self.close_channel.0.take()
    }

    fn process_message(&mut self, event: &SseEvent) {
        let (name, event) = event;
        match name {
            Some(s) if s == "run-task" => match serde_json::from_str::<agent::RunTask>(&event) {
                Ok(event) => {
                    if !self.spawn_task(&event.task_id, event.execution) {
                        eprintln!("Task {} not found", event.task_id);
                    }
                }
                Err(e) => {
                    eprintln!("Unable to interpret {} event: {}. {}", s, event, e);
                }
            },
            Some(s) if s == "stop-task" => {
                unimplemented!();
            }
            _ => {}
        }
    }

    pub fn register_task(&mut self, task: TaskDef) {
        self.tasks.insert(task.id.clone(), task);
    }

    fn spawn_task(&mut self, task_id: &str, execution_id: Uuid) -> bool {
        if let Some(task) = self.tasks.get(task_id) {
            match task.spawn() {
                Ok(child) => {
                    let execution = Execution::with_arc(execution_id);
                    self.executions.insert(execution_id, Arc::clone(&execution));

                    let (tx, rx) = mpsc::channel(100);
                    tokio::spawn(Self::read_stdout(child, tx));
                    tokio::spawn(Self::report_execution_back(execution_id, rx));
                    return true;
                }
                Err(e) => {
                    eprintln!("Unable to spawn child: {}", e);
                }
            }
        }
        false
    }

    async fn report_execution_back(id: Uuid, mut rx: mpsc::Receiver<ChildProgress>) {
        use ExecutionStatus::*;
        let client = Client::new();
        let uri = "http://localhost:8080/execution/report";

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
            };
            let json = serde_json::to_string(&report).expect("Unable to serialize JSON");
            let request = Request::builder()
                .uri(uri)
                .method("POST")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(json))
                .expect("Unable to build request");

            match client.request(request).await {
                Err(e) => {
                    eprintln!("Error while reporting back to Curator: {:?}", e);
                }
                Ok(r) if !r.status().is_success() => {
                    eprintln!("Error while reporting back to Curator HTTP/{}", r.status());
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
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_read_by_lines() -> Result<()> {
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
}
