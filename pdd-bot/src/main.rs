mod args;

use clap::Parser;
use std::path::Path;
use std::process::ExitCode;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

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
        None => {
            if let Err(e) = run(args.config).await {
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

async fn run<P: AsRef<Path> + Send>(path: P) -> anyhow::Result<()> {
    init_tracing();
    tracing::info!(
        "{}, version: {}",
        env!("CARGO_PKG_NAME"),
        args::get_version_str()
    );
    let path = path.as_ref();
    let config = args::AppConfig::from_file(path)?;
    tracing::info!("Running with config: {}", path.display());
    Ok(())
}

fn init_tracing() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "pdd_bot=debug,teloxide=debug".into()),
        ))
        .with(
            tracing_subscriber::fmt::layer()
                .with_file(true)
                .with_line_number(true),
        )
        .init();
}
