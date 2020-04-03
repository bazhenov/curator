use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Structs used in Curator<->agent communication protocol
pub mod agent {
    use super::*;

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
}

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
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Hash, Eq)]
pub struct Task {
    pub id: String,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
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

pub enum ExecutionStatus {
    INITIATED,
    REJECTED,
    RUNNING,
    FAILED,
    COMPLETED,
}

#[cfg(test)]
mod tests {

    use super::{agent::*, client::*, *};
    use crate::errors::*;
    use serde::de::DeserializeOwned;
    use serde_json::{json, Value};
    use std::fmt::Debug;

    #[test]
    fn check_execution_ref() -> Result<()> {
        assert_json_eq(
            ExecutionRef {
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
            Agent {
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
    fn check_stop_task() -> Result<()> {
        assert_json_eq(
            StopTask {
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
