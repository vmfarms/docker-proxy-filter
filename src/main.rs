use tracing::{debug, error, info, warn};
use tracing_subscriber;

use futures_util::TryStreamExt;
use ::http::StatusCode;
use ntex::{http::{self, HttpMessage}, web::{self}};
use std::sync::{Arc, Mutex};

use dotenvy;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use std::sync::LazyLock;
use regex::Regex;

#[derive(Clone)]
struct AppStateWithContainerMap {
    container_map: Arc<Mutex<HashMap<String, Option<String>>>>, // <- Mutex is necessary to mutate safely across threads
}

struct GetMatch {
    id: String,
    resource: String
}

#[derive(Serialize, Deserialize)]
struct DockerErrorMessage {
    pub message: String
}

#[derive(Serialize, Deserialize)]
struct ContainerSummary {
    #[serde(rename = "Id")]
    pub id: Option<String>,

    #[serde(rename = "Names")]
    pub names: Option<Vec<String>>,

    pub message: Option<String>,

    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

#[derive(Serialize, Deserialize)]
struct ContainerInspect {
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
struct ContainerConfig {
    #[serde(rename = "Env")]
    env: Option<Vec<String>>,

    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

#[derive(Deserialize, Debug)]
struct Config {
  proxy_url: String,
  container_names: Vec<String>,
  #[serde(default="default_scrub")]
  scrub_envs: bool,
  #[serde(default="default_port")]
  port: u16
}

fn default_scrub() -> bool {
    false
}
fn default_port() -> u16 {
    2375
}

async fn forward(
    req: web::HttpRequest,
    body: ntex::util::Bytes,
    client: web::types::State<http::Client>,
    forward_url: web::types::State<url::Url>,
    container_names: web::types::State<Vec<String>>,
    scrub_env: web::types::State<bool>,
    data: web::types::State<AppStateWithContainerMap>
) -> Result<web::HttpResponse, web::Error> {
    let mut new_url = forward_url.get_ref().clone();
    new_url.set_path(req.uri().path());
    new_url.set_query(req.uri().query());
    let forwarded_req = client.request_from(new_url.as_str(), req.head());
    let mut res = forwarded_req
        .send_body(body)
        .await
        .map_err(web::Error::from)?;

    let mut client_resp = web::HttpResponse::build(res.status());
    client_resp.content_type(res.content_type());

    if res.status() == 200 {

        if new_url.path().contains("containers/json") {

            let containers = &res.json::<Vec<ContainerSummary>>().await.unwrap();
            
            let filtered_containers = containers.into_iter()
                .filter(|&con| is_container_named(con, &container_names.get_ref()))
                .collect::<Vec<&ContainerSummary>>();

            let fresp = web::HttpResponse::build(res.status()).json(&filtered_containers);
            debug!("{} of {} containers valid", filtered_containers.len(), containers.len());
            Ok(fresp)
        } else {

            match match_container_get(new_url.path()) {
                Some (m) => {
                    let mut cm = data.container_map.lock().unwrap();
                    if !cm.contains_key(&m.id) {
                        debug!("Requested container Id not in map, trying to inspect: {}", &m.id);
                        let name_res = get_container_name(&forward_url.to_string(), &m.id).await;
                        match name_res {
                            Ok(name) => {
                                debug!("Container Id {} has name '{}'", &m.id, &name);
                                cm.insert(m.id.clone(), Some(name));
                            }
                            Err(e) => {
                                warn!("Could not inspect container {}: {e}", &m.id);
                                cm.insert(m.id.clone(), None);
                            }
                        }
                    }

                    let name_val = cm.get(&m.id).unwrap();
                    
                    match name_val {
                        Some(n) => {
                            if container_names.get_ref().iter().any(|x| n.contains(x)) {
                            //if n.contains(container_name.get_ref()) { //n.iter().any(|x| x.contains(container_name.get_ref())) {

                                if m.resource == "json" {

                                    client_resp.content_type("application/json");

                                    if *scrub_env.get_ref() {
                                        let mut container = res.json::<ContainerInspect>().await.unwrap();
                                        container.config.as_mut().unwrap().env = Some(Vec::new());
                                        Ok(client_resp.json(&container))
                                    } else {
                                        Ok(client_resp.streaming(res.into_stream()))
                                    }

                                } else {
                                    client_resp.content_type(res.content_type());
                                    Ok(client_resp.streaming(res.into_stream()))
                                }
                            } else {
                                debug!("Container {} does not include container filter name, 404ing...", &m.id);
                                client_resp.status(StatusCode::NOT_FOUND);
                                Ok(client_resp.json(&DockerErrorMessage { message: format!("No such container: {}", &m.id)}))
                            }
                        }
                        None => {
                            debug!("Container {} does not exist or Docker API previously returned an error, 404ing...", &m.id);
                            client_resp.status(StatusCode::NOT_FOUND);
                            Ok(client_resp.json(&DockerErrorMessage { message: format!("No such container: {}", &m.id)}))
                        }
                    }
                }
                None => {
                    let stream = res.into_stream();
                    Ok(client_resp.streaming(stream))
                }
            }
        }

    } else {
        let stream = res.into_stream();
        Ok(client_resp.streaming(stream))
    }


}

fn match_container_get(haystack: &str) -> Option<GetMatch> {
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

// fn is_container_get(haystack: &str) -> bool {
//    match_container_get(&haystack).is_some()
// }

fn is_container_named(container: &ContainerSummary, container_names: &Vec<String>) -> bool {
    return Option::is_some(&container.names.clone().unwrap().iter().find(|&y|  {
        return container_names.iter().any(|z| y.contains(z));
    }));
}

async fn get_container_name(u: &String, container_id: &String) -> Result<String, Box<dyn std::error::Error>> {

    let url = format!("{}containers/{id}/json", u, id = container_id);
    //debug!(log, "Get Container URL: {}", url);
    let resp = reqwest::get(&url)
        .await?
        .json::<ContainerInspect>()
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

#[ntex::main]
async fn main() -> std::io::Result<()> {
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = match envy::from_env::<Config>() {
       Ok(config) => { 
        info!("{:#?}", config);
        config
    },
       Err(error) => {
        error!("Could not parse envs correctly! {:#?}", error); 
        panic!("Unable to start due to invalid envs")
     }
    };

    let cm = AppStateWithContainerMap {
        container_map: Arc::new(Mutex::new(HashMap::<String, Option<String>>::new()))
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
            .wrap(web::middleware::Logger::default())
            .default_service(web::route().to(forward))
    })
    .bind(("0.0.0.0", config.port))?
    .run()
    .await
}
