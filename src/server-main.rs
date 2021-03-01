extern crate ctrlc;
extern crate curator;

use std::{
    sync::atomic::{AtomicBool, Ordering},
    sync::Arc,
    time::Duration,
};

use tokio::time::sleep;

use curator::{prelude::*, server::Curator};

#[actix_web::main]
async fn main() -> Result<()> {
    env_logger::init();

    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(start_server())?;
    Ok(())
}

async fn start_server() -> Result<()> {
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })?;

    let _curator = Curator::start()?;
    while running.load(Ordering::SeqCst) {
        sleep(Duration::from_millis(100)).await;
    }
    println!("Got Ctrl-C! Shuting down...");
    //curator.stop(false).await;

    Ok(())
}
