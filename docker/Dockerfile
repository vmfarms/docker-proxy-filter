FROM rust:1.90-bookworm as Builder

ENV RUST_LOG=info,docker_proxy_filter=debug

WORKDIR /app

RUN cargo init

COPY Cargo.lock Cargo.toml ./

# build dummy project with dependencies to get them compiled and cached
RUN cargo build --release --target $(rustc --print host-tuple)

# copy full app code
COPY src/ src/
# updated modified timestamp so build runs again
RUN touch src/main.rs

# build full app to host arch and copy to generic location
RUN export ARCH=$(rustc --print host-tuple); \
    cargo build --release --target $ARCH \
    && mkdir build \
    && cp target/$ARCH/release/docker-proxy-filter build/docker-proxy-filter

FROM debian:bookworm-slim
# need ssl for web server
RUN apt-get update && apt-get upgrade -y && apt-get install openssl -y && apt clean && rm -rf /var/lib/apt/lists/*
# copy built app from generic location
COPY --from=builder /app/build/docker-proxy-filter /usr/local/bin/docker-proxy-filter
CMD ["docker-proxy-filter"]