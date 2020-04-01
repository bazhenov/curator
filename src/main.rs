extern crate curator;
use curator::errors::*;
use curator::server::Curator;
use std::{thread, time::Duration};

#[actix_rt::main]
async fn main() -> Result<()> {
    let curator = Curator::start()?;

    loop {
        thread::sleep(Duration::from_secs(1));
        let event = (Some("run-task".to_string()), "{}".to_string());
        curator.notify_all(&event);
    }
}
