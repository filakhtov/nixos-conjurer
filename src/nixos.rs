use std::fmt::Display;

use crate::process::run_command_checked;

#[derive(Debug)]
pub struct Error {
    message: String,
}

impl Error {
    pub fn new(message: String) -> Self {
        Self { message }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for Error {}

pub fn add_channel<C: AsRef<str>>(channel: C) -> Result<(), Error> {
    let args: Vec<&str> = vec!["--add", channel.as_ref()];

    if let Err(e) = run_command_checked("nix-channel", &args) {
        return Err(Error::new(format!("failed to add the Nix channel: {}", e)));
    }

    Ok({})
}

pub fn update_channels() -> Result<(), Error> {
    if let Err(e) = std::fs::remove_dir("/nix/var/nix/profiles/default") {
        return Err(Error::new(format!(
            "failed to remove default profile directory: {}",
            e
        )));
    }

    if let Err(e) = run_command_checked("nix-channel", &["--update"]) {
        return Err(Error::new(format!("failed to update Nix channels: {}", e)));
    }

    Ok({})
}

pub fn install<P: AsRef<str>>(packages: Vec<P>) -> Result<(), Error> {
    let mut args: Vec<&str> = vec!["-iA"];
    args.extend(packages.iter().map(|s| s.as_ref()));

    if let Err(e) = run_command_checked("nix-env", &args) {
        return Err(Error::new(format!(
            "failed to install the Nix packages: {}",
            e
        )));
    }

    Ok({})
}
