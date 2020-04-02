use crate::errors::*;
use hyper::{body::HttpBody as _, header, Body, Client, Request, Response};
use std::io::{Cursor, Seek, SeekFrom, Write};
use tokio::process::Command;

pub struct SseClient {
    response: Response<Body>,
    lines: Lines,
}

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
    pub command: fn () -> Command
}

impl SseClient {
    pub async fn connect(host: &str) -> Result<Self> {
        let client = Client::new();

        let req = Request::builder()
            .method("POST")
            .uri(host)
            .header(header::ACCEPT, "text/event-stream")
            .body(Body::empty())?;

        let response = client.request(req).await?;

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
        self.0
            .write(bytes.as_ref())
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
            self.0.seek(SeekFrom::End(0))?;

            Ok(Some(line))
        } else {
            Ok(None)
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
