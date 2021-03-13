use curator::prelude::*;
use bollard::Docker;
use futures::stream::StreamExt;

#[tokio::main]
async fn main() -> Result<()> {
    let docker = Docker::connect_with_local_defaults()?;

    let mut events = docker.events::<String>(None);
    while let Some(event) = events.next().await {
        let event = event?;
        println!("- type: {:?}", event.typ);
        println!("  action: {:?}", event.action);
        println!("  actor: {:?}", event.actor);
        println!();
    }
    Ok(())
}
