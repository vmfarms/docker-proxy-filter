use std::sync::LazyLock;
use regex::Regex;
use std::collections::HashMap;

use crate::config::{ContainerLabels, ContainerNames};
use crate::utils;

pub mod types;

pub struct GetMatch {
    pub id: String,
    pub resource: String
}

pub fn match_container_get(haystack: &str) -> Option<GetMatch> {
    static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"containers/(?<id>.+)/(?<resource>.+)").unwrap());
    let captures = RE.captures(haystack);

    match captures {
       Some(cap) => {
            Some(GetMatch { 
                id: String::from(cap.name("id").unwrap().as_str()), 
                resource: String::from(cap.name("resource").unwrap().as_str())
            })
       }
       None => {
            None
       }
    }
}

pub fn match_labels_or_names(filter_names: &ContainerNames, filter_labels: &ContainerLabels, names: &Vec<String>, labels: &HashMap<String, String>) -> bool {
    if filter_names.is_empty() && filter_labels.is_empty() {
        return true
    }
    let mut any = false;
    if !filter_names.is_empty() {
        if utils::strings_in_strings(&names, &filter_names) {
            any = true;
        }
    }
    if !filter_labels.is_empty() {
        if utils::label_match(&labels, &filter_labels) {
            any = true;
        }
    }
    any
}

pub fn container_summary_match(container: &types::ContainerSummary, container_names: &ContainerNames, container_labels: &ContainerLabels) -> bool {

    return match_labels_or_names(container_names, container_labels, &container.names.as_ref().unwrap_or(Vec::new().as_ref()), &container.labels.as_ref().unwrap_or(&HashMap::<String,String>::new()))
}

pub async fn get_container_info(u: &String, container_id: &String) -> Result<(String, HashMap<String,String>), Box<dyn std::error::Error>> {

    let url = format!("{}containers/{id}/json", u, id = container_id);
    let resp = reqwest::get(&url)
        .await?
        .json::<types::ContainerInspect>()
        .await;
        match resp {
            Ok(json_res) => {
                if json_res.message.is_some() {
                    return Err(json_res.message.unwrap().into())
                }
                Ok((json_res.name.expect("Container has a name"), json_res.config.expect("Container has config").labels.expect("Container has labels")))
            }
            Err(e) => {
                return Err(Box::new(e));
            }
        }
}
