FROM rust:1.85-slim AS builder

WORKDIR /usr/src/app
COPY . .

RUN cargo build --release

FROM gcr.io/distroless/static-debian12:nonroot

COPY --from=builder --chown=nonroot:nonroot /usr/src/app/target/release/mq /usr/local/bin/mq

CMD ["mq"]
