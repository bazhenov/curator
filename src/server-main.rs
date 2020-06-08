extern crate curator;

use std::{thread, time::Duration};

use curator::{prelude::*, server::Curator};

#[actix_rt::main]
async fn main() -> Result<()> {
    env_logger::init();

    Curator::start()?;
    loop {
        thread::sleep(Duration::from_secs(1));
    }
}
