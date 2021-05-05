use chrono::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use uuid::Uuid;

/// Structs used in Curator<->Agent communication protocol
pub mod agent {
    use super::*;

    pub const RUN_TASK_EVENT_NAME: &str = "run-task";

    #[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
    pub struct Agent {
        pub name: String,
        pub tasks: Vec<Task>,
    }

    impl Agent {
        pub fn new(name: &str, tasks: Vec<Task>) -> Self {
            Self {
                name: name.to_string(),
                tasks,
            }
        }
    }

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    pub struct RunTask {
        pub task_name: String,
        pub container_id: String,
        pub execution_id: Uuid,
    }

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    pub struct StopTask {
        pub execution: Uuid,
    }

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    pub struct ExecutionReport {
        pub id: Uuid,
        pub status: ExecutionStatus,
        pub stdout_append: Option<String>,
    }
}

/// Structs used in Curator<->Client communication protocol
pub mod client {
    pub use super::agent::Agent;
    use super::*;

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    pub struct ExecutionRef {
        pub execution_id: uuid::Uuid,
    }

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    pub struct RunTask {
        pub task_name: String,
        pub container_id: String,
        pub agent: String,
    }

    #[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
    pub struct Execution {
        pub id: Uuid,
        pub agent: String,
        pub output: String,
        pub task: Task,
        pub status: ExecutionStatus,
        pub started: DateTime<Utc>,
        pub finished: Option<DateTime<Utc>>,
        pub artifact_size: Option<usize>,
    }

    impl Execution {
        pub fn new(id: Uuid, task: Task, agent: String) -> Self {
            Self {
                id,
                agent,
                task,
                output: String::new(),
                status: ExecutionStatus::INITIATED,
                started: Utc::now(),
                finished: None,
                artifact_size: None,
            }
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Eq, Default)]
pub struct Task {
    pub name: String,
    pub container_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub labels: BTreeMap<String, String>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Copy, Clone)]
pub enum ExecutionStatus {
    INITIATED,
    REJECTED,
    RUNNING,
    FAILED,
    COMPLETED,
}

impl ExecutionStatus {
    pub fn is_terminal(self) -> bool {
        use ExecutionStatus::*;

        matches!(self, REJECTED | COMPLETED | FAILED)
    }

    pub fn from_unix_exit_code(exit_code: i32) -> Self {
        if exit_code == 0 {
            Self::COMPLETED
        } else {
            Self::FAILED
        }
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct Execution {
    pub id: uuid::Uuid,
    pub status: ExecutionStatus,
    pub stdout: String,
}

impl Execution {
    pub fn new(id: Uuid) -> Self {
        Self {
            id,
            status: ExecutionStatus::INITIATED,
            stdout: String::new(),
        }
    }

    pub fn with_arc(id: Uuid) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self::new(id)))
    }
}

#[cfg(test)]
mod tests {

    use super::{agent, client, *};
    use crate::prelude::*;
    use crate::tests::*;
    use maplit::btreemap;
    use serde_json::json;

    #[test]
    fn check_execution_ref() -> Result<()> {
        assert_json_eq(
            client::ExecutionRef {
                execution_id: Uuid::parse_str("596cf5b4-70ba-11ea-bc55-0242ac130003")?,
            },
            json!({"execution_id": "596cf5b4-70ba-11ea-bc55-0242ac130003"}),
        )?;

        Ok(())
    }

    #[test]
    fn check_task() -> Result<()> {
        assert_json_eq(
            Task {
                name: "my-id".to_string(),
                container_id: "49d4ebd43a56".to_string(),
                description: Some("some description".to_string()),
                ..Default::default()
            },
            json!({"name": "my-id", "container_id": "49d4ebd43a56", "description": "some description"}),
        )?;

        assert_json_reads(
            Task {
                name: "my-id".to_string(),
                description: Some("some description".to_string()),
                labels: btreemap! {"shell".into() => "unix".into()},
                ..Default::default()
            },
            json!({"name": "my-id", "container_id": "", "description": "some description", "labels": {"shell": "unix"}}),
        )
    }

    #[test]
    fn check_agent() -> Result<()> {
        assert_json_eq(
            agent::Agent {
                name: "my-app".into(),
                tasks: vec![Task {
                    name: "my-task".into(),
                    description: Some("some description".to_string()),
                    ..Default::default()
                }],
            },
            json!({
                "name": "my-app",
                "tasks": [
                    { "name": "my-task", "container_id": "", "description": "some description" }
                ]
            }),
        )
    }

    #[test]
    fn check_run_task() -> Result<()> {
        assert_json_eq(
            agent::RunTask {
                task_name: "clean".into(),
                container_id: "49d4ebd43a56".into(),
                execution_id: Uuid::parse_str("596cf5b4-70ba-11ea-bc55-0242ac130003")?,
            },
            json!({
                "task_name": "clean",
                "container_id": "49d4ebd43a56",
                "execution_id": "596cf5b4-70ba-11ea-bc55-0242ac130003"
            }),
        )
    }

    #[test]
    fn check_client_run_task() -> Result<()> {
        assert_json_eq(
            client::RunTask {
                task_name: "clean".into(),
                container_id: "0f8734960f42".into(),
                agent: "app".into(),
            },
            json!({
                "task_name": "clean",
                "container_id": "0f8734960f42",
                "agent": "app"
            }),
        )
    }

    #[test]
    fn check_execution_report() -> Result<()> {
        assert_json_eq(
            agent::ExecutionReport {
                id: Uuid::parse_str("596cf5b4-70ba-11ea-bc55-0242ac130003")?,
                status: ExecutionStatus::RUNNING,
                stdout_append: Some("output".into()),
            },
            json!({
                "id": "596cf5b4-70ba-11ea-bc55-0242ac130003",
                "status": "RUNNING",
                "stdout_append": "output"
            }),
        )
    }

    #[test]
    fn check_execution_status() -> Result<()> {
        assert_json_eq(
            Execution {
                id: Uuid::parse_str("596cf5b4-70ba-11ea-bc55-0242ac130003")?,
                status: ExecutionStatus::RUNNING,
                stdout: "output".into(),
            },
            json!({
                "id": "596cf5b4-70ba-11ea-bc55-0242ac130003",
                "status": "RUNNING",
                "stdout": "output"
            }),
        )
    }

    #[test]
    fn check_client_execution() -> Result<()> {
        assert_json_eq(
            client::Execution {
                id: Uuid::parse_str("596cf5b4-70ba-11ea-bc55-0242ac130003")?,
                status: ExecutionStatus::RUNNING,
                output: "output".into(),
                agent: "app".into(),
                task: Task {
                    name: "clean".into(),
                    ..Default::default()
                },
                started: Utc.ymd(2017, 11, 3).and_hms(9, 10, 11),
                finished: None,
                artifact_size: Some(3315),
            },
            json!({
                "id": "596cf5b4-70ba-11ea-bc55-0242ac130003",
                "status": "RUNNING",
                "output": "output",
                "started": "2017-11-03T09:10:11Z",
                "finished": null,
                "task": {
                    "name": "clean",
                    "container_id": ""
                },
                "agent": "app",
                "artifact_size": 3315,
            }),
        )
    }

    #[test]
    fn check_stop_task() -> Result<()> {
        assert_json_eq(
            agent::StopTask {
                execution: Uuid::parse_str("596cf5b4-70ba-11ea-bc55-0242ac130003")?,
            },
            json!({ "execution": "596cf5b4-70ba-11ea-bc55-0242ac130003" }),
        )
    }
}
