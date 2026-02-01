use anyhow::{Context, Result};
use serde::Deserialize;
use serenity::model::id::ChannelId;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(deserialize_with = "deserialize_trimmed_nonempty")]
    pub discord_token: String,
    #[serde(deserialize_with = "deserialize_channel_id")]
    pub discord_channel_id: ChannelId,
    #[serde(default = "default_discord_channel_capacity")]
    pub discord_channel_capacity: usize,
    #[serde(default = "default_associations_file")]
    pub associations_file: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(envy::from_env().context("failed to parse environment config")?)
    }
}

fn default_associations_file() -> String {
    "associations.json".to_string()
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
