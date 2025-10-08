FROM rust:1.90-bookworm as Builder

ENV RUST_LOG=info,docker_proxy_filter=debug

WORKDIR /usr/src/app
COPY . .


RUN cargo install --target x86_64-unknown-linux-gnu --path .

FROM debian:bookworm-slim
RUN apt-get update && apt-get upgrade -y && apt-get install openssl -y && apt clean && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/app/target/x86_64-unknown-linux-gnu/release/docker-proxy-filter /usr/local/bin/docker-proxy-filter
CMD ["docker-proxy-filter"]