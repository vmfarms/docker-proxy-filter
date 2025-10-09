use tracing::*;
use ntex::{http, web::{self}};
use std::sync::{Arc, Mutex};

use std::collections::HashMap;

mod config;
mod docker;
mod proxy;
mod utils;

use proxy::{AppStateWithContainerMap};

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

    let cm = AppStateWithContainerMap {
        container_map: Arc::new(Mutex::new(HashMap::<String, Option<bool>>::new()))
    };

    let forward_url = url::Url::parse(&config.proxy_url)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
    web::server(move || {
        web::App::new()
            .state(http::Client::new())
            .state(forward_url.clone())
            .state(config.container_names.clone())
            .state(cm.clone())
            .state(config.scrub_envs.clone())
            .state(config.clone())

            .wrap(web::middleware::Logger::default())
            .default_service(web::route().to(proxy::forward))
    })
    .bind(("0.0.0.0", port))?
    .run()
    .await
}
