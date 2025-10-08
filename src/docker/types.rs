use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Serialize, Deserialize)]
pub struct DockerErrorMessage {
    pub message: String
}

#[derive(Serialize, Deserialize)]
pub struct ContainerSummary {
    #[serde(rename = "Id")]
    pub id: Option<String>,

    #[serde(rename = "Names")]
    pub names: Option<Vec<String>>,

    pub message: Option<String>,

    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

#[derive(Serialize, Deserialize)]
pub struct ContainerInspect {
    #[serde(rename = "Id")]
    pub id: Option<String>,

    #[serde(rename = "Name")]
    pub name: Option<String>,

    #[serde(rename = "Config")]
    pub config: Option<ContainerConfig>,

    pub message: Option<String>,

    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

#[derive(Serialize, Deserialize)]
pub struct ContainerConfig {
    #[serde(rename = "Env")]
    pub env: Option<Vec<String>>,

    #[serde(flatten)]
    extra: HashMap<String, Value>,
}