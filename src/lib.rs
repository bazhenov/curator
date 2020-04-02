#[macro_use]
extern crate error_chain;

pub mod client;
pub mod protocol;
pub mod server;

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
