use clap::Parser;
use std::path::Path;
use std::process::ExitCode;
use tokio::signal;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod install;

mod args;
mod data;
mod http;
mod reports;
mod state;
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
        Some(args::Action::DefaultConfig) => {
            let default = args::AppConfig::default();
            let toml =
                toml::to_string_pretty(&default).expect("Failed to serialize default config");
            println!("{toml}");
            ExitCode::SUCCESS
        }
        Some(args::Action::Install) => match install::install() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("failed to install: {e}");
                ExitCode::from(4)
            }
        },
        Some(args::Action::Uninstall) => match install::uninstall() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("failed to uninstall: {e}");
                ExitCode::from(5)
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

async fn run<P: AsRef<Path> + Send>(path: P) -> anyhow::Result<()> {
    init_tracing();
    tracing::info!(
        "{}, version: {}",
        env!("CARGO_PKG_NAME"),
        args::get_version_str().trim()
    );
    let path = path.as_ref();
    let config = args::AppConfig::from_file(path)?;
    let app_state = state::AppState::default();
    let shutdown_handlers = vec![
        tgbot::run(config.tg_config, app_state.clone()).await?,
        http::run(config.html_config, app_state.clone()).await?,
    ];
    let shutdown_result = wait_for_shutdown_signal(&app_state).await;
    for handler in shutdown_handlers.into_iter().flatten() {
        if handler.send(()).is_err() {
            tracing::error!("failed to send shutdown signal to handler");
        } else {
            tracing::info!("shutdown signal sent to handler");
        }
    }
    shutdown_result
}

async fn config_check<P: AsRef<Path> + Send>(path: P) -> anyhow::Result<()> {
    let config = args::AppConfig::from_file(path)?;
    for warning in config.check_compatibility()? {
        eprintln!("warning: {warning}");
    }
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

#[allow(clippy::redundant_pub_crate)]
async fn wait_for_shutdown_signal(app_state: &state::AppState) -> anyhow::Result<()> {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install ctrl+c handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            tracing::info!("ctrl+c received, shutting down");
            Ok(())
        }
        _ = terminate => {
            tracing::info!("terminate received, shutting down");
            Ok(())
        }
        stopped = app_state.wait_for_runnable_stop() => {
            match stopped.status {
                state::RunnableStatus::Running => unreachable!(),
                state::RunnableStatus::Stopped => {
                    anyhow::bail!("{} stopped unexpectedly", stopped.name)
                }
                state::RunnableStatus::Failed(error) => {
                    anyhow::bail!("{} failed: {error}", stopped.name)
                }
            }
        }
    }
}
