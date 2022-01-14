extern crate curator;

use curator::{prelude::*, server::Curator};

#[actix_web::main]
#[logfn(ok = "Trace", err = "Error")]
async fn main() -> Result<()> {
    env_logger::init();

    let (acceptor, _) = Curator::start()?;
    acceptor.await?;

    Ok(())
}
