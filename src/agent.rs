use crate::prelude::*;
use hyper::{
    body::HttpBody as _,
    header::{ACCEPT, CONTENT_TYPE},
    Body, Client, Request, Response,
};
use serde::Serialize;
use std::collections::HashMap;
use std::io::{Cursor, Seek, SeekFrom, Write};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::io::BufReader;
use tokio::prelude::*;
use tokio::process::{Child, Command};
use tokio::stream::StreamExt;
use tokio::sync::mpsc::{self, Receiver, Sender};
use uuid::Uuid;

/// Sse event is the tuple: event name and event content (fields `event` and `data` respectively)
pub type SseEvent = (Option<String>, String);

pub struct SseClient {
    response: Response<Body>,
    lines: Lines,
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
            bail!(ErrorKind::ClientConnectError(
                host.into(),
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
            let bytes = self
                .response
                .body_mut()
                .data()
                .await
                .map(|r| r.chain_err(|| "Unable to read from server"));

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
            .chain_err(|| "Unable to write data to in-memory cursor. Should never happen")?;

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
            let line =
                String::from_utf8(line_bytes.to_vec()).chain_err(|| "Invalid UTF sequence")?;

            // skipping newline character and replace cursor with leftover data
            self.0 = Cursor::new(tail[1..].to_vec());
            Seek::seek(&mut self.0, SeekFrom::End(0))?;

            Ok(Some(line))
        } else {
            Ok(None)
        }
    }
}

pub struct CuratorAgent {
    agent: AgentRef,
    tasks: Shared<HashMap<String, fn() -> Command>>,
    executions: Shared<HashMap<Uuid, Arc<Mutex<Execution>>>>,
}

impl CuratorAgent {
    pub fn new(application: &str, instance: &str) -> Self {
        let application = application.into();
        let instance = instance.into();
        Self {
            agent: AgentRef {
                application,
                instance,
            },
            tasks: shared(HashMap::new()),
            executions: shared(HashMap::new()),
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        let tasks = self
            .tasks
            .lock()
            .unwrap()
            .keys()
            .map(|t| Task { id: t.into() })
            .collect::<Vec<_>>();
        let agent = agent::Agent {
            application: self.agent.application.clone(),
            instance: self.agent.instance.clone(),
            tasks,
        };
        let mut client = SseClient::connect("http://localhost:8080/events", &agent).await?;

        while let Some((name, event)) = client.next_event().await? {
            match name {
                Some(s) if s == "run-task" => {
                    match serde_json::from_str::<agent::RunTask>(&event) {
                        Ok(event) => {
                            if !self.spawn_task(&event.task_id, event.execution) {
                                eprintln!("Task {} not found", event.task_id);
                            }
                        }
                        Err(e) => {
                            eprintln!("Unable to interpret {} event: {}. {}", s, event, e);
                        }
                    }
                }
                Some(s) if s == "stop-task" => {
                    unimplemented!();
                }
                _ => {}
            }
        }

        Ok(())
    }

    pub fn register_task(&mut self, task_id: &str, factory: fn() -> Command) {
        self.tasks
            .lock()
            .unwrap()
            .insert(task_id.to_string(), factory);
    }

    fn spawn_task(&mut self, task_id: &str, execution_id: Uuid) -> bool {
        if let Some(factory) = self.tasks.lock().unwrap().get(task_id) {
            let mut task = factory();
            match task.stdout(Stdio::piped()).spawn() {
                Ok(child) => {
                    let execution = Execution::with_arc(execution_id);
                    self.executions
                        .lock()
                        .unwrap()
                        .insert(execution_id, Arc::clone(&execution));

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

    async fn report_execution_back(id: Uuid, mut rx: Receiver<ChildProgress>) {
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

    async fn read_stdout(mut child: Child, mut tx: Sender<ChildProgress>) {
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
