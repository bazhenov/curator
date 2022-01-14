# syntax = docker/dockerfile:experimental
FROM rust:1.51-slim AS builder
WORKDIR /opt

ADD src ./src
ADD tests ./tests
ADD Cargo.toml ./Cargo.toml
ADD Cargo.lock ./Cargo.lock

# Building project, unit and integration tests
RUN --mount=type=cache,target=./target \
  --mount=type=cache,target=/usr/local/cargo/registry \
  cargo build --release --bins && \
  cargo build --release --tests && \
  cargo build --release --test='*'

# Copying production executables
RUN --mount=type=cache,target=./target \
  cp ./target/release/curator-server /opt && \
  cp ./target/release/curator-agent /opt

# Copying test executables
# ?????????????????? stands for exactly 18 characters representing hashcode of tests-executables
RUN --mount=type=cache,target=./target \
  cp -T ./target/release/deps/docker-???????????????? /opt/test-docker && \
  cp -T ./target/release/deps/curator_sse_test-???????????????? /opt/test-curator_sse_test && \
  cp -T ./target/release/deps/curator-???????????????? /opt/test-curator

FROM centos:centos8 AS curator-server
WORKDIR /opt
COPY --from=builder /opt/curator-server /curator-server
EXPOSE 8080
ENTRYPOINT /curator-server

# Base docker image for creating agent images
# with custom discovery utilities
FROM centos:centos8 AS curator-agent
COPY --from=builder /opt/curator-agent /curator-agent

WORKDIR /opt
ENTRYPOINT ["/curator-agent"]
