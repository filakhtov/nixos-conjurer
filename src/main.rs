mod alpine;
mod app;
mod archive;
mod builder;
mod config;
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
    let args: Vec<String> = std::env::args().collect();

    let app = match crate::app::init_app(&args) {
        Ok(app) => app,
        Err(e) => match e.code() {
            crate::app::ErrorCode::CommandLineParserError => usage(&args),
            _ => {
                eprintln!("{}", e);

                return Err(e.code() as i32);
            }
        },
    };

    if let Err(e) = app.run() {
        return Err(e.code() as i32);
    }

    Ok({})
}

fn usage(args: &Vec<String>) -> ! {
    let bin_path = std::path::PathBuf::from(&args[0]);
    let bin_name = match bin_path.file_name() {
        Some(name) => match name.to_str() {
            Some(name) => name,
            None => "nixos-conjurer",
        },
        None => "nixos-conjurer",
    };

    eprintln!("Usage: {} <configuration-path>", bin_name);

    std::process::exit(1);
}
