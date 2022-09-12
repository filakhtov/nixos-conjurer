use crate::{
    alpine::{
        enable_edge_repositories, install_packages, update_repositories, BaseSystemDownloader,
    },
    archive::extract,
    http::Client,
    mount::Mount,
    nixos,
    process::{run_command_checked, run_forked},
};
use nix::{
    sched::{unshare, CloneFlags},
    unistd::{chroot, getgid, getuid, pivot_root},
};
use std::{
    env::set_current_dir,
    ffi::OsString,
    fs::File,
    io::Write,
    os::unix::prelude::OsStringExt,
    path::{Path, PathBuf},
};
use tempdir::TempDir;

macro_rules! ok {
    ($($msg:expr),+) => {{
        print!("... OK: ");
        println!($($msg),+);
    }}
}

macro_rules! err {
    ($($msg:expr),+) => {{
        eprint!("... ERROR: ");
        eprintln!($($msg),+);

        return Err({})
    }};
}

pub struct App {
    bsd: BaseSystemDownloader,
}

impl App {
    fn new(base_system_downloader: BaseSystemDownloader) -> Self {
        Self {
            bsd: base_system_downloader,
        }
    }

    pub fn run(&self) -> Result<(), ()> {
        // Create a temporary root directory
        let root_dir = make_temp_dir()?;

        // Download the minimal chroot system tarball (and verify its integrity)
        let tarball_path = self.download_rootfs_tarball(root_dir.path())?;

        // Extract the rootfs tarball
        extract_rootfs_tarball(&tarball_path)?;

        // Run the build process in an isolated chroot environment
        let image_path = match run_forked(|| build_image(root_dir.path())) {
            Ok(r) => r?,
            Err(e) => {
                eprintln!("!!! FAILURE: {}", e);

                return Err({});
            }
        };

        // Pull the image out of temporary root directory
        pull_image(&image_path)?;

        Ok({})
    }

    fn download_rootfs_tarball(&self, root_path: &Path) -> Result<PathBuf, ()> {
        println!("Downloading base system tarball...");
        let base_system_tarball = root_path.join("alpine-minirootfs.tgz");
        match self.bsd.download(&base_system_tarball) {
            Ok(_) => {
                ok!("downloaded and verified the tarball");
                Ok(base_system_tarball)
            }
            Err(e) => err!("{}", e),
        }
    }
}

pub fn init_app() -> App {
    let client = Client::builder()
        .connect_timeout(None)
        .request_timeout(None)
        .build()
        .expect("Failed to initialize the HTTP client");

    let bsd = BaseSystemDownloader::new(client);

    App::new(bsd)
}

fn make_temp_dir() -> Result<TempDir, ()> {
    println!("Creating a temporary root directory...");
    match TempDir::new("nixoslxcgen") {
        Ok(tmp_dir) => {
            ok!("successfully created `{}`", tmp_dir.path().display());

            Ok(tmp_dir)
        }
        Err(e) => err!("{}", e),
    }
}

fn extract_rootfs_tarball(tarball_path: &Path) -> Result<(), ()> {
    println!("Extracting base system tarball...");
    match extract(&tarball_path) {
        Ok(_) => {
            ok!("`{}` was successfully extracted", tarball_path.display());
            Ok({})
        }
        Err(e) => err!("{}", e),
    }
}

fn build_image(root_path: &Path) -> Result<PathBuf, ()> {
    // Create a new namespace for the build process
    setup_namespace(root_path)?;

    // Run the build process in the new namespace
    let image_path = match run_forked(|| run_build_process()) {
        Ok(p) => p?,
        Err(e) => err!("{}", e),
    };

    // Return the resulting absolute path to the built image
    Ok(root_path.join(&image_path))
}

fn run_build_process() -> Result<PathBuf, ()> {
    // Fix resolv.conf
    fix_resolv_conf()?;

    // Add the Alpine edge repository
    add_repositories()?;

    // Install bash, xz, tar, nix via apk
    install_nix()?;

    // Add the nixpkg channel and update channels
    nix_update_channels()?;

    // Install `nixos-generate` through nix
    install_nixos_generate()?;

    // Generate an image
    let image_path = nixos_generate()?;

    Ok(image_path)
}

fn setup_namespace(root_path: &Path) -> Result<(), ()> {
    println!("Entering the private namespace...");

    let uid = getuid();
    let gid = getgid();

    if let Err(e) = unshare(
        CloneFlags::CLONE_NEWUSER
            | CloneFlags::CLONE_NEWNS
            | CloneFlags::CLONE_NEWPID
            | CloneFlags::CLONE_NEWUTS
            | CloneFlags::CLONE_NEWIPC,
    ) {
        err!("failed to enter the namespace: {}", e);
    }

    // create a directory for new root
    let new_root = root_path.join("new_root");
    if let Err(e) = std::fs::create_dir_all(&new_root) {
        err!(
            "failed to create a directory `{}` to hold a new root for pivoting: {}",
            new_root.display(),
            e
        );
    }

    // create a directory for old root
    let old_root = root_path.join("old_root");
    if let Err(e) = std::fs::create_dir_all(&old_root) {
        err!(
            "failed to create a directory `{}` to hold an old root for pivoting: {}",
            old_root.display(),
            e
        );
    }

    // mount a temporary root directory into a new root directory
    let _new_root_mount = match Mount::bind(&root_path, &new_root) {
        Ok(m) => m,
        Err(e) => err!("failed to bind-mount the temporary root: {}", e),
    };

    // mount /proc in the chroot
    let proc_path = new_root.join("proc");
    let _proc_mount = match Mount::bind("/proc", proc_path) {
        Ok(m) => m,
        Err(e) => err!("failed to mount `/proc` in the temporary root: {}", e),
    };

    // mount /sys in the chroot
    let sys_path = new_root.join("sys");
    let _sys_mount = match Mount::bind("/sys", sys_path) {
        Ok(m) => m,
        Err(e) => err!("failed to mount `/sys` in the temporary root: {}", e),
    };

    // mount /dev in the chroot
    let dev_path = new_root.join("dev");
    let _dev_mount = match Mount::bind("/dev", dev_path) {
        Ok(m) => m,
        Err(e) => err!("failed to mount `/dev` in the temporary root: {}", e),
    };

    // change directory to the new root
    if let Err(e) = set_current_dir(&new_root) {
        err!(
            "failed to change the current directory to `{}`: {}",
            new_root.display(),
            e
        );
    }

    // pivot root
    if let Err(e) = pivot_root(".", "old_root") {
        err!("failed to pivot root to {}: {}", new_root.display(), e);
    }

    // chroot into the new root
    if let Err(e) = chroot("/") {
        err!(
            "failed to change root directory to `{}`: {}",
            new_root.display(),
            e
        );
    }

    // change directory to reset context
    if let Err(e) = set_current_dir("/") {
        err!("failed to change the current directory to `/`: {}", e);
    }

    let setgroups_file_path = "/proc/self/setgroups";
    let mut setgroups_file = match File::create(setgroups_file_path) {
        Ok(f) => f,
        Err(e) => err!("unable to create the `{}` file: {}", setgroups_file_path, e),
    };
    if let Err(e) = setgroups_file.write_all(b"deny") {
        err!(
            "unable to write to the `{}` file: {}",
            setgroups_file_path,
            e
        )
    }

    // create UID map
    let uid_map_file_path = "/proc/self/uid_map";
    let mut uid_map_file = match File::create(uid_map_file_path) {
        Ok(f) => f,
        Err(e) => err!(
            "failed to create the UID map file `{}`: {}",
            uid_map_file_path,
            e
        ),
    };
    if let Err(e) = uid_map_file.write_all(format!("0 {} 1", uid).as_bytes()) {
        err!(
            "failed to write new uid mapping to `{}`: {}",
            uid_map_file_path,
            e
        );
    }

    // create GID map
    let gid_map_file_path = "/proc/self/gid_map";
    let mut gid_map_file = match File::create(gid_map_file_path) {
        Ok(f) => f,
        Err(e) => err!(
            "failed to create the UID map file `{}`: {}",
            gid_map_file_path,
            e
        ),
    };
    if let Err(e) = gid_map_file.write_all(format!("0 {} 1", gid).as_bytes()) {
        err!(
            "failed to write new uid mapping to `{}`: {}",
            gid_map_file_path,
            e
        );
    }

    ok!("configured and entered an isolate namespace");

    Ok({})
}

fn pull_image(tarball_path: &Path) -> Result<(), ()> {
    println!("Pulling the resulting image from the temporary root...");
    let image_name = match tarball_path.file_name() {
        Some(p) => PathBuf::from(p),
        _ => err!(
            "failed to get the image filename from: `{}`",
            tarball_path.display()
        ),
    };

    if let Err(e) = std::fs::copy(&tarball_path, &image_name) {
        err!(
            "failed to copy the resulting image from `{}` to `{}`: {}",
            tarball_path.display(),
            image_name.display(),
            e
        );
    }

    ok!(
        "successfully copied the `{}` image file",
        image_name.display()
    );

    Ok({})
}

fn fix_resolv_conf() -> Result<(), ()> {
    println!("Fix DNS resolution in the namespace...");
    let resolv_conf_path = "/etc/resolv.conf";

    if let Err(e) = std::fs::write(&resolv_conf_path, "nameserver 8.8.8.8") {
        err!("Unable to create `{}` file: {}", resolv_conf_path, e);
    }

    ok!("created a `{}` configuration file", resolv_conf_path);

    Ok({})
}

fn add_repositories() -> Result<(), ()> {
    println!("Adding Alpine edge repositories...");
    if let Err(e) = enable_edge_repositories() {
        err!("{}", e);
    }

    ok!("added main, community and testing edge repositories");

    Ok({})
}

fn install_nix() -> Result<(), ()> {
    println!("Installing the Nix package manager...");
    if let Err(e) = update_repositories() {
        err!("{}", e);
    }

    if let Err(e) = install_packages(&["nix"]) {
        err!("{}", e);
    }

    create_nix_conf()?;

    ok!("Nix package manager was successfully installed and configured");

    Ok({})
}

fn create_nix_conf() -> Result<(), ()> {
    if let Err(e) = std::fs::write("/etc/nix/nix.conf", "build-users-group =") {
        err!("failed to create the `nix.conf` configuration file: {}", e);
    }

    Ok({})
}

fn nix_update_channels() -> Result<(), ()> {
    println!("Configure and update Nix channels...");

    if let Err(e) = nixos::add_channel("https://nixos.org/channels/nixpkgs-unstable") {
        err!("{}", e)
    }

    if let Err(e) = nixos::update_channels() {
        err!("{}", e)
    }

    ok!("nixpkgs channel was added and channels were successfully updated");

    Ok({})
}

fn install_nixos_generate() -> Result<(), ()> {
    println!("Installing the `nixpkgs.nixos-generators` package through Nix...");

    if let Err(e) = nixos::install(vec!["nixpkgs.nixos-generators"]) {
        err!("{}", e);
    }

    ok!("successfully installed the package");

    Ok({})
}

fn nixos_generate() -> Result<PathBuf, ()> {
    println!("Generating an LXC container image...");

    let result = match run_command_checked("nixos-generate", &["-f", "lxc"]) {
        Ok(o) => o,
        Err(e) => err!("{}", e),
    };

    // trim the leading / and trailing newline `\n` from the output
    let output_path = result
        .stdout
        .into_iter()
        .skip(1)
        .filter(|c| *c as char != '\n')
        .collect();
    let image_path = PathBuf::from(OsString::from_vec(output_path));

    ok!("generated the image: {}", image_path.display());

    Ok(image_path)
}
