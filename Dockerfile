# syntax=docker/dockerfile:1
FROM rust:1.96.0-slim@sha256:c37af730be4fd8104cbf9aedbd6ab259e51ca2d5437817a0f8680edf66ac6c28 AS builder

WORKDIR /usr/src/app
COPY . /usr/src/app

RUN cargo build -p mq-run --bin mq --release

FROM gcr.io/distroless/cc:nonroot

COPY --from=builder --chown=nonroot:nonroot /usr/src/app/target/release/mq /usr/local/bin/mq

ENTRYPOINT [ "mq" ]
