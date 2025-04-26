FROM rust:1.86 AS rust

FROM ubuntu:25.04 AS base
RUN apt update && apt install curl zip make git gcc gpg build-essential libssl-dev g++-x86-64-linux-gnu libc6-dev-amd64-cross -y

RUN curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg | gpg --dearmor -o /usr/share/keyrings/githubcli-archive-keyring.gpg;
RUN echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" | tee /etc/apt/sources.list.d/github-cli.list > /dev/null;
RUN apt update && apt install -y gh;

COPY --from=rust /usr/local/cargo/bin /usr/local/cargo/bin
COPY --from=rust /usr/local/rustup /usr/local/rustup
COPY --from=rust /usr/local/cargo /usr/local/cargo

ENV RUSTUP_HOME=/usr/local/rustup
ENV CARGO_HOME=/usr/local/cargo
ENV PATH="/usr/local/cargo/bin:${PATH}"

RUN chmod -R 777 /usr/local/rustup
RUN chmod -R 777 /usr/local/cargo


FROM base AS dev
RUN useradd -m -s /bin/bash dev && \
    mkdir -p /workspaces/launcher && \
    chown -R dev:dev /workspaces/launcher
USER dev


FROM base AS ci
RUN useradd -m -s /bin/bash ci && \
    mkdir -p /workspaces/launcher && \
    chown -R ci:ci /workspaces/launcher

WORKDIR /build
COPY . .
USER ci