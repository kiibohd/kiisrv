use crate::kll::KllConfig;

use std::process::Command;

use serde_derive::{Deserialize, Serialize};
use shared_child::SharedChild;

#[derive(Clone, Deserialize)]
pub struct BuildRequest {
    pub config: KllConfig,
    pub env:    String,
}

#[derive(Clone, Serialize)]
pub struct BuildResult {
    pub filename: String,
    pub success:  bool,
}

pub fn start_build(
    service: &str,
    hash: &str,
    scan: &str,
    varient: &str,
    layers: Vec<String>,
) -> SharedChild {
    /*let mut sleep = Command::new("sleep");
    sleep.args(&["10"]);
    let process = SharedChild::spawn(&mut sleep).expect("Failed to execute!");*/

    let mut args = vec![
        "-f".to_string(),
        "docker-compose-build.yml".to_string(),
        "run".to_string(),
        "--rm".to_string(),
        "-T".to_string(),
        service.to_string(),
        hash.to_string(),
        scan.to_string(),
        varient.to_string(),
    ];
    args.extend(layers);

    let mut compile = Command::new("docker-compose");
    compile.args(&args);
    let process = SharedChild::spawn(&mut compile).expect("Failed to execute!");

    println!(" >> Created PID: {}", process.id());
    return process;

    /*if result.status.success() {
        println!("Finished Build {}", hash);
        return Some(hash.to_string());
    } else {
        println!("Failure!");
        println!("{}", String::from_utf8_lossy(&result.stdout));
        println!("{}", String::from_utf8_lossy(&result.stderr));
    }
    
    None*/
}

pub fn list_containers() -> Vec<String> {
    let result = Command::new("docker-compose")
        .args(&[
            "-f",
            "docker-compose-build.yml",
            "config",
            "--services",
        ])
        .output()
        .expect("Failed!");
    let out = String::from_utf8_lossy(&result.stdout);
    out.lines().skip(1).map(|s| s.to_string()).collect()
}

pub fn get_builds(service: &str) -> String {
    let result = Command::new("docker-compose")
        .args(&[
            "-f",
            "docker-compose-build.yml",
            "run",
            "--rm",
            "--entrypoint",
            "/usr/bin/find",
            service,
            "/mnt/builds",
            "-printf",
            "%P\n",
        ])
        .output()
        .expect("Failed!");
    String::from_utf8_lossy(&result.stdout).to_string()
}

fn old_builds(service: &str) {
    let status = Command::new("docker-compose")
        .args(&[
            "-f",
            "docker-compose-build.yml",
            "run",
            "--rm",
            "--entrypoint",
            "/usr/bin/find",
            service,
            "/mnt/builds",
            //"-mtime", "+1",
            "-depth",
            "-mmin",
            "+5",
            "-print",
        ])
        .status()
        .expect("Failed!");

    if status.success() {
        println!("Success!");
    } else {
        println!("Failed.");
    }
}
