mod build;
mod kll;

use crate::build::*;
use crate::kll::*;

use std::collections::hash_map::{DefaultHasher, HashMap};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::Arc;

use bodyparser;
use iron::prelude::*;
use iron::{headers, modifiers::Header, status, typemap::Key};
use logger::Logger;
use mount::Mount;
use persistent::{Read, Write};
use staticfile::Static;

use serde_derive::{Deserialize, Serialize};
use serde_json;
use shared_child::SharedChild;

const API_HOST: &str = "0.0.0.0:3000";
const MAX_BODY_LENGTH: usize = 1024 * 1024 * 10;
const BUILD_ROUTE: &str = "./tmp";

const LAYOUT_DIR: &str = "./layouts";
const BUILD_DIR: &str = "./tmp_builds";
const CONFIG_DIR: &str = "./tmp_config";

#[derive(Clone, Deserialize)]
pub struct BuildRequest {
    pub config: KllConfig,
    pub env: String,
}

#[derive(Clone, Serialize)]
pub struct BuildResult {
    pub filename: String,
    pub success: bool,
}

#[derive(Copy, Clone)]
pub struct JobQueue;
impl Key for JobQueue {
    type Value = HashMap<String, Option<Arc<SharedChild>>>;
}

fn build_request(req: &mut Request<'_, '_>) -> IronResult<Response> {
    if let Ok(Some(body)) = req.get::<bodyparser::Struct<BuildRequest>>() {
        let config = body.config;
        let container = match body.env.as_ref() {
            "lts" => "controller-050",
            "latest" => "controller-050",
            _ => "controller-050",
        }
        .to_string();

        let hash = {
            let mut hasher = DefaultHasher::new();
            container.hash(&mut hasher);
            //body.hash(&mut hasher);
            let h = hasher.finish();
            format!("{:x}", h)
        };
        println!("Received request: {}", hash);

        let job: Option<Arc<SharedChild>> = {
            let mutex = req.get::<Write<JobQueue>>().unwrap();
            let mut queue = mutex.lock().unwrap();

            if let Some(job) = (*queue).get(&hash) {
                println!(" > Existing task");
                job.clone()
            } else {
                println!(" > Starting new build");

                let config_dir = format!("{}/{}", CONFIG_DIR, hash);
                fs::create_dir_all(&config_dir).unwrap();

                let mut layers: Vec<String> = Vec::new();
                let files = generate_kll(&config, body.env == "lts");
                for file in files {
                    let filename = format!("{}/{}", config_dir, file.name);
                    fs::write(&filename, file.content).unwrap();
                    layers.push(format!("{}", filename));
                }

                let info = configure_build(&config, layers);
                let output_file = format!("{}-{}-{}.zip", info.name, info.variant, hash);
                println!("{:?}", info);

                let config_file = format!("{}/{}-{}.json", config_dir, info.name, info.variant);
                fs::write(&config_file, serde_json::to_string(&config).unwrap()).unwrap();

                let process = start_build(container, info, hash.clone(), output_file);
                let arc = Arc::new(process);
                (*queue).insert(hash.clone(), Some(arc.clone()));
                Some(arc)
            }

            // drop lock
        };

        let info = configure_build(&config, vec!["".to_string()]);
        let output_file = format!("{}-{}-{}.zip", info.name, info.variant, hash);

        let success = match job {
            Some(arc) => {
                let process = arc.clone();
                println!(" > Waiting for task to finish {}", process.id());
                let exit_status = process.wait().unwrap();
                println!(" > Done");

                {
                    let rwlock = req.get::<Write<JobQueue>>().unwrap();
                    let mut queue = rwlock.lock().unwrap();
                    let job = (*queue).get_mut(&hash).unwrap();
                    *job = None;
                    // drop lock
                }

                exit_status.success()
            }
            None => {
                println!(" > Job already in finished {}. Updating time.", hash);
                true
            }
        };

        if success {
            let result = BuildResult {
                filename: format!("{}/{}", BUILD_ROUTE, output_file),
                success: true,
            };

            return Ok(Response::with((
                status::Ok,
                Header(headers::ContentType::json()),
                serde_json::to_string(&result).unwrap(),
            )));
        } else {
            return Ok(Response::with((
                status::InternalServerError,
                Header(headers::ContentType::json()),
                "{ error: \"build failed\" }",
            )));
        }
    } else if let Err(err) = req.get::<bodyparser::Struct<BuildRequest>>() {
        println!("Parse error: {:?}", err);
        use bodyparser::BodyErrorCause::JsonError;
        let s = if let JsonError(e) = err.cause {
            println!("e: {:?}", e);
            e.to_string()
        } else {
            err.detail
        };

        return Ok(Response::with((
            status::BadRequest,
            Header(headers::ContentType::json()),
            format!("{{ \"error\": \"{}\" }}", s),
        )));
    }

    return Ok(Response::with((
        status::BadRequest,
        Header(headers::ContentType::json()),
        "{ \"error\": \"bad request\" }",
    )));
}

fn main() {
    pretty_env_logger::init();

    /*let status = Command::new("docker-compose")
        .args(&["-f", "docker-compose.yml", "up", "-d", "--no-recreate"])
        .status()
        .expect("Failed!");

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }*/

    let queue: HashMap<String, Option<Arc<SharedChild>>> = HashMap::new();

    /*println!("\nExisting builds: ");
    let builds = get_builds("controller-050");
    for build in builds.lines().skip(1) {
        println!(" - {}", build);
        queue.insert(build.to_string(), None);
    }

    println!("\nBuilds to purge: ");
    old_builds("controller-050");
    println!("");*/

    println!("\nAvailable build containers:");
    println!("{:?}", list_containers());

    let (logger_before, logger_after) = Logger::new(None);

    let mut mount = Mount::new();
    mount.mount("/layouts/", Static::new(Path::new(LAYOUT_DIR)));
    mount.mount("/tmp/", Static::new(Path::new(BUILD_DIR)));
    mount.mount("/", build_request);

    println!("\nBuild dispatcher starting.\nListening on {}", API_HOST);
    let mut chain = Chain::new(mount);
    chain.link_before(Write::<JobQueue>::one(queue));
    chain.link_before(Read::<bodyparser::MaxBodyLength>::one(MAX_BODY_LENGTH));
    chain.link_before(logger_before);
    chain.link_after(logger_after);
    Iron::new(chain).http(API_HOST).unwrap();
}
