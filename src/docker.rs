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

pub fn container_summary_match(container: &types::ContainerSummary, container_names: &ContainerNames, container_labels: &ContainerLabels) -> bool {

    return match_labels_or_names(container_names, container_labels, &container.names.as_ref().unwrap_or(Vec::new().as_ref()), &container.labels.as_ref().unwrap_or(&HashMap::<String,String>::new()))
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
