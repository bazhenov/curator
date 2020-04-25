#!/usr/bin/env sh

cargo build --bin curator-agent

cd ./local

sudo RUST_LOG=curator=info ../target/debug/curator-agent \
  --application my --instance single --host localhost:8080