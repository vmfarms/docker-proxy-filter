use tracing::*;
use ntex::{http, web::{self}, time::Seconds};
use std::sync::{Arc, Mutex};

use std::collections::HashMap;

mod config;
mod docker;
mod proxy;
mod tunnel;
mod utils;

use proxy::AppStateWithContainerMap;

#[ntex::main]
async fn main() -> std::io::Result<()> {

    match config::loadenv() {
        Ok(_) => (),
        Err(err) => {
            println!("there was a problem reading .env: {err}");
        }
    }

    tracing_subscriber::fmt::fmt()
        // uses RUST_LOG env for filtering log levels and namespaces
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let (config, port) = match config::get_config() {
       Ok((config, port)) => {
       (config, port)
    },
       Err(_error) => {
        panic!("Unable to start due to invalid envs")
     }
    };

    if config.container_labels.is_empty() && config.container_names.is_empty() && config.exclude_labels.is_empty() && config.exclude_names.is_empty() {
        warn!("You have not defined any filters! All containers will be exposed. If you are using docker-proxy-filter only for SCRUB_ENVS then this is expected behavior, otherwise check your environmental variables.");
    }
    if !config.exclude_labels.is_empty() || !config.exclude_names.is_empty() {
        info!("Exclude filters active -- containers matching EXCLUDE_LABELS/EXCLUDE_NAMES will be hidden");
    }

    let cm = AppStateWithContainerMap {
        container_map: Arc::new(Mutex::new(HashMap::<String, Option<bool>>::new()))
    };

    let forward_url = url::Url::parse(&config.proxy_url)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;

    // Parse upstream address from PROXY_URL for the TCP tunnel.
    let upstream_host = forward_url.host_str().unwrap_or("socket-proxy").to_string();
    let upstream_port = forward_url.port().unwrap_or(2375);
    let upstream_addr = format!("{}:{}", upstream_host, upstream_port);

    // The ntex HTTP server listens on an internal port (not exposed).
    // The TCP proxy on the external port routes upgrade requests directly
    // to socket-proxy and everything else to the internal HTTP handler.
    let internal_port = port + 1; // e.g., 2376 internal, 2375 external
    let http_addr = format!("127.0.0.1:{}", internal_port);

    // Spawn the TCP proxy (external-facing, handles upgrade detection)
    let listen_port = port;
    let tunnel_upstream = upstream_addr.clone();
    let tunnel_http = http_addr.clone();
    tokio::spawn(async move {
        let listener = match tokio::net::TcpListener::bind(format!("0.0.0.0:{}", listen_port)).await {
            Ok(l) => l,
            Err(e) => {
                error!("Failed to bind TCP proxy on port {}: {}", listen_port, e);
                return;
            }
        };
        info!("TCP proxy listening on 0.0.0.0:{}", listen_port);

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    debug!("TCP proxy accepted connection from {}", addr);
                    let upstream = tunnel_upstream.clone();
                    let http = tunnel_http.clone();
                    tokio::spawn(async move {
                        tunnel::handle_connection(stream, upstream, http).await;
                    });
                }
                Err(e) => {
                    warn!("TCP proxy accept error: {}", e);
                }
            }
        }
    });

    // Start the ntex HTTP server on the internal port
    info!("HTTP handler on 127.0.0.1:{}", internal_port);
    web::server(move || {
        web::App::new()
            .state(
                http::Client::build()
                    .connector(
                        http::client::Connector::default()
                            // Allow many concurrent connections to the upstream
                            // socket-proxy. Without this, streaming endpoints
                            // (events, stats, attach) exhaust the default pool
                            // and block all subsequent requests.
                            .limit(128)
                            .finish()
                    )
                    // Docker operations (stop, build, pull) can take minutes.
                    // Default 5s timeout causes "Timeout while waiting for response".
                    .timeout(Seconds(300))
                    .finish()
            )
            .state(forward_url.clone())
            .state(config.container_names.clone())
            .state(cm.clone())
            .state(config.scrub_envs.clone())
            .state(config.clone())

            .wrap(web::middleware::Logger::default())
            .default_service(web::route().to(proxy::forward))
    })
    .bind(("127.0.0.1", internal_port))?
    .run()
    .await
}
