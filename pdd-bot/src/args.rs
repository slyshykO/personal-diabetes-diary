use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::path::Path;

const GIT_VERSION: &str = env!("GIT_VERSION");
const GIT_VERSION_STR: &str = concat!('\0', "Ver.:", env!("GIT_VERSION"), '\0');

pub(crate) fn get_version_str() -> &'static str {
    GIT_VERSION_STR
}

#[derive(Parser)]
#[clap(author, version = GIT_VERSION, about, long_about = None)]
#[clap(args_conflicts_with_subcommands = true)]
pub(crate) struct Args {
    /// Path to config file.
    #[clap(short, long, value_parser, default_value = "config.toml")]
    pub(crate) config: String,
    #[clap(subcommand)]
    pub(crate) action: Option<Action>,
}

#[derive(Subcommand)]
pub(crate) enum Action {
    /// Check format config.
    CheckConfig {
        /// Path to config file.
        #[clap(short, long, value_parser, default_value = "config.toml")]
        config: String,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct AppConfig {
    pub(crate) tg_bot_token: Option<String>,
    pub(crate) tg_chat_id: Option<Vec<String>>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            tg_bot_token: None,
            tg_chat_id: None,
        }
    }
}

impl AppConfig {
    pub fn from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let content = fs_err::read_to_string(path)?;
        let config = toml::from_str(&content)?;
        Ok(config)
    }
    pub fn from_str<S: AsRef<str>>(content: S) -> anyhow::Result<Self> {
        let s = content.as_ref();
        let config = toml::from_str(s)?;
        Ok(config)
    }
}
