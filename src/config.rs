use dotenvy;
use serde::Deserialize;
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

pub fn loadenv() -> Result<(), dotenvy::Error> {
    match dotenvy::dotenv() {
        Ok(_) => Ok(()),
        Err(err) => {
             // we don't care if there isn't a .env file
            if err.not_found() {
                Ok(())
            } else {
                Err(err)
            }
        }
    }
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
