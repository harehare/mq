# syntax=docker/dockerfile:1
FROM rust:1.96.1-slim@sha256:31ee7fc65186be7e0e0ccb3f2ca305f14e4739e7642a1ae65753aa5d7b874523 AS builder

WORKDIR /usr/src/app
COPY . /usr/src/app

RUN cargo build -p mq-run --bin mq --release

FROM gcr.io/distroless/cc:nonroot

COPY --from=builder --chown=nonroot:nonroot /usr/src/app/target/release/mq /usr/local/bin/mq

ENTRYPOINT [ "mq" ]
