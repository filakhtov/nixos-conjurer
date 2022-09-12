use crate::http;
use crate::process::run_command_checked;
use serde::Deserialize;
use serde_yaml;
use sha2::{Digest, Sha512};
use std::fs::File;
use std::io::{copy, Read, Result as IoResult};
use std::path::Path;

type Result<T> = core::result::Result<T, Error>;

macro_rules! err {
    ($($args:expr),+) => {{
        return Err(Error::new(format!($($args,)+)))
    }};
}

pub struct BaseSystemDownloader {
    client: http::Client,
}

impl BaseSystemDownloader {
    pub fn new(client: http::Client) -> Self {
        Self { client }
    }

    pub fn download<P: AsRef<Path>>(&self, destination_path: P) -> Result<()> {
        Ok(match self.download_impl(destination_path.as_ref()) {
            Ok(_) => {}
            Err(e) => err!(
                "unable to download and verify Alpine base system tarball: {}",
                e
            ),
        })
    }

    fn download_impl(&self, p: &Path) -> Result<()> {
        let a = "x86_64";
        let version_file = self.download_version_file(a)?;
        let release_info = parse_release_info(&version_file)?;
        let downloaded_size = self.download_tarball(a, &release_info.file, p)?;
        verify_tarball_size(downloaded_size, release_info.size)?;
        verify_checksum(p, &release_info.sha512)?;
        Ok({})
    }

    fn download_version_file(&self, a: &str) -> Result<String> {
        Ok(match self.download_verion_file_impl(a) {
            Ok(v) => v,
            Err(e) => err!("failed to download the version file: {}", e),
        })
    }

    fn download_verion_file_impl(&self, a: &str) -> http::Result<String> {
        let url = format!(
            "https://dl-cdn.alpinelinux.org/alpine/latest-stable/releases/{}/latest-releases.yaml",
            a
        );

        let req = http::GetRequest::new(url)?;
        let response = self.client.get(req)?.as_text()?;

        Ok(response)
    }

    fn download_tarball(&self, a: &str, t: &str, p: &Path) -> Result<u64> {
        let reader = match self.download_tarball_impl(a, t) {
            Ok(r) => r,
            Err(e) => err!("download failed: {}", e),
        };

        Ok(match write_tarball(reader, p) {
            Ok(s) => s,
            Err(e) => err!("failed to write tarball file: {}", e),
        })
    }

    fn download_tarball_impl(&self, a: &str, t: &str) -> http::Result<impl Read> {
        let url = format!(
            "https://dl-cdn.alpinelinux.org/alpine/latest-stable/releases/{}/{}",
            a, t
        );

        let req = http::GetRequest::new(url)?;
        let response = self.client.get(req)?.as_reader()?;

        Ok(response)
    }
}

#[derive(Deserialize)]
struct VersionFile {
    flavor: String,
    file: String,
    size: u64,
    sha512: String,
}

pub struct Error {
    error: String,
}

impl Error {
    pub fn new<S: AsRef<str>>(error: S) -> Self {
        let error = error.as_ref().to_owned();

        Self { error }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.error)
    }
}

fn parse_release_info(f: &str) -> Result<VersionFile> {
    let vf: Vec<VersionFile> = match serde_yaml::from_str(f) {
        Ok(f) => f,
        Err(e) => err!("failed to parse the version file: {}", e),
    };

    for rel in vf {
        if rel.flavor == "alpine-minirootfs" {
            return Ok(rel);
        }
    }

    err!("unable to find the `alpine-minirootfs` release in the version file")
}

fn verify_checksum(p: &Path, c: &str) -> Result<()> {
    match verify_checksum_impl(p, c) {
        Ok(_) => Ok({}),
        Err(e) => err!("checksum verification failed: {}", e),
    }
}

fn verify_checksum_impl(p: &Path, c: &str) -> IoResult<()> {
    let mut hasher = Sha512::new();
    let mut file = File::open(p)?;

    copy(&mut file, &mut hasher)?;
    let actual_checksum = format!("{:x}", hasher.finalize());

    if actual_checksum == c {
        return Ok({});
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::Other,
        format!(
            "SHA-512 checksum doesn't match. Expected `{}`, got `{}`",
            c, actual_checksum
        ),
    ))
}

fn verify_tarball_size(download_size: u64, expected_size: u64) -> Result<()> {
    if download_size == expected_size {
        return Ok({});
    }

    err!(
        "size mismatch: expected {}, but got {}",
        expected_size,
        download_size
    )
}

fn write_tarball(r: impl Read, p: &Path) -> IoResult<u64> {
    let mut r = r;
    let mut file = File::create(p)?;
    copy(&mut r, &mut file)
}

pub fn enable_edge_repositories() -> Result<()> {
    let repository_conf_path = "/etc/apk/repositories";
    let repositories = "https://dl-cdn.alpinelinux.org/alpine/edge/main/\n\
        https://dl-cdn.alpinelinux.org/alpine/edge/community/\n\
        https://dl-cdn.alpinelinux.org/alpine/edge/testing/\n";

    if let Err(e) = std::fs::write(repository_conf_path, repositories) {
        err!("failed to enable edge repositories: {}", e);
    }

    Ok({})
}

fn run_apk(args: &[&str]) -> Result<()> {
    if let Err(e) = run_command_checked("apk", &args) {
        err!("{}", e);
    }

    Ok({})
}

pub fn update_repositories() -> Result<()> {
    match run_apk(&vec!["update"]) {
        Ok(_) => Ok({}),
        Err(e) => err!("failed to update repositories: {}", e),
    }
}

pub fn install_packages(packages: &[&str]) -> Result<()> {
    let mut args = vec!["add"];
    args.extend(packages);
    match run_apk(&args) {
        Ok(_) => Ok({}),
        Err(e) => err!("failed to install packages: {}", e),
    }
}
