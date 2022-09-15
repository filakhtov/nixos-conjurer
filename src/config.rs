use std::{
    ffi::OsString,
    fs::File,
    path::{Path, PathBuf},
};

use serde::Deserialize;

#[derive(Deserialize)]
pub struct Configuration {
    output_path: Option<PathBuf>,
    output_format: String,
    nix_configuration_path: Option<PathBuf>,
    nix_configuration: Option<String>,
}

#[derive(Debug)]
pub struct Error {
    message: String,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for Error {}

impl Configuration {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let path = path.as_ref();
        let conf_file = open_config_file(path)?;
        let conf = parse_config_file(conf_file)?;
        conf.validate()?;

        Ok(conf)
    }

    fn validate(&self) -> Result<(), Error> {
        if let Some(_) = self.nix_configuration {
            if let Some(_) = self.nix_configuration_path {
                return Err(Error {
                    message: "Configuration file contains both `nix_configuration`\
                                    and `nix_configuration_path` options"
                        .into(),
                });
            }
        }

        Ok({})
    }

    pub fn output_path(&self) -> &Option<PathBuf> {
        &self.output_path
    }

    pub fn output_format(&self) -> OsString {
        OsString::from(&self.output_format)
    }

    pub fn nix_configuration_path(&self) -> &Option<PathBuf> {
        &self.nix_configuration_path
    }

    pub fn nix_configuration(&self) -> &Option<String> {
        &self.nix_configuration
    }

    pub fn has_nix_configuration(&self) -> bool {
        if let Some(_) = self.nix_configuration {
            return true;
        }

        if let Some(_) = self.nix_configuration_path {
            return true;
        }

        false
    }
}

fn open_config_file(path: &Path) -> Result<File, Error> {
    match File::open(path) {
        Ok(cf) => Ok(cf),
        Err(e) => {
            return Err(Error {
                message: format!(
                    "Failed to open configuration file `{}`: {}",
                    path.display(),
                    e
                ),
            })
        }
    }
}

fn parse_config_file(file: File) -> Result<Configuration, Error> {
    match serde_yaml::from_reader(file) {
        Ok(c) => Ok(c),
        Err(e) => {
            return Err(Error {
                message: format!("Failed to parse configuration file: {}", e),
            })
        }
    }
}
