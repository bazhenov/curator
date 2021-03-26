#[macro_use]
extern crate anyhow;

pub mod agent;
pub mod docker;
pub mod protocol;
pub mod server;

pub mod prelude {
    pub use super::errors::*;
    pub use super::protocol::*;
    pub use anyhow::Error as AnyhowError;
    pub use anyhow::{ensure, Context, Result};
    pub use log_derive::logfn;
    pub use thiserror::Error;
    pub type IoResult<T> = std::result::Result<T, std::io::Error>;

    pub use log::{debug, error, info, trace, warn};
}

pub mod errors {
    use anyhow::Error;

    pub fn log_errors(e: &Error) {
        log::error!("{}", format_error_chain(e));
    }

    pub fn format_error_chain(error: &Error) -> String {
        let mut description = String::new();
        for (i, e) in error.chain().enumerate() {
            if i == 0 {
                description.push_str(&format!("{}\n", e));
            } else {
                description.push_str(&format!("Caused by: {}\n", e));
            }
        }
        description
    }
}

#[cfg(test)]
pub mod tests {
    use super::prelude::*;
    use serde::de::DeserializeOwned;
    use serde::Serialize;
    use serde_json::Value;
    use std::fmt::Debug;

    /// Checks serializing/deserializing cycle of value and json
    pub fn assert_json_eq<T>(value: T, json: Value) -> Result<()>
    where
        T: Serialize + DeserializeOwned + PartialEq + Debug,
    {
        assert_eq!(serde_json::to_value(&value)?, json);
        assert_eq!(value, serde_json::from_value(json)?);

        Ok(())
    }

    /// Checks deserializing cycle of value and json
    pub fn assert_json_reads<T>(value: T, json: Value) -> Result<()>
    where
        T: DeserializeOwned + PartialEq + Debug,
    {
        assert_eq!(value, serde_json::from_value(json)?);

        Ok(())
    }
}
