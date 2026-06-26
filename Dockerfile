# syntax=docker/dockerfile:1
FROM rust:1.96.0-slim@sha256:6abf73f05806f36362d0ff2722f2250c6153398831edd0455e0e0baa1f78ecc7 AS builder

WORKDIR /usr/src/app
COPY . /usr/src/app

RUN cargo build -p mq-run --bin mq --release

FROM gcr.io/distroless/cc:nonroot

COPY --from=builder --chown=nonroot:nonroot /usr/src/app/target/release/mq /usr/local/bin/mq

ENTRYPOINT [ "mq" ]
