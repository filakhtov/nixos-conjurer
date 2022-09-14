use crate::{
    alpine::BaseSystemDownloader,
    builder::{self, Builder},
    http::Client,
    process::run_forked,
};
use std::path::{Path, PathBuf};

pub struct App {
    builder: Builder,
}

pub fn init_app() -> App {
    let client = Client::builder()
        .connect_timeout(None)
        .request_timeout(None)
        .build()
        .expect("Failed to initialize the HTTP client");

    let bsd = BaseSystemDownloader::new(client);
    let builder = Builder::new(bsd);

    App { builder }
}

impl App {
    pub fn run(&self) -> Result<(), ()> {
        // Prepare chroot environment
        let build_dir = self.builder.create_chroot()?;

        // Run the build process in an isolated chroot environment
        let image_path = match run_forked(|| build(build_dir.path())) {
            Ok(r) => r?,
            Err(e) => {
                eprintln!("!!! FAILURE: {}", e);

                return Err({});
            }
        };

        // Pull the image out of temporary root directory
        // TODO: make destination configurable
        builder::pull_image(&image_path)?;

        Ok({})
    }
}

fn build(root_path: &Path) -> Result<PathBuf, ()> {
    // Create a new namespace for the build process
    builder::setup_namespace(root_path)?;

    // Run the build process in the new namespace
    let image_path = match run_forked(|| builder::run_build_process()) {
        Ok(p) => p?,
        Err(e) => {
            eprintln!("!!! FAILURE: {}", e);

            return Err({});
        }
    };

    // Clean up after the build process
    builder::clean_up();

    // Return the resulting absolute path to the built image
    Ok(root_path.join(&image_path))
}
