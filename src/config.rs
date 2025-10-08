use dotenvy;
use serde::Deserialize;
use std::path::PathBuf;
use tracing::*;

fn default_scrub() -> bool {
    false
}
fn default_port() -> u16 {
    2375
}

#[derive(Deserialize, Debug)]
pub struct Config {
    pub proxy_url: String,
    pub container_names: Vec<String>,
    #[serde(default = "default_scrub")]
    pub scrub_envs: bool,
    #[serde(default = "default_port")]
    pub port: u16,
}

pub fn loadenv() -> Result<PathBuf, dotenvy::Error> {
    dotenvy::dotenv()
}

pub fn get_config() -> Result<Config, envy::Error> {
    match envy::from_env::<Config>() {
        Ok(config) => {
            info!("{:#?}", config);
            Ok(config)
        }
        Err(error) => {
            error!("Could not parse envs correctly! {:#?}", error);
            Err(error)
        }
    }
}
