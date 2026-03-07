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
    pub container_map: Arc<Mutex<HashMap<String, Option<bool>>>>,
}

macro_rules! is {
    ($cond:expr; $if:expr; $else:expr) => {
        if $cond { $if } else { $else }
    };
}

/// Returns true for Docker API endpoints that stream large/long-lived responses
/// (image pull/push/build, container logs/attach/exec). These must NOT be
/// buffered -- they can be gigabytes or run indefinitely.
fn is_streaming_endpoint(path: &str) -> bool {
    // Image operations: POST /images/create (pull), POST /images/*/push, POST /build
    if path.contains("/images/create") || path.contains("/push") || path.contains("/build") {
        return true;
    }
    // Container streaming: logs, attach, exec/*/start
    if path.contains("/logs") || path.contains("/attach") || path.contains("/exec/") {
        return true;
    }
    false
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

    // Streaming endpoints (image pull, logs, attach, build) pass through
    // without buffering -- they can be gigabytes or run indefinitely.
    if is_streaming_endpoint(new_url.path()) {
        debug!("Streaming endpoint: {}", new_url.path());
        let forwarded_req = client.request_from(new_url.as_str(), req.head());
        let res = forwarded_req
            .send_body(body)
            .await
            .map_err(web::Error::from)?;
        let mut client_resp = web::HttpResponse::build(res.status());
        client_resp.content_type(res.content_type().to_string().as_str());
        client_resp.header("Connection", "close");
        return Ok(client_resp.streaming(res));
    }

    let forwarded_req = client.request_from(new_url.as_str(), req.head());
    let mut res = forwarded_req
        .send_body(body)
        .await
        .map_err(web::Error::from)?;

    let status = res.status();
    let content_type = res.content_type().to_string();

    // Buffer the ENTIRE response body immediately to release the upstream
    // connection back to the pool. Without this, concurrent requests each
    // hold an open connection while making inspect sub-requests, exhausting
    // the pool and deadlocking the single-threaded runtime.
    let res_body = res.body().limit(2097152).await.map_err(web::Error::from)?;

    let mut client_resp = web::HttpResponse::build(status);
    client_resp.content_type(content_type.as_str());
    // Prevent the Docker client (Go net/http) from reusing this connection.
    client_resp.header("Connection", "close");

    if status == 200 {

        if new_url.path().contains("containers/json") {
            let _list_span = span!(Level::DEBUG, "Container List").entered();

            let containers: Vec<ContainerSummary> = match serde_json::from_slice(&res_body) {
                Ok(list) => list,
                Err(e) => { panic!("{e}"); }
            };

            let filtered_containers = containers.iter()
                .filter(|con| {
                    let _list_span = span!(Level::DEBUG, "Container", id = utils::short_id(con.id.as_ref().unwrap())).entered();
                    docker::container_summary_match(con, app_config.get_ref())
                })
                .collect::<Vec<&ContainerSummary>>();

            debug!("{} of {} containers valid", filtered_containers.len(), containers.len());
            Ok(client_resp.json(&filtered_containers))
        } else {

            match docker::match_container_get(new_url.path()) {
                Some (m) => {
                    let short_cid = utils::short_id(&m.id);
                    let _con_span = span!(Level::DEBUG, "Container", id = short_cid).entered();
                    debug!("Matched container route with Id {}", &m.id);

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
                            if *n {
                                debug!("Matched container filters");
                                if m.resource == "json" {
                                    client_resp.content_type("application/json");
                                    if app_config.get_ref().scrub_envs {
                                        let mut container: ContainerInspect = serde_json::from_slice(&res_body).unwrap();
                                        container.config.as_mut().unwrap().env = Some(Vec::new());
                                        Ok(client_resp.json(&container))
                                    } else {
                                        Ok(client_resp.body(res_body))
                                    }
                                } else {
                                    client_resp.content_type(content_type.as_str());
                                    Ok(client_resp.body(res_body))
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
                None => {
                    Ok(client_resp.body(res_body))
                }
            }
        }

    } else {
        Ok(client_resp.body(res_body))
    }
}
