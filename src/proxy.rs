use tracing::*;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use ntex::{http::{self, HttpMessage}, web::{self}};
use futures_util::TryStreamExt;
use ::http::StatusCode;

use crate::docker::{self, types::*};

#[derive(Clone)]
pub struct AppStateWithContainerMap {
    pub container_map: Arc<Mutex<HashMap<String, Option<String>>>>, // <- Mutex is necessary to mutate safely across threads
}

pub async fn forward(
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
                .filter(|&con| docker::is_container_named(con, &container_names.get_ref()))
                .collect::<Vec<&ContainerSummary>>();

            let fresp = web::HttpResponse::build(res.status()).json(&filtered_containers);
            debug!("{} of {} containers valid", filtered_containers.len(), containers.len());
            Ok(fresp)
        } else {

            match docker::match_container_get(new_url.path()) {
                Some (m) => {
                    let mut cm = data.container_map.lock().unwrap();
                    if !cm.contains_key(&m.id) {
                        debug!("Requested container Id not in map, trying to inspect: {}", &m.id);
                        let name_res = docker::get_container_name(&forward_url.to_string(), &m.id).await;
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