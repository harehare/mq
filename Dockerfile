FROM rust:1.85-slim AS builder

WORKDIR /usr/src/app
COPY . /usr/src/app

RUN cargo build -p mq-cli --release

FROM gcr.io/distroless/cc:nonroot

COPY --from=builder --chown=nonroot:nonroot /usr/src/app/target/release/mq /usr/local/bin/mq

CMD ["mq"]
