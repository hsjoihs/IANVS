use std::path::PathBuf;

use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serenity::model::id::ChannelId;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(deserialize_with = "deserialize_trimmed_nonempty")]
    pub discord_token: String,
    #[serde(deserialize_with = "deserialize_channel_id")]
    pub discord_channel_id: ChannelId,
    pub git_repo_path: PathBuf,
    #[serde(default = "default_scan_interval_secs")]
    pub scan_interval_secs: u64,
    #[serde(default = "default_scan_channel_capacity")]
    pub scan_channel_capacity: usize,
    #[serde(default = "default_associations_file")]
    pub associations_file: String,
    #[serde(default = "default_git_remote_name")]
    pub git_remote_name: String,
    #[serde(default = "default_git_branch_name")]
    pub git_branch_name: String,
    #[serde(default)]
    pub git_ssh_key_path: Option<PathBuf>,
    #[serde(default = "default_discord_channel_capacity")]
    pub discord_channel_capacity: usize,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let config: Config = envy::from_env().context("failed to parse environment config")?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            self.scan_interval_secs >= 5,
            "SCAN_INTERVAL_SECS must be at least 5 seconds, got {}",
            self.scan_interval_secs
        );
        ensure!(
            self.scan_channel_capacity > 0,
            "SCAN_CHANNEL_CAPACITY must be at least 1"
        );
        ensure!(
            self.discord_channel_capacity > 0,
            "DISCORD_CHANNEL_CAPACITY must be at least 1"
        );
        ensure!(
            !self.discord_token.trim().is_empty(),
            "DISCORD_TOKEN must not be empty"
        );
        Ok(())
    }
}

fn default_scan_interval_secs() -> u64 {
    30
}

fn default_scan_channel_capacity() -> usize {
    100
}

fn default_associations_file() -> String {
    "associations.json".to_string()
}

fn default_git_remote_name() -> String {
    "origin".to_string()
}

fn default_git_branch_name() -> String {
    "main".to_string()
}

fn default_discord_channel_capacity() -> usize {
    100
}

fn deserialize_trimmed_nonempty<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        return Err(serde::de::Error::custom("value must not be empty"));
    }
    Ok(trimmed)
}

fn deserialize_channel_id<'de, D>(deserializer: D) -> Result<ChannelId, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    let parsed: u64 = value
        .parse()
        .map_err(|_| serde::de::Error::custom("DISCORD_CHANNEL_ID must be a valid u64"))?;
    Ok(ChannelId::new(parsed))
}
