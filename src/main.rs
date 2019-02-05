mod build;
mod kll;

use crate::build::*;
use crate::kll::*;

use std::collections::hash_map::{DefaultHasher, HashMap};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use bodyparser;
use iron::prelude::*;
use iron::{headers, modifiers::Header, status, typemap::Key};
use logger::Logger;
use mount::Mount;
use persistent::{Read, Write};
use router::Router;
use staticfile::Static;
use urlencoded::UrlEncodedQuery;

use chrono::prelude::*;
use rusqlite::{types::ToSql, Connection};

use serde_derive::{Deserialize, Serialize};
use serde_json;
use shared_child::SharedChild;

const API_HOST: &str = "0.0.0.0:3001";
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

#[derive(Clone)]
pub enum JobEntry {
    Building(Arc<SharedChild>),
    Finished(bool),
}

#[derive(Copy, Clone)]
pub struct JobQueue;
impl Key for JobQueue {
    type Value = HashMap<String, JobEntry>;
}

#[derive(Copy, Clone)]
pub struct VersionsMap;
impl Key for VersionsMap {
    type Value = HashMap<String, String>;
}

fn get_layout(req: &mut Request<'_, '_>) -> IronResult<Response> {
    let mut default_params = HashMap::new();
    default_params.insert("rev".to_string(), vec!["HEAD".to_string()]);
    let params = &req.get::<UrlEncodedQuery>().unwrap_or(default_params);
    let rev = &params.get("rev").unwrap()[0];

    let file = &req
        .extensions
        .get::<Router>()
        .unwrap()
        .find("file")
        .unwrap_or("/");

    let path = PathBuf::from(format!("{}/{}", LAYOUT_DIR, file));
    let realfile = fs::read_link(&path).unwrap_or(PathBuf::from(file));
    let realpath = format!("{}/{}", LAYOUT_DIR, realfile.to_str().unwrap());
    println!("Get layout {:?} ({})", file, rev);

    let result = Command::new("git")
        .args(&["show", &format!("{}:{}", rev, realpath)])
        .output()
        .expect("Failed!");
    let content = String::from_utf8_lossy(&result.stdout).to_string();

    Ok(Response::with((
        status::Ok,
        Header(headers::ContentType::json()),
        content,
    )))
}

fn build_request(req: &mut Request<'_, '_>) -> IronResult<Response> {
    if let Ok(Some(body)) = req.get::<bodyparser::Struct<BuildRequest>>() {
        let config = body.config;
        let container = match body.env.as_ref() {
            "lts" => "controller-050",
            "latest" | _ => "controller-054",
        }
        .to_string();

        let config_str = serde_json::to_string(&config).unwrap();

        let request_time: DateTime<Utc> = Utc::now();

        let hash = {
            let mut hasher = DefaultHasher::new();
            container.hash(&mut hasher);
            //body.hash(&mut hasher);
            let h = hasher.finish();
            format!("{:x}", h)
        };
        println!("Received request: {}", hash);

        let job: JobEntry = {
            let mutex = req.get::<Write<JobQueue>>().expect("Could not find mutex");
            let mut queue = mutex.lock().expect("Could not lock mutex"); // *** Panics if poisoned **

            if let Some(job) = (*queue).get(&hash) {
                println!(" > Existing task");
                job.clone()
            } else {
                println!(" > Starting new build");

                let config_dir = format!("{}/{}", CONFIG_DIR, hash);
                fs::create_dir_all(&config_dir).expect("Could not create directory");

                let mut layers: Vec<String> = Vec::new();
                let files = generate_kll(&config, body.env == "lts");
                for file in files {
                    let filename = format!("{}/{}", config_dir, file.name);
                    fs::write(&filename, file.content).expect("Could not write kll file");
                    layers.push(format!("{}", filename));
                }

                let info = configure_build(&config, layers);
                let output_file = format!("{}-{}-{}.zip", info.name, info.layout, hash);
                println!("{:?}", info);

                let config_file = format!("{}/{}-{}.json", config_dir, info.name, info.layout);
                fs::write(&config_file, &config_str).expect("Could not write config file");

                let process = start_build(container.clone(), info, hash.clone(), output_file);
                let job = JobEntry::Building(Arc::new(process));
                (*queue).insert(hash.clone(), job.clone());
                job
            }

            // drop lock
        };

        let info = configure_build(&config, vec!["".to_string()]);
        let mut output_file = format!("{}-{}-{}.zip", info.name, info.layout, hash);

        let (success, duration) = match job {
            JobEntry::Building(arc) => {
                let process = arc.clone();
                println!(" > Waiting for task to finish {}", process.id());
                let exit_status = process.wait().unwrap();
                let success: bool = exit_status.success();
                println!(" > Done");

                {
                    let rwlock = req.get::<Write<JobQueue>>().expect("Could not find mutex");
                    let mut queue = rwlock.lock().expect("Could not lock mutex");
                    let job = (*queue).get_mut(&hash).expect("Could not find job");
                    *job = JobEntry::Finished(success);
                    // drop lock
                }

                let duration = Some(Utc::now().signed_duration_since(request_time));
                (success, duration)
            }
            JobEntry::Finished(success) => {
                println!(" > Job already in finished {}. Updating time.", hash);
                (success, None)
            }
        };

        let build_duration = match duration {
            Some(t) => Some(t.num_milliseconds()),
            None => None,
        };
        println!(
            "Started at: {:?}, Duration: {:?}",
            request_time, build_duration
        );

        if !success {
            output_file = format!("{}-{}-{}_error.zip", info.name, info.layout, hash);
        }

        let result = BuildResult {
            filename: format!("{}/{}", BUILD_ROUTE, output_file),
            success: success,
        };

        return Ok(Response::with((
            status::Ok,
            Header(headers::ContentType::json()),
            serde_json::to_string(&result).unwrap(),
        )));
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

fn versions_request(req: &mut Request<'_, '_>) -> IronResult<Response> {
    let versions = req.get::<Read<VersionsMap>>().unwrap();

    Ok(Response::with((
        status::Ok,
        Header(headers::ContentType::json()),
        serde_json::to_string(&*versions).unwrap(),
    )))
}

fn version_map() -> HashMap<String, String> {
    let mut versions: HashMap<String, String> = HashMap::new();
    versions.insert("latest".to_string(), "controller-053".to_string());
    versions.insert("lts".to_string(), "controller-050".to_string());
    versions.insert("v.0.5.3".to_string(), "controller-053".to_string());
    versions.insert("v.0.5.2".to_string(), "controller-052".to_string());
    versions.insert("v.0.5.1".to_string(), "controller-051".to_string());
    versions.insert("v.0.5.0".to_string(), "controller-050".to_string());
    versions.insert("0.4.9".to_string(), "controller-049".to_string());

    let containers = list_containers();
    versions
        .into_iter()
        .filter(|(_, v)| containers.contains(&v))
        .collect()
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

    let queue: HashMap<String, JobEntry> = HashMap::new();

    /*println!("\nExisting builds: ");
    let builds = get_builds("controller-050");
    for build in builds.lines().skip(1) {
        println!(" - {}", build);
        queue.insert(build.to_string(), None);
    }

    println!("\nBuilds to purge: ");
    old_builds("controller-050");
    println!("");*/

    let versions = version_map();
    println!("\nVersions:");
    println!("{:#?}", versions);

    let (logger_before, logger_after) = Logger::new(None);

    let mut layout_router = Router::new();
    layout_router.get("/:file", get_layout, "layout");

    let mut mount = Mount::new();
    //mount.mount("/layouts/", Static::new(Path::new(LAYOUT_DIR)));
    mount.mount("/layouts/", layout_router);
    mount.mount("/tmp/", Static::new(Path::new(BUILD_DIR)));
    mount.mount("/versions", versions_request);
    mount.mount("/", build_request);

    println!("\nBuild dispatcher starting.\nListening on {}", API_HOST);
    let mut chain = Chain::new(mount);
    chain.link_before(Write::<JobQueue>::one(queue));
    chain.link_before(Read::<VersionsMap>::one(versions));
    chain.link_before(Read::<bodyparser::MaxBodyLength>::one(MAX_BODY_LENGTH));
    chain.link_before(logger_before);
    chain.link_after(logger_after);
    Iron::new(chain).http(API_HOST).unwrap();
}
