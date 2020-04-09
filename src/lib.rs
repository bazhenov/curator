#[macro_use]
extern crate failure;

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

    pub use log::{info, trace, warn, error};
}

pub type Shared<T> = Arc<Mutex<T>>;

pub fn shared<T>(obj: T) -> Shared<T> {
    Arc::new(Mutex::new(obj))
}

pub mod errors {
    pub type Result<T> = std::result::Result<T, failure::Error>;
    pub use failure::bail;
    pub use failure::ResultExt;

    pub fn report_errors(e: failure::Error) {
        for cause in e.iter_chain() {
            if let Some(name) = cause.name() {
                log::error!("{}: {}", name, cause);
            } else {
                log::error!("{}", cause);
            }
        }
    }
}
