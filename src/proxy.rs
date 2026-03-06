use tracing::*;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use ntex::{http::{self, HttpMessage}, web::{self}};
use ::http::StatusCode;

use crate::config::{AppConfig};
use crate::docker::{self, types::*};
use crate::utils;

#[derive(Clone)]
pub struct AppStateWithContainerMap {
    pub container_map: Arc<Mutex<HashMap<String, Option<bool>>>>, // <- Mutex is necessary to mutate safely across threads
}

macro_rules! is {
    ($cond:expr; $if:expr; $else:expr) => {
        if $cond { $if } else { $else }
    };
}

pub async fn forward(
    req: web::HttpRequest,
    body: ntex::util::Bytes,
    client: web::types::State<http::Client>,
    forward_url: web::types::State<url::Url>,
    app_config: web::types::State<AppConfig>,
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
    // Prevent the Docker client (Go net/http) from reusing this connection.
    // Without this, chunked transfer encoding terminators arrive on pooled
    // connections and produce "Unsolicited response on idle HTTP channel" warnings.
    client_resp.header("Connection", "close");

    if res.status() == 200 {

        // if route is to list containers we want to return a filtered list
        if new_url.path().contains("containers/json") {
            let _list_span = span!(Level::DEBUG, "Container List").entered();

            let container_res = &res.json::<Vec<ContainerSummary>>()
            // 2mb in bytes
            .limit(2097152).await;

            let containers = match container_res {
                Ok(list_res) => {
                    list_res
                }
                Err(e) => {
                    panic!("{e}");
                }
            };
            
            // filter containers by include/exclude rules
            let filtered_containers = containers.into_iter()
                .filter(|&con| {
                    let _list_span = span!(Level::DEBUG, "Container", id = utils::short_id(con.id.as_ref().unwrap())).entered();
                    docker::container_summary_match(con, app_config.get_ref())
                })
                .collect::<Vec<&ContainerSummary>>();

            debug!("{} of {} containers valid", filtered_containers.len(), containers.len());
            Ok(client_resp.json(&filtered_containers))
        } else {

            // only deal with routes that are for containers like /containers/1234/{some_resource}
            match docker::match_container_get(new_url.path()) {
                // the regex pulls the container id from the route with a named capture group, avaiable as m.id
            
                Some (m) => {
                    let short_cid = utils::short_id(&m.id); //format!("{start}...{end}", start = &m.id[..6], end = &m.id[&m.id.len() - 6..]);
                    let _con_span = span!(Level::DEBUG, "Container", id = short_cid).entered();
                    debug!("Matched container route with Id {}", &m.id);

                    // Check the map without holding the lock across await points.
                    // Holding std::sync::Mutex across .await deadlocks the single-threaded runtime.
                    let needs_inspect = {
                        let cm = data.container_map.lock().unwrap();
                        !cm.contains_key(&m.id) || cm.get(&m.id).unwrap().is_none()
                    };

                    if needs_inspect {
                        debug!("Requested Id not in map, trying to inspect...");
                        let info_res = docker::get_container_info(&client, &forward_url, &m.id).await;
                        let mut cm = data.container_map.lock().unwrap();
                        match info_res {
                            Ok((name, labels)) => {
                                let is_container_match = docker::container_info_match(app_config.get_ref(), &name, &labels);
                                debug!("Recording container '{}' {} valid", name, is!(is_container_match; "as";"as not"));
                                cm.insert(m.id.clone(), Some(is_container_match));
                            }
                            Err(e) => {
                                warn!("Could not inspect: {e}");
                                if e.to_string().contains("No such container") {
                                    cm.insert(m.id.clone(), Some(false));
                                } else {
                                    cm.insert(m.id.clone(), None);
                                }
                            }
                        }
                    }

                    let allowed = data.container_map.lock().unwrap();
                    let allowed = allowed.get(&m.id).unwrap();
                    
                    match allowed {
                        Some(n) => {
                            // only return if a response if requested container has a name  that includes values from CONTAINER_NAMES
                            if *n {
                                debug!("Matched container filters");
                                // if the route resource is specifically the Container Inspect API
                                // then we may need to scrub Envs if SCRUB_ENVS=true
                                if m.resource == "json" {

                                    client_resp.content_type("application/json");

                                    if app_config.get_ref().scrub_envs {
                                        let mut container = res.json::<ContainerInspect>().await.unwrap();
                                        container.config.as_mut().unwrap().env = Some(Vec::new());
                                        Ok(client_resp.json(&container))
                                    } else {
                                        let body = res.body().limit(2097152).await.map_err(web::Error::from)?;
                                        Ok(client_resp.body(body))
                                    }

                                } else {
                                    client_resp.content_type(res.content_type());
                                    let body = res.body().limit(2097152).await.map_err(web::Error::from)?;
                                    Ok(client_resp.body(body))
                                }
                            } else {
                                debug!("Does not match container filters, 404ing...");
                                client_resp.status(StatusCode::NOT_FOUND);
                                Ok(client_resp.json(&DockerErrorMessage { message: format!("No such container: {}", &m.id)}))
                            }
                        }
                        None => {
                            debug!("Does not exist or Docker API previously returned an error, 404ing...");
                            client_resp.status(StatusCode::NOT_FOUND);
                            Ok(client_resp.json(&DockerErrorMessage { message: format!("No such container: {}", &m.id)}))
                        }
                    }
                }
                // if we don't match container route then proxy response through, unmodified
                None => {
                    let body = res.body().limit(2097152).await.map_err(web::Error::from)?;
                    Ok(client_resp.body(body))
                }
            }
        }

    } else {
        let body = res.body().limit(2097152).await.map_err(web::Error::from)?;
        Ok(client_resp.body(body))
    }


}