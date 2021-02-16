use bollard::{
    container::{Config, LogsOptions, WaitContainerOptions},
    Docker,
};
use futures::StreamExt;

type BollardError = bollard::errors::Error;

type Result<T> = std::result::Result<T, BollardError>;

#[tokio::main]
async fn main() -> Result<()> {
    let docker = Docker::connect_with_unix_defaults()?;

    let config = Config {
        image: Some("openjdk:11.0-jdk"),
        cmd: Some(vec!["date"]),
        ..Default::default()
    };

    let container = docker.create_container::<&str, _>(None, config).await?;

    println!("Container id: {}", container.id);

    docker.start_container::<&str>(&container.id, None).await?;

    let options = Some(LogsOptions::<String> {
        stdout: true,
        ..Default::default()
    });

    let mut stream = docker.logs(&container.id, options);

    while let Some(p) = stream.next().await {
        match p {
            Ok(out) => print!("{}", out),
            Err(e) => eprintln!("{}", e),
        }
    }
    println!("Output finished");

    println!("Waiting...");
    let options = Some(WaitContainerOptions {
        condition: "not-running",
    });
    docker
        .wait_container(&container.id, options)
        .next()
        .await
        .unwrap()?;
    docker
        .remove_container(&container.id, Default::default())
        .await?;

    Ok(())
}
