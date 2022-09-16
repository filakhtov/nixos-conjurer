use crate::{
    alpine::BaseSystemDownloader,
    builder::{self, Builder},
    config::Configuration,
    http::Client,
    process::run_forked,
};
use std::path::{Path, PathBuf};

pub struct App {
    builder: Builder,
}

#[derive(Debug, Copy, Clone)]
pub enum ErrorCode {
    CommandLineParserError = 1,
    ConfigurationLoaderError = 2,
    InitializtionError = 3,
    RuntimeError = 4,
}

#[derive(Debug)]
pub struct Error {
    message: String,
    code: ErrorCode,
}

impl Error {
    fn new<M: AsRef<str>>(code: ErrorCode, message: M) -> Self {
        let message = message.as_ref().to_owned();

        Self { code, message }
    }

    pub fn code(&self) -> ErrorCode {
        self.code
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code as i32)
    }
}

impl std::error::Error for Error {}

pub fn init_app(args: &Vec<String>) -> Result<App, Error> {
    let conf_path = parse_arguments(args)?;
    let configuration = match Configuration::load(&conf_path) {
        Ok(c) => c,
        Err(e) => {
            return Err(Error::new(
                ErrorCode::ConfigurationLoaderError,
                format!("{}", e),
            ))
        }
    };

    let client = match Client::builder()
        .connect_timeout(None)
        .request_timeout(None)
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return Err(Error::new(
                ErrorCode::InitializtionError,
                format!("Failed to initialize the HTTP client: {}", e),
            ));
        }
    };

    let bsd = BaseSystemDownloader::new(client);
    let builder = Builder::new(bsd, configuration);

    Ok(App { builder })
}

fn parse_arguments(args: &Vec<String>) -> Result<PathBuf, Error> {
    if args.len() != 2 {
        return Err(Error::new(
            ErrorCode::CommandLineParserError,
            "Failed to parse command line arguments.",
        ));
    }

    Ok(PathBuf::from(&args[1]))
}

impl App {
    pub fn run(&self) -> Result<(), Error> {
        match self.run_build() {
            Err(_) => return Err(Error::new(ErrorCode::RuntimeError, "Build failed.")),
            _ => Ok({}),
        }
    }

    fn run_build(&self) -> Result<(), ()> {
        // Prepare chroot environment
        let build_dir = self.builder.create_chroot()?;

        // Run the build process in an isolated chroot environment
        let image_path = match run_forked(|| self.build(build_dir.path())) {
            Ok(r) => r?,
            Err(e) => {
                eprintln!("!!! FAILURE: {}", e);

                return Err({});
            }
        };

        // Pull the image out of temporary root directory
        self.builder.pull_image(&image_path)?;

        Ok({})
    }

    fn build(&self, root_path: &Path) -> Result<PathBuf, ()> {
        // Create a new namespace for the build process
        builder::setup_namespace(root_path)?;

        // Run the build process in the new namespace
        let image_path = match run_forked(|| self.builder.run_build_process()) {
            Ok(p) => p?,
            Err(e) => {
                eprintln!("!!! FAILURE: {}", e);

                return Err({});
            }
        };

        // Return the resulting absolute path to the built image
        Ok(root_path.join(&image_path))
    }
}
