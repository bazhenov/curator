#!/usr/bin/env sh

cargo build --bin curator-agent

cd ./local

RUST_LOG=curator=info ../target/debug/curator-agent \
  --application my --instance single --host localhost:8080