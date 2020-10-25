extern crate ctrlc;
extern crate curator;

use std::{
    sync::atomic::{AtomicBool, Ordering},
    sync::Arc,
    time::Duration,
};

use tokio::time::delay_for;

use curator::{prelude::*, server::Curator};

#[actix_rt::main]
async fn main() -> Result<()> {
    env_logger::init();

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })?;

    let curator = Curator::start()?;
    while running.load(Ordering::SeqCst) {
        delay_for(Duration::from_millis(100)).await;
    }
    println!("Got Ctrl-C! Shuting down...");
    curator.stop(true).await;

    Ok(())
}
