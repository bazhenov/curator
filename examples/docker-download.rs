use bollard::{container::DownloadFromContainerOptions, Docker};
use curator::docker::Container;
use curator::prelude::*;
use futures::{stream::Stream, stream::StreamExt};
use std::{fs::OpenOptions, io::Write};

#[tokio::main]
async fn main() -> Result<()> {
    let docker = Docker::connect_with_unix_defaults()?;

    let container = Container::start(&docker, "alpine:3.12", Some(vec!["sleep", "5"])).await?;
    println!("Container id: {}", container.id);

    let options = DownloadFromContainerOptions { path: "/etc" };
    let stream = docker
        .download_from_container(&container.id, Some(options))
        .map(|batch| batch.context("Unable to read"));

    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .open("./target/export.tar")?;

    write_stream_to_file(stream, file).await
}

async fn write_stream_to_file<T: AsRef<[u8]>>(
    mut stream: impl Stream<Item = Result<T>> + Unpin,
    mut target: impl Write,
) -> Result<()> {
    while let Some(batch) = stream.next().await {
        target.write(batch?.as_ref())?;
    }

    Ok(())
}
