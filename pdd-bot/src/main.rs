use clap::Parser;
use std::path::Path;
use std::process::ExitCode;

pub mod install;

mod args;
mod tgbot;
#[tokio::main]
async fn main() -> ExitCode {
    let args = args::Args::parse();
    match args.action {
        Some(args::Action::CheckConfig { config }) => match config_check(config).await {
            Ok(()) => {
                println!("config is ok");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("bad config: {e}");
                ExitCode::from(3)
            }
        },
        Some(args::Action::Install) => match install::install() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("failed to install: {e}");
                ExitCode::from(4)
            }
        },
        None => {
            if let Err(e) = tgbot::run(args.config).await {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
    }
}

async fn config_check<P: AsRef<Path> + Send>(_path: P) -> anyhow::Result<()> {
    Ok(())
}
