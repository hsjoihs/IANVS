use std::path::PathBuf;

use anyhow::{Context, Result, ensure};
use serenity::model::id::ChannelId;
use tracing::warn;

#[derive(Debug, Clone)]
pub struct Config {
    pub discord_token: String,
    pub discord_channel_id: ChannelId,
    pub git_repo_path: PathBuf,
    pub scan_interval_secs: u64,
    pub scan_channel_capacity: usize,
    pub associations_file: String,
    pub git_remote_name: String,
    pub git_branch_name: String,
    pub git_ssh_key_path: Option<PathBuf>,
    pub discord_channel_capacity: usize,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok();

        let discord_token = std::env::var("DISCORD_TOKEN").context("DISCORD_TOKEN must be set")?;
        let discord_token = discord_token.trim().to_string();
        ensure!(!discord_token.is_empty(), "DISCORD_TOKEN must not be empty");

        let discord_channel_id: u64 = std::env::var("DISCORD_CHANNEL_ID")
            .context("DISCORD_CHANNEL_ID must be set")?
            .parse()
            .context("DISCORD_CHANNEL_ID must be a valid u64")?;

        let git_repo_path = std::env::var("GIT_REPO_PATH")
            .context("GIT_REPO_PATH must be set")?
            .into();

        let scan_interval_secs = match std::env::var("SCAN_INTERVAL_SECS") {
            Ok(value) => {
                let parsed: u64 = value.parse().with_context(|| {
                    format!("SCAN_INTERVAL_SECS must be a valid u64, got {}", value)
                })?;
                ensure!(
                    parsed >= 5,
                    "SCAN_INTERVAL_SECS must be at least 5 seconds, got {}",
                    parsed
                );
                if parsed > 3600 {
                    warn!(
                        "SCAN_INTERVAL_SECS is very large ({}s); scanning may appear disabled",
                        parsed
                    );
                }
                parsed
            }
            Err(_) => 30,
        };

        let scan_channel_capacity = match std::env::var("SCAN_CHANNEL_CAPACITY") {
            Ok(value) => {
                let parsed: usize = value.parse().with_context(|| {
                    format!("SCAN_CHANNEL_CAPACITY must be a valid usize, got {}", value)
                })?;
                ensure!(parsed > 0, "SCAN_CHANNEL_CAPACITY must be at least 1");
                parsed
            }
            Err(_) => 100,
        };

        let associations_file =
            std::env::var("ASSOCIATIONS_FILE").unwrap_or_else(|_| "associations.json".to_string());

        let git_remote_name =
            std::env::var("GIT_REMOTE_NAME").unwrap_or_else(|_| "origin".to_string());

        let git_branch_name =
            std::env::var("GIT_BRANCH_NAME").unwrap_or_else(|_| "main".to_string());

        let git_ssh_key_path = std::env::var("GIT_SSH_KEY_PATH").ok().map(PathBuf::from);

        let discord_channel_capacity = match std::env::var("DISCORD_CHANNEL_CAPACITY") {
            Ok(value) => {
                let parsed: usize = value.parse().with_context(|| {
                    format!(
                        "DISCORD_CHANNEL_CAPACITY must be a valid usize, got {}",
                        value
                    )
                })?;
                ensure!(parsed > 0, "DISCORD_CHANNEL_CAPACITY must be at least 1");
                parsed
            }
            Err(_) => 100,
        };

        Ok(Config {
            discord_token,
            discord_channel_id: ChannelId::new(discord_channel_id),
            git_repo_path,
            scan_interval_secs,
            scan_channel_capacity,
            associations_file,
            git_remote_name,
            git_branch_name,
            git_ssh_key_path,
            discord_channel_capacity,
        })
    }
}
