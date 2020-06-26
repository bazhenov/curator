#[macro_use]
extern crate anyhow;

pub mod agent;
pub mod protocol;
pub mod server;

use std::sync::Arc;
use std::sync::Mutex;

pub mod prelude {
    pub use super::errors::*;
    pub use super::protocol::*;
    pub use super::shared;
    pub use super::Shared;
    pub use anyhow::{Context, Result};
    pub use thiserror::Error;
    pub type IoResult<T> = std::result::Result<T, std::io::Error>;

    pub use log::{error, info, trace, warn};
}

pub type Shared<T> = Arc<Mutex<T>>;

pub fn shared<T>(obj: T) -> Shared<T> {
    Arc::new(Mutex::new(obj))
}

pub mod errors {
    use anyhow::Error;

    pub fn log_errors(e: &Error) {
        log::error!("{}", e);
    }

    pub fn format_error_chain(e: &Error) -> String {
        format!("{}", e)
    }
}
