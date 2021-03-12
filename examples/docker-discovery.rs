use bollard::Docker;
use curator::{
    agent::TaskDef,
    docker::{list_running_containers, run_toolchain_discovery},
    prelude::*,
};
use termion::{clear, color, style};
use tokio::time::{sleep, Duration};

type TaskSet = Vec<(String, Vec<TaskDef>)>;

#[tokio::main]
async fn main() -> Result<()> {
    let docker = Docker::connect_with_local_defaults()?;

    let toolchain = "bazhenov.me/curator/toolchain-example:dev";

    loop {
        let task_set = build_task_set(&docker, toolchain).await?;
        println!("{}", clear::All);
        if !task_set.is_empty() {
            println!("Running containers:");
            for (container_id, tasks) in task_set {
                println!(
                    " + {green}{bold}{id}{reset}",
                    id = container_id,
                    bold = style::Bold,
                    green = color::Fg(color::Green),
                    reset = style::Reset,
                );
                if !tasks.is_empty() {
                    for task in tasks {
                        println!("    - {:?}", task);
                    }
                } else {
                    println!(
                        "      {yellow}No tasks found{reset}",
                        yellow = color::Fg(color::Yellow),
                        reset = style::Reset,
                    )
                }
            }
        } else {
            println!(
                "{yellow}No containers found{reset}",
                yellow = color::Fg(color::Yellow),
                reset = style::Reset,
            )
        }
        sleep(Duration::from_secs(1)).await;
    }
}

async fn build_task_set(docker: &Docker, toolchain: &str) -> Result<TaskSet> {
    let mut result = vec![];
    for id in list_running_containers(&docker).await? {
        let tasks = run_toolchain_discovery(&docker, &id, toolchain).await?;
        let tuple = (id, tasks);
        result.push(tuple);
    }
    Ok(result)
}
