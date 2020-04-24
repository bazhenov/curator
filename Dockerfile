# syntax = docker/dockerfile:experimental
FROM rust:1.42.0-stretch AS builder
WORKDIR /opt

ADD src ./src
ADD Cargo.toml ./Cargo.toml
ADD Cargo.lock ./Cargo.lock

RUN --mount=type=cache,target=./target \
    --mount=type=cache,target=~/.cargo/registry \
  cargo build --release

RUN --mount=type=cache,target=./target cp ./target/release/curator-server /opt
RUN --mount=type=cache,target=./target cp ./target/release/curator-agent /opt

FROM centos:centos8 AS curator-server
COPY --from=builder /opt/curator-server /opt/curator-server
ENTRYPOINT ["/opt/curator-server"]

FROM centos:centos8 AS curator-agent
COPY --from=builder /opt/curator-agent /opt/curator-agent
ENTRYPOINT ["/opt/curator-agent"]