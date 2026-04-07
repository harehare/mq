# syntax=docker/dockerfile:1
FROM rust:1.94.1-slim@sha256:f1a887e70d5bb8773c3248c096ae296d7e5618dc41b51685a7759d6dc9ed0551 AS builder

WORKDIR /usr/src/app
COPY . /usr/src/app

RUN cargo build -p mq-run --bin mq --release

FROM gcr.io/distroless/cc:nonroot

COPY --from=builder --chown=nonroot:nonroot /usr/src/app/target/release/mq /usr/local/bin/mq

ENTRYPOINT [ "mq" ]

