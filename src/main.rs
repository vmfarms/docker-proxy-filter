#[macro_use]
extern crate slog;
extern crate slog_async;
extern crate slog_term;

use futures_util::TryStreamExt;
use ::http::StatusCode;
use ntex::{http, web::{self, Responder, HttpResponse}};
use std::sync::{Arc, Mutex};

use slog::{Drain, Logger};
use dotenv::dotenv;
use std::env;
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

// #[derive(Serialize, Deserialize)]
// struct DockerErrorMessage {
//     pub message: Option<String>
// }

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

async fn forward(
    req: web::HttpRequest,
    body: ntex::util::Bytes,
    client: web::types::State<http::Client>,
    forward_url: web::types::State<url::Url>,
    container_name: web::types::State<String>,
    log: web::types::State<Logger>,
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

    info!(log, "test");

    if res.status() == 200 {

        if new_url.path().contains("containers/json") {

            let containers = &res.json::<Vec<ContainerSummary>>().await.unwrap();
            
            let filtered_containers = containers.into_iter()
                .filter(|&con| is_container_named(con, &container_name.get_ref()))
                .collect::<Vec<&ContainerSummary>>();

            let fresp = web::HttpResponse::build(res.status()).json(&filtered_containers);

            Ok(fresp)
        } else {

            match match_container_get(new_url.path()) {
                Some (m) => {
                    let mut cm = data.container_map.lock().unwrap();
                    if !cm.contains_key(&m.id) {
                        let id_res = get_container_name(&forward_url.to_string(), &m.id, &log).await;
                        match id_res {
                            Ok(id) => {
                                cm.insert(m.id.clone(), Some(id));
                            }
                            Err(e) => {
                                warn!(log, "Could not get container info"; "error" => %e);
                                cm.insert(m.id.clone(), None);
                            }
                        }
                    }

                    let name_val = cm.get(&m.id).unwrap();
                    
                    match name_val {
                        Some(n) => {
                            if n.contains(container_name.get_ref()) { //n.iter().any(|x| x.contains(container_name.get_ref())) {

                                client_resp.content_type("application/json");

                                if *scrub_env.get_ref() {
                                    let mut container = res.json::<ContainerInspect>().await.unwrap();
                                    container.config.as_mut().unwrap().env = Some(Vec::new());
                                    Ok(client_resp.json(&container))
                                } else {
                                    Ok(client_resp.streaming(res.into_stream()))
                                }
                            } else {
                                client_resp.status(StatusCode::NOT_FOUND);
                                Ok(client_resp.finish())
                            }
                        }
                        None => {
                            client_resp.status(StatusCode::NOT_FOUND);
                            Ok(client_resp.finish())
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

// fn is_container_json(haystack: &str) -> bool {
//     match match_container_get(&haystack) {
//         Some(m) => m.resource == "json",
//         None => false
//     }
// }


fn is_container_named(container: &ContainerSummary, container_name: &String) -> bool {
    return Option::is_some(&container.names.clone().unwrap().iter().find(|&y|  { 
        return y.contains(container_name);
    }));
}

async fn get_container_name(u: &String, container_id: &String, log: &Logger) -> Result<String, Box<dyn std::error::Error>> {

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
    let decorator = slog_term::TermDecorator::new().build();
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    let drain = slog_async::Async::new(drain).build().fuse();
    let log = slog::Logger::root(drain, o!());

    dotenv().ok();


    let proxy_url_env = env::var("PROXY_URL");
    let mut proxy_url: String = String::new();

    match proxy_url_env {
        Ok(val) => {
            info!(log, "PROXY_URL: {}", val);
            proxy_url.push_str(&val);
        },
        Err(e) => {
            crit!(log, "Missing PROXY_URL"; "error" => %e);
            panic!();
        },
    }

    let container_name_env = env::var("CONTAINER_NAME");
    let mut container_name: String = String::new();

    match container_name_env {
        Ok(val) => {
            info!(log, "CONTAINER_NAME: {}", val);
            container_name.push_str(&val);
        },
        Err(e) => {
            crit!(log, "Missing CONTAINER_NAME"; "error" => %e);
            panic!();
        },
    }

    let scub_env =  match env::var("SCRUB_ENVS") {
        Ok(val) => {
            let truthy = val == "true";
            info!(log, "SCRUB_ENVS: {}", truthy);
            truthy
        },
        Err(e) => {
            info!(log, "SCRUB_ENVS: false");
            false
        },
    };

    let cm = AppStateWithContainerMap {
        container_map: Arc::new(Mutex::new(HashMap::<String, Option<String>>::new()))
    };

    let forward_url = url::Url::parse(&proxy_url)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
    web::server(move || {
        web::App::new()
            .state(http::Client::new())
            .state(forward_url.clone())
            .state(container_name.clone())
            .state(log.clone())
            .state(cm.clone())
            .state(scub_env.clone())
            .wrap(web::middleware::Logger::default())
            .default_service(web::route().to(forward))
    })
    .bind(("0.0.0.0", 2376))?
    .run()
    .await
}
