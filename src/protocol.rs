use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct Task {
    id: String,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct Agent {
    application: String,
    instance: String,
    tasks: Vec<Task>,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct RunTask {
    task_id: String,
    execution: Uuid,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct StopTask {
    execution: Uuid,
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::errors::*;
    use serde::de::DeserializeOwned;
    use serde_json::{json, Value};
    use std::fmt::Debug;

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
                application: String::from("my-app"),
                instance: String::from("single"),
                tasks: vec![Task {
                    id: String::from("my-task"),
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
            RunTask {
                task_id: String::from("clean"),
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
