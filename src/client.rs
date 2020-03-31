use crate::errors::*;
use bytes::Bytes;
use hyper::error::Error as HyperError;
use hyper::{body::HttpBody as _, client::HttpConnector, header, Body, Client, Request, Response};
use std::io::{self, BufRead, Cursor, Seek, SeekFrom, Write};

pub struct SseClient {
    client: Client<HttpConnector, Body>,
    response: Response<Body>,
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

        Ok(Self { client, response })
    }

    pub async fn next(&mut self) -> Option<Result<Bytes>> {
        self.response
            .body_mut()
            .data()
            .await
            .map(|r| r.chain_err(|| "Unable to read from server"))
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

    fn feed(&mut self, bytes: &[u8]) -> Result<Vec<String>> {
        self.0
            .write(bytes)
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
