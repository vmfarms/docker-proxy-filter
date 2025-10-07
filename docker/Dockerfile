FROM rust:1.90-bookworm as Builder

WORKDIR /usr/src/myapp
COPY . .


RUN cargo install --target x86_64-unknown-linux-gnu --path .

FROM debian:bookworm-slim
RUN apt-get update && apt-get upgrade -y && apt-get install openssl -y && apt clean && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/myapp/target/x86_64-unknown-linux-gnu/release/docker-container-proxy /usr/local/bin/myapp
CMD ["myapp"]