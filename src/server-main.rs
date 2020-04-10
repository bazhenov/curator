extern crate curator;

use std::{thread, time::Duration};

use termion::clear;

use curator::{prelude::*, server::Curator};

#[actix_rt::main]
async fn main() -> Result<()> {
    let curator = Curator::start()?;

    loop {
        {
            let agents = curator.agents.lock().unwrap();
            println!("{}", clear::All);
            for a in agents.values() {
                println!("{}@{}", a.agent.instance, a.agent.application);
                for task in a.tasks.iter() {
                    println!(" - {}", task.id);
                }
            }
        }

        thread::sleep(Duration::from_secs(1));
    }
}
