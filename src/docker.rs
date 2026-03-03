use std::sync::LazyLock;
use regex::Regex;
use std::collections::HashMap;

use crate::config::{ContainerLabels, ContainerNames};
use crate::utils;

use ntex::{http::{self}, web::{self}};

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
    if !filter_names.is_empty() {
        if utils::strings_in_strings(&names, &filter_names) {
            return true;
        }
    }
    if !filter_labels.is_empty() {
        if utils::label_match(&labels, &filter_labels) {
            return true;
        }
    }
    false
}

/// Returns true if the container matches any exclusion filter.
pub fn matches_exclude(exclude_names: &ContainerNames, exclude_labels: &ContainerLabels, names: &Vec<String>, labels: &HashMap<String, String>) -> bool {
    if exclude_names.is_empty() && exclude_labels.is_empty() {
        return false
    }
    if !exclude_names.is_empty() {
        if utils::strings_in_strings(names, exclude_names) {
            return true;
        }
    }
    if !exclude_labels.is_empty() {
        if utils::label_match(labels, exclude_labels) {
            return true;
        }
    }
    false
}

/// Combined include + exclude check. A container is valid if:
/// 1. It matches at least one include filter (or no include filters are set)
/// 2. It does NOT match any exclude filter
pub fn container_summary_match(container: &types::ContainerSummary, config: &crate::config::AppConfig) -> bool {
    let names = container.names.as_ref().cloned().unwrap_or_default();
    let labels = container.labels.as_ref().cloned().unwrap_or_default();

    let included = match_labels_or_names(&config.container_names, &config.container_labels, &names, &labels);
    if !included {
        return false;
    }
    let excluded = matches_exclude(&config.exclude_names, &config.exclude_labels, &names, &labels);
    !excluded
}

/// Combined include + exclude check for individual container lookups.
pub fn container_info_match(config: &crate::config::AppConfig, name: &str, labels: &HashMap<String, String>) -> bool {
    let names = vec![name.to_string()];
    let included = match_labels_or_names(&config.container_names, &config.container_labels, &names, labels);
    if !included {
        return false;
    }
    let excluded = matches_exclude(&config.exclude_names, &config.exclude_labels, &names, labels);
    !excluded
}

pub async fn get_container_info(client: &web::types::State<http::Client>, u: &web::types::State<url::Url>, container_id: &String) -> Result<(String, HashMap<String,String>), Box<dyn std::error::Error>> {

    let mut uri = u.get_ref().clone();
    uri.set_path(format!("containers/{id}/json", id = container_id).as_str());

    let mut res = client.get(uri.to_string())
    .send()
    .await;
       // .map_err(web::Error::from)?;

    match res {
        Ok(mut okres) => {
            let info_res = okres.json::<types::ContainerInspect>()
            .limit(2097152).await;
            match info_res {
                Ok(json_res) => {
                    if json_res.message.is_some() {
                        return Err(json_res.message.unwrap().into())
                    }
                    Ok((json_res.name.expect("Container has a name"), json_res.config.expect("Container has config").labels.expect("Container has labels")))
                },
                Err(e) => {
                    return Err(Box::new(e));
                }
            }
        },
        Err(e) => {
            return Err(Box::new(e));
        }
    }
}
