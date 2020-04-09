use chrono::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// Structs used in Curator<->Agent communication protocol
pub mod agent {
    use super::*;

    pub const RUN_TASK_EVENT_NAME: &str = "run-task";

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    pub struct Agent {
        pub application: String,
        pub instance: String,
        pub tasks: Vec<Task>,
    }

    impl Agent {
        pub fn new(application: &str, instance: &str, tasks: Vec<Task>) -> Self {
            Self {
                application: application.to_string(),
                instance: instance.to_string(),
                tasks,
            }
        }
    }

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    pub struct RunTask {
        pub task_id: String,
        pub execution: Uuid,
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
        pub task_id: String,
        pub agent: AgentRef,
    }

    #[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
    pub struct Execution {
        pub id: Uuid,
        pub agent: AgentRef,
        pub output: String,
        pub task: Task,
        pub status: ExecutionStatus,
        pub started: DateTime<Utc>,
        pub finished: Option<DateTime<Utc>>,
    }

    impl Execution {
        pub fn new(id: Uuid, task: Task, agent: AgentRef) -> Self {
            Self {
                id,
                agent,
                task,
                output: String::new(),
                status: ExecutionStatus::INITIATED,
                started: Utc::now(),
                finished: None,
            }
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Hash, Eq)]
pub struct Task {
    pub id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Hash, Eq, PartialEq)]
pub struct AgentRef {
    pub application: String,
    pub instance: String,
}

impl From<agent::Agent> for AgentRef {
    fn from(agent: agent::Agent) -> Self {
        AgentRef {
            application: agent.application,
            instance: agent.instance,
        }
    }
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

        match self {
            REJECTED | COMPLETED | FAILED => true,
            _ => false,
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
    use crate::errors::*;
    use serde::de::DeserializeOwned;
    use serde_json::{json, Value};
    use std::fmt::Debug;

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
                id: "my-id".to_string(),
            },
            json!({"id": "my-id"}),
        )?;

        Ok(())
    }

    #[test]
    fn check_agent() -> Result<()> {
        assert_json_eq(
            agent::Agent {
                application: "my-app".into(),
                instance: "single".into(),
                tasks: vec![Task {
                    id: "my-task".into(),
                }],
            },
            json!({
                "application": "my-app",
                "instance": "single",
                "tasks": [
                    { "id": "my-task" }
                ]
            }),
        )?;

        Ok(())
    }

    #[test]
    fn check_run_task() -> Result<()> {
        assert_json_eq(
            agent::RunTask {
                task_id: "clean".into(),
                execution: Uuid::parse_str("596cf5b4-70ba-11ea-bc55-0242ac130003")?,
            },
            json!({
                "task_id": "clean",
                "execution": "596cf5b4-70ba-11ea-bc55-0242ac130003"
            }),
        )?;

        Ok(())
    }

    #[test]
    fn check_client_run_task() -> Result<()> {
        assert_json_eq(
            client::RunTask {
                task_id: "clean".into(),
                agent: AgentRef {
                    application: "app".into(),
                    instance: "single".into(),
                },
            },
            json!({
                "task_id": "clean",
                "agent": {
                    "application": "app",
                    "instance": "single"
                }
            }),
        )?;

        Ok(())
    }

    #[test]
    fn check_agent_ref() -> Result<()> {
        assert_json_eq(
            AgentRef {
                instance: "foo".into(),
                application: "app".into(),
            },
            json!({
                "instance": "foo",
                "application": "app"
            }),
        )?;

        Ok(())
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
        )?;

        Ok(())
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
        )?;

        Ok(())
    }

    #[test]
    fn check_client_execution() -> Result<()> {
        assert_json_eq(
            client::Execution {
                id: Uuid::parse_str("596cf5b4-70ba-11ea-bc55-0242ac130003")?,
                status: ExecutionStatus::RUNNING,
                output: "output".into(),
                agent: AgentRef {
                    application: "app".into(),
                    instance: "single".into(),
                },
                task: Task { id: "clean".into() },
                started: Utc.ymd(2017, 11, 3).and_hms(9, 10, 11),
                finished: None,
            },
            json!({
                "id": "596cf5b4-70ba-11ea-bc55-0242ac130003",
                "status": "RUNNING",
                "output": "output",
                "started": "2017-11-03T09:10:11Z",
                "finished": null,
                "task": {
                    "id": "clean"
                },
                "agent": {
                    "application": "app",
                    "instance": "single"
                }
            }),
        )?;

        Ok(())
    }

    #[test]
    fn check_stop_task() -> Result<()> {
        assert_json_eq(
            agent::StopTask {
                execution: Uuid::parse_str("596cf5b4-70ba-11ea-bc55-0242ac130003")?,
            },
            json!({ "execution": "596cf5b4-70ba-11ea-bc55-0242ac130003" }),
        )?;

        Ok(())
    }

    fn assert_json_eq<T>(value: T, json: Value) -> Result<()>
    where
        T: Serialize + DeserializeOwned + PartialEq + Debug,
    {
        assert_eq!(serde_json::to_value(&value)?, json);
        assert_eq!(value, serde_json::from_value(json)?);
        Ok(())
    }
}
