mod args;

use clap::Parser;
use std::path::Path;
use std::process::ExitCode;
use teloxide::prelude::*;
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
    let tg_bot_token = config
        .tg_bot_token
        .ok_or_else(|| anyhow::anyhow!("tg_bot_token is required in config"))?;
    let tg_chat_id = config
        .tg_chat_id
        .ok_or_else(|| anyhow::anyhow!("tg_chat_id is required in config"))?;
    let bot = Bot::new(tg_bot_token);
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
