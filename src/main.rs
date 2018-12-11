mod build;
mod kll;

use crate::build::*;
use crate::kll::*;

use serde_json;

use std::fs;
use std::collections::hash_map::{DefaultHasher, HashMap};
use std::process::Command;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use bodyparser;
use iron::prelude::*;
use iron::{headers, modifiers::Header, status, typemap::Key};
use persistent::{Read, Write};
use shared_child::SharedChild;

const API_HOST: &str = "localhost:3000";
//const FILE_HOST: &str = "https://configurator.input.club";
const FILE_HOST: &str = "http://localhost:8080";
const MAX_BODY_LENGTH: usize = 1024 * 1024 * 10;

#[derive(Copy, Clone)]
pub struct JobQueue;
impl Key for JobQueue {
    type Value = HashMap<String, Option<Arc<SharedChild>>>;
}

fn build_request(req: &mut Request<'_, '_>) -> IronResult<Response> {
    if let Ok(Some(body)) = req.get::<bodyparser::Struct<BuildRequest>>() {
        let container = "controller-050";

        let config = body.config;
        let name = config.header.name.replace(" ", "_"); //sanitize
        let variant = config
            .header
            .variant
            .clone()
            .unwrap_or("".to_string())
            .replace(" ", "_");

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

                let build_dir = format!("{}/{}", "tmp_kll", hash);
                fs::create_dir_all(&build_dir).unwrap();

                let mut layers: Vec<String> = Vec::new();
                let files = generate_kll(config, body.env == "lts");
                for file in files {
                    let filename = format!("{}/{}", build_dir, file.name);
                    fs::write(&filename, file.content).unwrap();
                    layers.push(format!("{}/{}/{}", "/tmp/kll", hash, filename));
                }

                let process = start_build(container, &hash, &name, &variant, layers);
                let arc = Arc::new(process);
                (*queue).insert(hash.clone(), Some(arc.clone()));
                Some(arc)
            }

            // drop lock
        };

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
                println!(" > Job already in finished {}. Nothing to do.", hash);
                true
            }
        };

        if success {
            let result = BuildResult {
                filename: format!("{}/{}.zip", FILE_HOST, hash),
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
    let status = Command::new("docker-compose")
        .args(&["-f", "docker-compose.yml", "up", "-d", "--no-recreate"])
        .status()
        .expect("Failed!");

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

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

    println!("\nBuild dispatcher starting.\nListening on {}", API_HOST);
    let mut chain = Chain::new(build_request);
    chain.link_before(Write::<JobQueue>::one(queue));
    chain.link_before(Read::<bodyparser::MaxBodyLength>::one(MAX_BODY_LENGTH));
    Iron::new(chain).http(API_HOST).unwrap();
}
