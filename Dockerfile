# syntax=docker/dockerfile:1
FROM rust:1.95.0-slim@sha256:275c320a57d0d8b6ab09454ab6d1660d70c745fb3cc85adbefad881b69a212cc AS builder

WORKDIR /usr/src/app
COPY . /usr/src/app

RUN cargo build -p mq-run --bin mq --release

FROM gcr.io/distroless/cc:nonroot

COPY --from=builder --chown=nonroot:nonroot /usr/src/app/target/release/mq /usr/local/bin/mq

ENTRYPOINT [ "mq" ]
