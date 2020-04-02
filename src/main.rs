extern crate curator;
use curator::errors::*;
use curator::server::Curator;
use std::{thread, time::Duration};
use termion::clear;

#[actix_rt::main]
async fn main() -> Result<()> {
    let curator = Curator::start()?;

    loop {
        {
            let agents = curator.agents.lock().unwrap();
            println!("{}", clear::All);
            for agent in agents.iter() {
                println!("{}@{}", agent.instance, agent.application);
                for task in agent.tasks.iter() {
                    println!(" - {}", task);
                }
            }
        }

        thread::sleep(Duration::from_secs(1));
        let event = (
            Some("run-task".to_string()),
            r#"{"task_id": "w", "execution": "596cf5b4-70ba-11ea-bc55-0242ac130003"}"#.to_string(),
        );
        curator.notify_all(&event);

        thread::sleep(Duration::from_secs(1));
        let event = (
            Some("run-task".to_string()),
            r#"{"task_id": "uptime", "execution": "596cf5b4-70ba-11ea-bc55-0242ac130003"}"#
                .to_string(),
        );
        curator.notify_all(&event);
    }
}
