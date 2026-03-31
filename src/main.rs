use clap::Parser;
use jackin::cli::Cli;

fn main() {
    let cli = Cli::parse();
    if let Err(error) = jackin::run(cli) {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}
