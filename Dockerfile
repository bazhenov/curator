# syntax = docker/dockerfile:experimental
FROM rust:1.44.1-slim AS builder
WORKDIR /opt

ADD src ./src
ADD tests ./tests
ADD Cargo.toml ./Cargo.toml
ADD Cargo.lock ./Cargo.lock

# Building project including tests
RUN --mount=type=cache,target=./target \
    --mount=type=cache,target=/usr/local/cargo/registry \
  cargo build --release --tests && \
  cp ./target/release/curator-server /opt && \
  cp ./target/release/curator-agent /opt

# Copying production executables
RUN --mount=type=cache,target=./target \
  cp ./target/release/curator-server /opt && \
  cp ./target/release/curator-agent /opt

# Copying test executables
# ?????????????????? stands for exactly 18 characters representing hashcode of tests-executables
RUN --mount=type=cache,target=./target \
  cp ./target/release/deps/docker-???????????????? /opt/test-docker && \
  cp ./target/release/deps/curator_sse_test-???????????????? /opt/test-curator_sse_test

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