use anyhow::Context;
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
    DefaultConfig,
    /// Install as service (Linux only).
    Install,
    /// Uninstall service and installed files (Linux only).
    Uninstall,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct TgConfig {
    pub(crate) tg_bot_token: Option<String>,
    pub(crate) tg_chat_id: Option<Vec<String>>,
    pub(crate) data_dir: Option<String>,
    pub(crate) input_timezone: Option<String>,
    pub(crate) glucose_after_meal_reminder_minutes: Option<u64>,
    pub(crate) glucose_after_meal_reminder_count: Option<u32>,
    pub(crate) glucose_after_meal_reminder_interval_minutes: Option<u64>,
}

impl Default for TgConfig {
    fn default() -> Self {
        Self {
            tg_bot_token: Some(String::new()),
            tg_chat_id: Some(Vec::new()),
            data_dir: Some("/opt/alex/share/data".to_string()),
            input_timezone: Some("Europe/Kyiv".to_string()),
            glucose_after_meal_reminder_minutes: Some(150),
            glucose_after_meal_reminder_count: Some(3),
            glucose_after_meal_reminder_interval_minutes: Some(3),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub(crate) struct AppConfig {
    pub(crate) tg_config: TgConfig,
}

#[allow(dead_code)]
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

    pub fn check_compatibility(&self) -> anyhow::Result<Vec<String>> {
        let mut warnings = Vec::new();

        match self.tg_config.tg_bot_token.as_deref() {
            Some(token) if !token.trim().is_empty() => {}
            Some(_) => warnings.push("tg_bot_token is empty".to_string()),
            None => warnings.push("tg_bot_token is missing".to_string()),
        }

        match &self.tg_config.tg_chat_id {
            Some(chat_ids) if chat_ids.is_empty() => {
                warnings.push("tg_chat_id is empty".to_string());
            }
            Some(chat_ids) => {
                for chat_id in chat_ids {
                    chat_id
                        .parse::<i64>()
                        .with_context(|| format!("invalid tg_chat_id '{chat_id}'"))?;
                }
            }
            None => warnings.push("tg_chat_id is missing".to_string()),
        }

        if let Some(input_timezone) = &self.tg_config.input_timezone {
            input_timezone
                .parse::<chrono_tz::Tz>()
                .map(|_| ())
                .map_err(|_| anyhow::anyhow!("invalid input_timezone '{input_timezone}'"))?;
        }

        Ok(warnings)
    }
}
