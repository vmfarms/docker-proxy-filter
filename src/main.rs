#[macro_use]
extern crate slog;
extern crate slog_async;
extern crate slog_term;

use futures_util::TryStreamExt;
use ntex::{http, web};
use slog::Drain;
use dotenv::dotenv;
use std::env;
//use std::collections::HashMap;
use serde::{Deserialize, Serialize};


#[derive(Serialize, Deserialize)]
struct Container {
    Id: String,
    Names: Vec<String>
}

async fn forward(
    req: web::HttpRequest,
    body: ntex::util::Bytes,
    client: web::types::State<http::Client>,
    forward_url: web::types::State<url::Url>,
    container_id: web::types::State<String>,
) -> Result<web::HttpResponse, web::Error> {
    let mut new_url = forward_url.get_ref().clone();
    new_url.set_path(req.uri().path());
    new_url.set_query(req.uri().query());
    let forwarded_req = client.request_from(new_url.as_str(), req.head());
    let res = forwarded_req
        .send_body(body)
        .await
        .map_err(web::Error::from)?;
    // if new_url.path().contains("containers/json") {
    //     let content = Vec<>;
    //     res.json()
    // }
    let mut client_resp = web::HttpResponse::build(res.status());
    let stream = res.into_stream();
    Ok(client_resp.streaming(stream))
}

async fn get_container(u: &String, container_name: &String) -> Result<Option<Container>, Box<dyn std::error::Error>> {

    let resp = reqwest::get(format!("{}/containers/json", u))
        .await?
        .json::<Vec<Container>>()
        .await?;
    let container = resp.into_iter().find(|x| Option::is_some(&x.Names.iter().find(|y| y.contains(container_name)))); //y == &container_name
    Ok(container)
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

    let container_res = get_container(&proxy_url, &container_name).await;

    let container: Container = match container_res {
        Ok(val) => {


            match val {
                Some(v) => {
                    info!(log, "Found container matching name '{}': {}", &container_name, v.Id);
                    v
                }
                None => {
                    crit!(log, "Could not find a contaienr with name '{}'", &container_name);
                    panic!();
                }    
            }
        },
        Err(e) => {
            crit!(log, "Error occurred while trying to get container"; "error" => %e);
            panic!();
        },
    };

    info!(log, "fsdf {}", container.Id);

    let forward_url = url::Url::parse(&proxy_url)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
    web::server(move || {
        web::App::new()
            .state(http::Client::new())
            .state(forward_url.clone())
            .state(container.Id.clone())
            .wrap(web::middleware::Logger::default())
            .default_service(web::route().to(forward))
    })
    .bind(("0.0.0.0", 9090))?
    .run()
    .await
}
