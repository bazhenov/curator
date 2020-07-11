extern crate ctrlc;
extern crate curator;

use std::{
    sync::atomic::{AtomicBool, Ordering},
    sync::Arc,
    thread,
    time::Duration,
};

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
        thread::sleep(Duration::from_millis(100));
    }
    println!("Got Ctrl-C! Shuting down...");
    curator.stop(true).await;

    Ok(())
}
