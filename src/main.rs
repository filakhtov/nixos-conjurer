mod alpine;
mod app;
mod archive;
mod http;
mod mount;
mod nixos;
mod process;

fn main() {
    if let Err(code) = run_main() {
        std::process::exit(code);
    }
}

fn run_main() -> Result<(), i32> {
    if let Err(_) = crate::app::init_app().run() {
        return Err(1);
    }

    Ok({})
}
