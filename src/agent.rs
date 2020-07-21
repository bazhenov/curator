use crate::prelude::*;
use futures::{future::FutureExt, select};
use hyper::{
    body::HttpBody as _,
    header::{ACCEPT, CONTENT_TYPE},
    http, Body, Client, Request, Response,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashMap},
    ffi::OsStr,
    fs::{read, File},
    io::{Cursor, Seek, SeekFrom, Write},
    path::Path,
    path::PathBuf,
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use tempdir::TempDir;
use tokio::{
    io::BufReader,
    prelude::*,
    process::{Child, ChildStderr, ChildStdout, Command},
    stream::StreamExt,
    sync::{mpsc, Notify},
    time::delay_for,
    try_join,
};
use uuid::Uuid;

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
}

#[derive(Deserialize, Hash, PartialEq, Eq, Clone, Debug, Default)]
pub struct TaskDef {
    pub id: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "BTreeSet::is_empty", default)]
    pub tags: BTreeSet<String>,
}

impl TaskDef {
    /**
     * Returns child process and it's working directory.
     * All content of working directory is dropped when `TempDir` is destroyed
     */
    fn spawn(&self) -> Result<(Child, ChildStdout, ChildStderr, TempDir)> {
        let work_dir = TempDir::new("curator_agent")?;
        let mut cmd = Command::new(&self.command);
        trace!(
            "Spawning command: {} with args {:?}",
            &self.command,
            &self.args
        );
        let mut child = cmd
            .args(&self.args)
            .current_dir(&work_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Unable to spawn process")?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| format_err!("Unable to get stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| format_err!("Unable to get stderr"))?;
        Ok((child, stdout, stderr, work_dir))
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

pub async fn discover(path: impl AsRef<Path>) -> Result<Vec<TaskDef>> {
    use std::os::unix::fs::PermissionsExt;
    let mut tasks = vec![];

    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;

        if let Some(name) = entry.path().file_name() {
            let is_executable = metadata.permissions().mode() & 0o111 > 0;
            let name_match = name.to_str().and_then(|name| name.find(".discovery."));
            trace!("Checking: {:?} exec: {}", entry, is_executable);
            if metadata.is_file() && is_executable && name_match.is_some() {
                tasks.append(&mut gather_tasks(entry.path())?);
            }
        }
    }
    Ok(tasks)
}

fn gather_tasks<T: DeserializeOwned>(script: PathBuf) -> Result<Vec<T>> {
    use std::process::Command;

    let output = Command::new(script)
        .stdout(Stdio::piped())
        .spawn()?
        .wait_with_output()?;
    let stdout = String::from_utf8(output.stdout)?;
    Ok(stdout
        .lines()
        .flat_map(|l| serde_json::from_str::<T>(&l))
        .collect())
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
    name: String,
    uri: String,
    tasks: HashMap<String, TaskDef>,
    close_handle: Arc<Notify>,
}

impl AgentLoop {
    pub fn run(host: &str, name: &str, tasks: Vec<TaskDef>) -> CloseHandle {
        let name = name.into();

        let tasks = tasks.into_iter().map(|i| (i.id.clone(), i)).collect();
        let close_handle = Arc::new(Notify::new());
        let mut agent_loop = Self {
            name,
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
        use ChildProgress::*;

        let (mut tx, rx) = mpsc::channel(100);
        tokio::spawn(Self::report_execution_back(
            self.uri.to_owned(),
            execution_id,
            rx,
        ));

        if let Some(task) = self.tasks.get(&task_id) {
            let task = task.clone();
            tokio::spawn(async move {
                if let Err(e) = Self::run_task(task, tx.clone()).await {
                    let reason = format!(
                        "Task {} failed to start: {}",
                        &task_id,
                        format_error_chain(&e)
                    );
                    tx.send(FailedToStart(reason)).await.unwrap();
                }
            });
        } else {
            warn!("Task {} not found", &task_id);
            let reason = format!("Task {} not found", &task_id);
            tokio::spawn(async move { tx.send(FailedToStart(reason)).await.unwrap() });
        }
    }

    async fn report_execution_back(uri: String, id: Uuid, rx: mpsc::Receiver<ChildProgress>) {
        if let Err(e) = Self::do_report_execution_back(uri, id, rx).await {
            error!("Reporting task failed: {}", e);
        }
    }

    async fn do_report_execution_back(
        uri: String,
        id: Uuid,
        mut rx: mpsc::Receiver<ChildProgress>,
    ) -> Result<()> {
        use ChildProgress::*;
        use ExecutionStatus::*;
        let client = Client::new();

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
                Interrupted => report_request(
                    &uri,
                    agent::ExecutionReport {
                        id,
                        status: FAILED,
                        stdout_append: Some("Interrupted by signal".to_owned()),
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

    async fn run_task(task: TaskDef, mut tx: mpsc::Sender<ChildProgress>) -> Result<()> {
        use ChildProgress::*;

        let (child, stdout, stderr, work_dir) = task.spawn()?;
        // work_dir should live until child completion, otherwise temporary directory will be removed
        let path = work_dir.path();

        let stdout_handle = write_stream_to_file(
            stdout,
            File::create(path.join("stdout.log"))?,
            tx.clone(),
            Stdout,
        );

        let stderr_handle = write_stream_to_file(
            stderr,
            File::create(path.join("stderr.log"))?,
            tx.clone(),
            Stdout,
        );

        try_join!(stdout_handle, stderr_handle)
            .context("Unable to process process stdin/stderr")?;
        let status = child.await.context("Unable to read process status")?;

        let progress_report = if let Some(code) = status.code() {
            trace!("Process exited with code {}", code);
            Finished(code)
        } else {
            warn!("Process interripted");
            Interrupted
        };
        tx.send(progress_report).await?;

        let path = "./result.tar.gz";
        trace!(
            "Gathering artifacts for directory: {}",
            work_dir.path().display()
        );
        Command::new("tar")
            .args(&[
                OsStr::new("-C"),
                work_dir.path().as_os_str(),
                OsStr::new("-czf"),
                OsStr::new(path),
                OsStr::new("."),
            ])
            .spawn()?
            .wait_with_output()
            .await?;
        drop(work_dir);
        tx.send(ArtifactsReady(path.into())).await?;
        Ok(())
    }
}

async fn write_stream_to_file<R: AsyncRead + Unpin, N: std::fmt::Debug>(
    r: R,
    mut file: File,
    mut channel: mpsc::Sender<N>,
    f: impl Fn(String) -> N,
) -> Result<()> {
    let mut reader = BufReader::new(r).lines();
    while let Some(line) = reader.next().await {
        let mut line = line?;
        line.push('\n');
        file.write_all(line.as_bytes()).unwrap();
        channel.send(f(line)).await.unwrap();
    }
    Ok(())
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

impl From<&str> for Artifact {
    fn from(path: &str) -> Self {
        Self(PathBuf::from(path))
    }
}

#[derive(Debug)]
enum ChildProgress {
    Stdout(String),
    Finished(i32),
    Interrupted,
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
    use std::fs;
    use tempfile::TempDir;

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
                command: "who".into(),
                args: vec!["-a".into()],
                description: Some("Calling who command".into()),
                tags: btreeset! {"who".into(), "unix".into()},
                ..Default::default()
            },
            json!({
                "id": "foo",
                "command": "who",
                "args": ["-a"],
                "tags": ["who", "unix"],
                "description": "Calling who command"
            }),
        )
    }

    #[tokio::test]
    async fn check_discovery_process() -> Result<()> {
        use fs::File;
        use std::os::unix::fs::PermissionsExt;
        init();

        let tmp = TempDir::new()?;
        let path = tmp.path().join("test.discovery.sh");
        let mut permissions = {
            let mut file = File::create(&path)?;
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

    #[test]
    fn artifact_should_remove_its_file() -> Result<()> {
        let tmp = TempDir::new()?;
        let path = tmp.path().join("test.bin");
        File::create(&path)?;

        {
            let _artifact = Artifact(path.clone());
        }
        assert!(
            !path.exists(),
            "File should be removed after artifact leaving scope"
        );

        Ok(())
    }

    fn create_task(command: &str, args: &[&str]) -> TaskDef {
        let command = command.to_string();

        let args = args.iter().map(|s| String::from(*s)).collect::<Vec<_>>();
        TaskDef {
            id: "id".to_string(),
            command,
            args,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn check_run_command_has_isolated_directory() -> Result<()> {
        init();

        let task = create_task("pwd", &[]);
        let (stdout1, _) = execute(task).await?;

        let task = create_task("pwd", &[]);
        let (stdout2, _) = execute(task).await?;

        assert_ne!(
            stdout1, stdout2,
            "Each execution should have each own private working directory"
        );

        Ok(())
    }

    #[tokio::test]
    async fn check_stdout_works_correctly() -> Result<()> {
        init();

        let (stdout, _) = execute(create_task("sh", &["-c", "echo 1; echo 2"])).await?;
        assert_eq!("1\n2\n", stdout);

        Ok(())
    }

    async fn execute(task: TaskDef) -> Result<(String, i32)> {
        use ChildProgress::*;

        let (tx, mut rx) = mpsc::channel(1);

        let task = tokio::spawn(AgentLoop::run_task(task, tx));

        let mut stdout = String::new();
        while let Some(msg) = rx.recv().await {
            match msg {
                Stdout(ref s) => stdout.push_str(s),
                Finished(exit_code) => {
                    task.await??;
                    return Ok((stdout, exit_code));
                }
                FailedToStart(reason) => panic!("Unable to start process: {}", reason),
                Interrupted => panic!("Interrupted"),
                ArtifactsReady(_) => {}
            }
        }
        panic!("No finished message was found in stream");
    }
}
