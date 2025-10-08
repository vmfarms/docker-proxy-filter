use std::sync::LazyLock;
use regex::Regex;

pub mod types;

pub struct GetMatch {
    pub id: String,
    pub resource: String
}

pub fn match_container_get(haystack: &str) -> Option<GetMatch> {
    static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"containers/(?<id>.+)/(?<resource>.+)").unwrap());
    let captures = RE.captures(haystack);

    let get_match = match captures {
       Some(cap) => {
            Some(GetMatch { 
                id: String::from(cap.name("id").unwrap().as_str()), 
                resource: String::from(cap.name("resource").unwrap().as_str())
            })
       }
       None => {
            None
       }
    };
    get_match
}

pub fn is_container_named(container: &types::ContainerSummary, container_names: &Vec<String>) -> bool {
    return Option::is_some(&container.names.clone().unwrap().iter().find(|&y|  {
        return container_names.iter().any(|z| y.contains(z));
    }));
}

pub async fn get_container_name(u: &String, container_id: &String) -> Result<String, Box<dyn std::error::Error>> {

    let url = format!("{}containers/{id}/json", u, id = container_id);
    //debug!(log, "Get Container URL: {}", url);
    let resp = reqwest::get(&url)
        .await?
        .json::<types::ContainerInspect>()
        .await;
        match resp {
            Ok(json_res) => {
                if json_res.message.is_some() {
                    return Err(json_res.message.unwrap().into())
                }
                match json_res.name {
                    Some(v) => {
                        Ok(v)
                    }
                    None => {
                        return Err("Container has no name".into())
                    }
                }
            }
            Err(e) => {
                return Err(Box::new(e));
            }
        }
}
