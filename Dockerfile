FROM rust:1.86 AS rust

FROM golang:1.24 AS gh
RUN GOBIN=/bin-go go install github.com/cli/cli@v2.72.0

FROM ubuntu:25.04 AS base
RUN apt update && apt install curl zip make git gcc gpg build-essential libssl-dev g++-x86-64-linux-gnu libc6-dev-amd64-cross -y

COPY --from=gh /bin-go/gh /usr/local/bin/gh
COPY --from=rust /usr/local/cargo/bin /usr/local/cargo/bin
COPY --from=rust /usr/local/rustup /usr/local/rustup
COPY --from=rust /usr/local/cargo /usr/local/cargo

ENV RUSTUP_HOME=/usr/local/rustup
ENV CARGO_HOME=/usr/local/cargo
ENV PATH="/usr/local/cargo/bin:${PATH}"

RUN chmod -R 777 /usr/local/rustup
RUN chmod -R 777 /usr/local/cargo


FROM base AS dev
RUN apt update && apt install -y bash-completion
RUN useradd -m -s /bin/bash dev
USER dev


FROM base AS ci
RUN useradd -m -s /bin/bash ci && \
    mkdir -p /build && \
    chown -R ci:ci /build

WORKDIR /build
COPY . .
USER ci