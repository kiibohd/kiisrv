mod build;
mod kll;
mod versions;

use crate::build::*;
use crate::kll::*;
//use crate::versions::version_map;

use indexmap::IndexMap;
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

const MAX_BODY_LENGTH: usize = 1024 * 1024 * 10;
const BUILD_ROUTE: &str = "./tmp";

const LAYOUT_DIR: &str = "./layouts";
const BUILD_DIR: &str = "./tmp_builds";
const CONFIG_DIR: &str = "./tmp_config";

const STATS_DB_FILE: &str = "./stats.db";
const STATS_DB_SCHEMA: &str = include_str!("../schema/stats.sqlite");

const CONFIG_DB_FILE: &str = "./config.db";
const CONFIG_DB_SCHEMA: &str = include_str!("../schema/config.sqlite");

const CONTROLLER_GIT_URL: &str = "https://github.com/kiibohd/controller.git";
const CONTROLLER_GIT_REMOTE: &str = "controller";

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
pub struct StatsDatabase;
impl Key for StatsDatabase {
    type Value = rusqlite::Connection;
}

#[derive(Copy, Clone)]
pub struct Versions;
impl Key for Versions {
    type Value = HashMap<String, VersionInfo>;
}

#[derive(Debug)]
struct RequestLog {
    id: i32,
    uid: Option<i32>,
    ip_addr: String,
    os: String,
    web: bool,
    serial: Option<i32>,
    hash: String,
    board: String,
    variant: String,
    layers: i32,
    container: String,
    success: bool,
    request_time: DateTime<Utc>,
    build_duration: Option<i32>,
}
impl RequestLog {
    fn from_row(row: &rusqlite::Row) -> Self {
        RequestLog {
            id: row.get(0),
            uid: row.get(1),
            ip_addr: row.get(2),
            os: row.get(3),
            web: row.get(4),
            serial: row.get(5),
            hash: row.get(6),
            board: row.get(7),
            variant: row.get(8),
            layers: row.get(9),
            container: row.get(10),
            success: row.get(11),
            request_time: row.get(12),
            build_duration: row.get(13),
        }
    }
}

#[derive(Debug)]
struct VersionMap {
    name: String,
    channel: String,
    container: String,
    git_tag: String,
}
impl VersionMap {
    fn from_row(row: &rusqlite::Row) -> Self {
        VersionMap {
            name: row.get(0),
            channel: row.get(1),
            container: row.get(2),
            git_tag: row.get(3),
        }
    }
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
        let ip = req.remote_addr.ip();
        let user_agent = req
            .headers
            .get::<headers::UserAgent>()
            .unwrap_or(&iron::headers::UserAgent("".to_owned()))
            .to_string();

        let os = {
            let ua = user_agent.to_lowercase();
            if ua.contains("windows") {
                "Windows"
            } else if ua.contains("mac") {
                "Mac"
            } else if ua.contains("linux") || ua.contains("x11") {
                "Linux"
            } else {
                "Unknown"
            }
        }
        .to_string();

        let is_desktop_configurator = user_agent.to_lowercase().contains("electron");
        println!("IP: {:?}", ip);
        println!("OS: {:?}", os);
        println!("WEB: {:?}", !is_desktop_configurator);

        let request_time: DateTime<Utc> = Utc::now();

        let config = body.config;
	//let versions = req.get::<Read<VersionsMap>>().unwrap();
        //let container = versions.get(body.env).unwrap_or("controller-050");
        let container = match body.env.as_ref() {
            "lts" => "controller-050",
            "nightly" => "controller-057",
            "latest" | _ => "controller-057",
        }
        .to_string();

        let config_str = serde_json::to_string(&config).unwrap();

        let request_time: DateTime<Utc> = Utc::now();

        let hash = {
            let mut hasher = DefaultHasher::new();
            container.hash(&mut hasher);
            config_str.hash(&mut hasher);
            let h = hasher.finish();
            format!("{:x}", h)
        };
        println!("Received request: {}", hash);

        let info = configure_build(&config, vec!["".to_string()]);
        let mut output_file = format!("{}-{}-{}.zip", info.name, info.layout, hash);
        //let file_exists = Path::new(&output_file).exists();
        //println!("Zip exists: {:?} ({})", file_exists, output_file)

        let job: JobEntry = {
            let mutex = req.get::<Write<JobQueue>>().expect("Could not find mutex");
            let queue = mutex.lock(); //.expect("Could not lock mutex"); // *** Panics if poisoned **
            if let Err(e) = queue {
                eprintln!("{:?}", e);
                std::process::exit(1);
            }
            let mut queue = queue.unwrap();
            let job = (*queue).get(&hash);

            //if file_exists && job.is_some() {
            if job.is_some() {
                println!(" > Existing task");
                job.unwrap().clone()
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

        let layers = vec![""];
        let args: &[&ToSql] = &[
            &(ip.to_string()),
            &os,
            &!is_desktop_configurator,
            &hash,
            &info.name,
            &info.layout,
            &(layers.len() as u32),
            &container,
            &success,
            &request_time,
            &build_duration,
        ];

        {
            let mutex = req
                .get::<Write<StatsDatabase>>()
                .expect("Could not find mutex");
            let db = mutex.lock().expect("Could not lock mutex");
            // TODO: uid, serial
            (*db).execute("INSERT INTO Requests (ip_addr, os, web, hash, board, variant, layers, container, success, request_time, build_duration)
                  VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", args).unwrap_or_else(|_| {
			println!("Error: Failed to insert request into stats db");
			0 as usize
		});
        }

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

fn stats(req: &mut Request<'_, '_>) -> IronResult<Response> {
    let mutex = req.get::<Write<StatsDatabase>>().unwrap();
    let db = mutex.lock().unwrap();
    let args: &[&ToSql] = &[];

    let mut result = String::new();
    let mut total_layers: usize = 0;
    let mut total_buildtime = 0;
    let mut os_counts: HashMap<String, usize> = HashMap::new();
    let mut platform_counts: HashMap<String, usize> = HashMap::new();
    let mut keyboard_counts: HashMap<String, usize> = HashMap::new();
    let mut container_counts: HashMap<String, usize> = HashMap::new();
    let mut hashes: Vec<String> = Vec::new();
    let mut users: Vec<String> = Vec::new();

    let mut stmt = (*db).prepare("SELECT * FROM Requests").unwrap();
    let rows = stmt
        .query_map(args, |row| RequestLog::from_row(row))
        .unwrap();
    for row in rows {
        let request = row.unwrap();
        println!("req: {:?}", request);

        let counter = os_counts.entry(request.os).or_insert(0);
        *counter += 1;

        let platform = match request.web {
            true => "Web",
            false => "Desktop",
        }
        .to_string();
        let counter = platform_counts.entry(platform).or_insert(0);
        *counter += 1;

        let keyboard = format!("{}-{}", request.board, request.variant);
        let counter = keyboard_counts.entry(keyboard).or_insert(0);
        *counter += 1;

        let counter = container_counts.entry(request.container).or_insert(0);
        *counter += 1;

        total_layers += request.layers as usize;
        total_buildtime += request.build_duration.unwrap_or(0) as i32;

        hashes.push(request.hash);
        users.push(request.ip_addr); //requst.uid
    }

    let total_builds = hashes.len();
    hashes.sort();
    hashes.dedup();
    let unique_builds = hashes.len();

    users.sort();
    users.dedup();
    let unique_users = users.len();

    let cache_ratio = match unique_builds {
        0 => 0.,
        _ => (total_builds as f32) / (unique_builds as f32),
    };

    let user_ratio = match unique_builds {
        0 => 0.,
        _ => (total_builds as f32) / (unique_users as f32),
    };

    let layers_ratio = match unique_builds {
        0 => 0.,
        _ => (total_layers as f32) / (total_builds as f32),
    };

    let build_time = match unique_builds {
        0 => 0,
        _ => total_buildtime / (unique_builds as i32),
    };

    result += &format!("Builds: {} ({} unique)\n", total_builds, unique_builds);
    result += &format!("Cache ratio: {:.1}\n", cache_ratio);
    result += &format!("Avg time: {:.3} s\n\n", (build_time as f32) / 1000.0);
    result += &format!("Users: {} unique\n", unique_users);
    result += &format!("Avg builds per user: {:.1}\n", user_ratio);
    result += &format!("Average number of layers: {}\n\n", layers_ratio);
    result += &format!("OS Counts: {:#?}\n", os_counts);
    result += &format!("Platform Counts: {:#?}\n", platform_counts);
    result += &format!("Keyboard Counts: {:#?}\n", keyboard_counts);
    result += &format!("Version Counts: {:#?}\n\n", container_counts);

    return Ok(Response::with((status::Ok, result)));
}

fn versions_request(req: &mut Request<'_, '_>) -> IronResult<Response> {
    let versions = req.get::<Read<Versions>>().unwrap();
    let versions: HashMap<String, Option<ReleaseInfo>> = (*versions)
        .iter()
        .map(|(k, v)| (k.clone(), v.info.clone()))
        .collect();

    Ok(Response::with((
        status::Ok,
        Header(headers::ContentType::json()),
        serde_json::to_string(&versions).unwrap(),
    )))
}

fn version_map(db: rusqlite::Connection) -> HashMap<String, VersionInfo> {
    let args: &[&ToSql] = &[];
    let mut stmt = db.prepare("SELECT * FROM Versions").unwrap();
    let rows = stmt
        .query_map(args, |row| VersionMap::from_row(row))
        .unwrap();
    let mut versions: Vec<VersionMap> = rows.map(|r| r.unwrap()).collect();

    let containers = list_containers();
    let tags = fetch_tags();
    versions
        .into_iter()
        .filter(|v| containers.contains(&v.container))
        .map(|v| {
            (
                v.name,
                VersionInfo {
                    container: v.container,
                    channel: v.channel,
                    info: tags.get(&v.git_tag).map(|v| v.clone()),
                },
            )
        })
        .collect()
}

fn fetch_tags() -> IndexMap<String, ReleaseInfo> {
    let result = Command::new("git")
        .args(&["ls-remote", "--tags", CONTROLLER_GIT_REMOTE])
        .output()
        .expect("Failed!");
    let out = String::from_utf8_lossy(&result.stdout);
    let mut map = out
        .lines()
        .filter(|l| !l.contains("^{}"))
        .map(|l| l.split("\t"))
        .map(|mut x| (x.next().unwrap().trim(), x.next().unwrap().trim()));

    let mut versions = IndexMap::new();

    for (h, t) in map.rev() {
        let hash = h.to_string();
        let tag = t.replace("refs/tags/", "");

        let result = Command::new("git")
            .args(&["rev-list", "--count", h])
            .output()
            .expect("Failed!");
        let commit: u16 = String::from_utf8_lossy(&result.stdout)
            .trim()
            .parse()
            .unwrap();
        let msb = ((commit & 0xFF00) >> 8) as u8;
        let lsb = ((commit & 0x00FF) >> 0) as u8;

        fn bcd_format(x: u8) -> String {
            if x > 99 {
                format!("{:x?}", x)
            } else {
                x.to_string()
            }
        }
        let bcd = format!("{}.{}", bcd_format(msb), bcd_format(lsb));

        let result = Command::new("git")
            .args(&["log", "-1", "--pretty=tformat:%ai", h])
            .output()
            .expect("Failed!");
        let out = String::from_utf8_lossy(&result.stdout);
        let date = out.trim().to_string();

        let notes = format!("https://github.com/kiibohd/controller/releases/tag/{}", tag);
        versions.insert(
            tag,
            ReleaseInfo {
                commit,
                date,
                hash,
                notes,
                bcd,
            },
        );
    }

    versions
}

#[derive(Debug, Clone)]
pub struct VersionInfo {
    container: String,
    channel: String,
    info: Option<ReleaseInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReleaseInfo {
    commit: u16,
    date: String,
    hash: String,
    bcd: String,
    notes: String,
}

fn main() {
    pretty_env_logger::init();

    let result = Command::new("git")
        .args(&["remote", "add", CONTROLLER_GIT_REMOTE, CONTROLLER_GIT_URL])
        .status()
        .expect("Failed");

    let result = Command::new("git")
        .args(&["fetch", CONTROLLER_GIT_REMOTE])
        .status()
        .expect("Failed");

    /*let status = Command::new("docker-compose")
        .args(&["-f", "docker-compose.yml", "up", "-d", "--no-recreate"])
        .status()
        .expect("Failed!");

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }*/

    let queue: HashMap<String, JobEntry> = HashMap::new();

    let args: &[&ToSql] = &[];
    let config_db = Connection::open(Path::new(CONFIG_DB_FILE)).unwrap();
    config_db.execute(CONFIG_DB_SCHEMA, args).unwrap();

    let stats_db = Connection::open(Path::new(STATS_DB_FILE)).unwrap();
    stats_db.execute(STATS_DB_SCHEMA, args).unwrap();

    /*println!("\nExisting builds: ");
    let builds = get_builds("controller-050");
    for build in builds.lines().skip(1) {
        println!(" - {}", build);
        queue.insert(build.to_string(), None);
    }

    println!("\nBuilds to purge: ");
    old_builds("controller-050");
    println!("");*/

    let containers = list_containers();
    println!("\nPossible containers:");
    println!("{:#?}", containers);

    let versions = version_map(config_db);
    println!("\nVersions:");
    for (v, i) in versions.iter() {
        println!("{} -> {} [{}]", v, i.container, i.channel);
    }

    let (logger_before, logger_after) = Logger::new(None);

    let mut layout_router = Router::new();
    layout_router.get("/:file", get_layout, "layout");

    let mut mount = Mount::new();
    //mount.mount("/layouts/", Static::new(Path::new(LAYOUT_DIR)));
    mount.mount("/layouts/", layout_router);
    mount.mount("/tmp/", Static::new(Path::new(BUILD_DIR)));
    mount.mount("/versions", versions_request);
    mount.mount("/", build_request);

    let host = std::env::var("KIISRV_HOST");
    let host = host.as_ref().map_or("0.0.0.0", String::as_str);

    let port = std::env::var("KIISRV_PORT");
    let port = port.as_ref().map_or("3001", String::as_str);

    let api_host: &str = &format!("{}:{}", host, port);
    println!("\nBuild dispatcher starting.\nListening on {}", api_host);

    let mut chain = Chain::new(mount);
    chain.link_before(Write::<JobQueue>::one(queue));
    chain.link_before(Write::<StatsDatabase>::one(stats_db));
    chain.link_before(Read::<Versions>::one(versions));
    chain.link_before(Read::<bodyparser::MaxBodyLength>::one(MAX_BODY_LENGTH));
    chain.link_before(logger_before);
    chain.link_after(logger_after);
    Iron::new(chain).http(api_host).unwrap();
}
