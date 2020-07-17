# syntax = docker/dockerfile:experimental
FROM rust:1.44.1-slim AS builder
WORKDIR /opt

ADD src ./src
ADD Cargo.toml ./Cargo.toml
ADD Cargo.lock ./Cargo.lock

RUN --mount=type=cache,target=./target \
    --mount=type=cache,target=/usr/local/cargo/registry \
  cargo build --release && \
  cp ./target/release/curator-server /opt && \
  cp ./target/release/curator-agent /opt

FROM centos:centos8 AS curator-server
WORKDIR /opt
COPY --from=builder /opt/curator-server /curator-server
EXPOSE 8080
ENTRYPOINT ["/curator-server"]

# Base docker image for creating agent images
# with custom discovery utilities
FROM centos:centos8 AS curator-agent
COPY --from=builder /opt/curator-agent /curator-agent

WORKDIR /opt
ENTRYPOINT /curator-agent \
	--host $CURATOR_HOST \
	--name $AGENT_NAME

# Version of curator-agent for local development only
FROM centos:centos8 AS curator-agent-dev

RUN --mount=type=cache,target=/var/cache/dnf yum -y update
RUN --mount=type=cache,target=/var/cache/dnf yum -y install jq lsof

COPY local/lsof.discovery.sh /opt/lsof.discovery.sh
COPY --from=builder /opt/curator-agent /curator-agent

WORKDIR /opt
ENTRYPOINT /curator-agent \
	--host $CURATOR_HOST \
	--name $AGENT_NAME