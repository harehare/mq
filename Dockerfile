# syntax=docker/dockerfile:1
FROM rust:1.98.1-slim@sha256:4cd829461bd5c4d511c32e269da9cb8929223b666519d8004e35fc8d1d771ab7 AS builder

WORKDIR /usr/src/app
COPY . /usr/src/app

RUN cargo build -p mq-run --bin mq --release

FROM gcr.io/distroless/cc:nonroot

COPY --from=builder --chown=nonroot:nonroot /usr/src/app/target/release/mq /usr/local/bin/mq

ENTRYPOINT [ "mq" ]
