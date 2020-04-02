use crate::errors::*;
use crate::protocol;
use hyper::{body::HttpBody as _, header, Body, Client, Request, Response};
use std::collections::HashMap;
use std::io::{Cursor, Seek, SeekFrom, Write};
use std::process::Stdio;
use tokio::io::BufReader;
use tokio::prelude::*;
use tokio::process::{Child, Command};
use tokio::stream::StreamExt;
use uuid::Uuid;
use error_chain::bail;

/// Sse event is the tuple: event name and event content (fields `event` and `data` respectively)
pub type SseEvent = (Option<String>, String);

// Definition of a task that agent can run
pub struct Task {
    /// Task id.
    ///
    /// Uniq for an agent, but not for the system (multiple agents can have same task)
    pub id: String,

    /// Command factory
    ///
    /// This function creates `Command` for execution
    pub command: fn() -> Command,
}

pub struct Execution {
    pub id: uuid::Uuid,
    pub task: String,
    pub status: protocol::ExecutionStatus,
    pub stdout: Vec<String>,
}

impl Execution {
    fn new(id: Uuid, task_id: &str) -> Self {
        Self {
            id,
            task: task_id.to_string(),
            status: protocol::ExecutionStatus::INITIATED,
            stdout: vec![],
        }
    }
}

pub struct SseClient {
    response: Response<Body>,
    lines: Lines,
}

impl SseClient {
    pub async fn connect(host: &str, agent: protocol::Agent) -> Result<Self> {
        let client = Client::new();

        let json = serde_json::to_string(&agent)?;
        let req = Request::builder()
            .method("POST")
            .uri(host)
            .header(header::ACCEPT, "text/event-stream")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json))?;

        let response = client.request(req).await?;
        if !response.status().is_success() {
            bail!(ErrorKind::ClientConnectError(host.into(), response.status().as_u16()));
        }
        println!("{:?}", response.status());

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

#[derive(Default)]
pub struct Executions {
    tasks: HashMap<String, fn() -> Command>,
    executions: HashMap<Uuid, Execution>,
}

impl Executions {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn register_task(&mut self, task_id: &str, factory: fn() -> Command) {
        self.tasks.insert(task_id.to_string(), factory);
    }

    pub fn run(&mut self, task_id: &str, execution_id: Uuid) -> bool {
        if let Some(factory) = self.tasks.get(task_id) {
            let mut task = factory();
            match task.stdout(Stdio::piped()).spawn() {
                Ok(child) => {
                    let execution = Execution::new(execution_id, task_id);
                    self.executions.insert(execution_id, execution);
                    tokio::spawn(async move {
                        Self::process_child(child).await;
                    });
                    return true;
                }
                Err(e) => {
                    eprintln!("Unable to spawn child: {}", e);
                }
            }
        }
        false
    }

    async fn process_child(child: Child) {
        let stdout = child.stdout.unwrap();
        let mut reader = BufReader::new(stdout).lines();

        while let Some(line) = reader.next().await {
            println!("Line: {}", line.expect("No line from process"));
        }
    }
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