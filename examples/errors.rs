use curator::prelude::*;
use std::fs::File;

fn main() {
    env_logger::init();

    match outer() {
        Ok(_) => {
            println!("Ok");
        }
        Err(e) => {
            log_errors(&e);
        }
    }
}

fn outer() -> Result<File> {
    File::open("./not-found").context("Failed while reading file")
}
