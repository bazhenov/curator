#[macro_use]
extern crate error_chain;

pub mod client;
pub mod server;
mod sse;

mod errors {

    error_chain! {
      foreign_links {
        Fmt(::std::fmt::Error);
        Utf(::std::string::FromUtf8Error);
        Io(::std::io::Error);
        Http(::actix_web::http::Error);
        Hyper(hyper::error::Error);
      }

      errors {
        UnknownError
      }
    }
}
