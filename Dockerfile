# syntax = docker/dockerfile:experimental
FROM rust:1.42.0-stretch AS builder
WORKDIR /opt

ADD src /opt/src
ADD Cargo.toml /opt/Cargo.toml
ADD Cargo.lock /opt/Cargo.lock

RUN --mount=type=cache,target=./target cargo build --release

RUN --mount=type=cache,target=./target cp /opt/target/release/curator-server /opt/curator-server
RUN --mount=type=cache,target=./target cp /opt/target/release/curator-agent /opt/curator-agent

FROM centos:centos8 AS curator-server
COPY --from=builder /opt/curator-server /opt/curator-server
ENTRYPOINT ["/opt/curator-server"]

FROM centos:centos8 AS curator-agent
COPY --from=builder /opt/curator-agent /opt/curator-agent
ENTRYPOINT ["/opt/curator-agent"]