#[macro_use]
extern crate error_chain;

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
    pub use error_chain::bail;
    pub use error_chain::ChainedError;
}

pub type Shared<T> = Arc<Mutex<T>>;

pub fn shared<T>(obj: T) -> Shared<T> {
    Arc::new(Mutex::new(obj))
}

pub mod errors {
    error_chain! {
      foreign_links {
        Fmt(::std::fmt::Error);
        Utf(::std::string::FromUtf8Error);
        Io(::std::io::Error);
        Http(::actix_web::http::Error);
        Hyper(hyper::Error);
        Json(::serde_json::Error);
        Uuid(uuid::Error);
        Tokio(tokio::sync::mpsc::error::SendError<std::result::Result<bytes::Bytes, std::io::Error>>);
      }

      errors {
        ClientConnectError(uri: String, status_code: u16) {
          display("Curator-server error HTTP/{} error ({})", status_code, uri)
        }
      }
    }
}
