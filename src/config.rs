use serde::Deserialize;
use tracing::*;
use std::collections::HashMap;

fn default_scrub() -> bool {
    false
}
fn default_port() -> u16 {
    2375
}
fn default_list() -> Vec<String> {
    Vec::new()
}

#[derive(Deserialize, Debug, Clone)]
pub struct EnvConfig {
    pub proxy_url: String,
    #[serde(default = "default_list")]
    pub container_names: Vec<String>,
    #[serde(default = "default_list")]
    pub container_labels: Vec<String>,
    #[serde(default = "default_list")]
    pub exclude_names: Vec<String>,
    #[serde(default = "default_list")]
    pub exclude_labels: Vec<String>,
    #[serde(default = "default_scrub")]
    pub scrub_envs: bool,
    #[serde(default = "default_port")]
    pub port: u16,
}

pub type ContainerLabels = HashMap<String, Option<String>>;
pub type ContainerNames = Vec<String>;

#[derive(Clone)]
pub struct AppConfig {
    pub proxy_url: String,
    pub container_names: ContainerNames,
    pub container_labels: ContainerLabels,
    pub exclude_names: ContainerNames,
    pub exclude_labels: ContainerLabels,
    pub scrub_envs: bool,
}

fn parse_label_list(labels: &[String]) -> ContainerLabels {
    let mut label_map: ContainerLabels = HashMap::<String, Option<String>>::new();
    for label_kv in labels.iter() {
        if let Some((k, v)) = label_kv.split_once('=') {
            label_map.insert(k.to_string(), Some(v.to_string()));
        } else {
            label_map.insert(label_kv.clone(), None);
        }
    }
    label_map
}

impl From<&EnvConfig> for AppConfig {
    fn from(item: &EnvConfig) -> Self {
        AppConfig {
            proxy_url: item.proxy_url.clone(),
            container_names: item.container_names.clone(),
            container_labels: parse_label_list(&item.container_labels),
            exclude_names: item.exclude_names.clone(),
            exclude_labels: parse_label_list(&item.exclude_labels),
            scrub_envs: item.scrub_envs,
        }
    }
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

pub fn get_config() -> Result<(AppConfig, u16), envy::Error> {
    match envy::from_env::<EnvConfig>() {
        Ok(config) => {
            info!("{:#?}", config);
            Ok((AppConfig::from(&config), config.port))
        }
        Err(error) => {
            error!("Could not parse envs correctly! {:#?}", error);
            Err(error)
        }
    }
}
