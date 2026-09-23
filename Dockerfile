# 忆栈同步服务镜像。构建上下文为仓库根目录。
#   docker build -t memstack-server .
#   podman build -t memstack-server .
# 网络受限时可用镜像源：
#   --build-arg RUST_IMAGE=docker.m.daocloud.io/library/rust:1.97-bookworm
#   --build-arg RUNTIME_IMAGE=docker.m.daocloud.io/library/debian:bookworm-slim

ARG RUST_IMAGE=rust:1.97-bookworm
ARG RUNTIME_IMAGE=debian:bookworm-slim

FROM ${RUST_IMAGE} AS builder
RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libsqlite3-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --release -p memstack-server

FROM ${RUNTIME_IMAGE}
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libsqlite3-0 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home /data memstack \
    && mkdir -p /data \
    && chown memstack:memstack /data
COPY --from=builder /src/target/release/memstack-server /usr/local/bin/memstack-server
USER memstack
ENV MEMSTACK_BIND=0.0.0.0:8080 \
    MEMSTACK_DATA=/data
VOLUME ["/data"]
EXPOSE 8080
ENTRYPOINT ["memstack-server"]
