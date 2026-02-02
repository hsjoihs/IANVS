use anyhow::{Context, Result};
use serde::Deserialize;
use serenity::model::id::ChannelId;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(deserialize_with = "deserialize_trimmed_nonempty")]
    pub discord_token: String,
    #[serde(deserialize_with = "deserialize_optional_channel_id")]
    pub discord_channel_id: Option<ChannelId>,
    #[serde(deserialize_with = "deserialize_optional_channel_id")]
    pub discord_user_notification_channel_id: Option<ChannelId>,
    #[serde(deserialize_with = "deserialize_optional_channel_id")]
    pub discord_mac_inquiry_channel_id: Option<ChannelId>,
    #[serde(default = "default_discord_channel_capacity")]
    pub discord_channel_capacity: usize,
    #[serde(default = "default_associations_file")]
    pub associations_file: String,
    #[serde(default = "default_scan_interval_secs")]
    pub scan_interval_secs: u16,
    #[serde(default = "default_persistence_interval_secs")]
    pub persistence_interval_secs: u16,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let config: Self = envy::from_env().context("failed to parse environment config")?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        // Ensure at least one channel is configured
        if self.discord_channel_id.is_none()
            && self.discord_user_notification_channel_id.is_none()
            && self.discord_mac_inquiry_channel_id.is_none()
        {
            anyhow::bail!(
                "At least one Discord channel must be configured. \
                 Set DISCORD_CHANNEL_ID (for both types), or set \
                 DISCORD_USER_NOTIFICATION_CHANNEL_ID and DISCORD_MAC_INQUIRY_CHANNEL_ID separately."
            );
        }
        Ok(())
    }

    /// Get the channel ID for user entry/exit notifications
    pub fn get_user_notification_channel_id(&self) -> ChannelId {
        self.discord_user_notification_channel_id
            .or(self.discord_channel_id)
            .expect("Channel configuration validated on load")
    }

    /// Get the channel ID for MAC address inquiry messages
    pub fn get_mac_inquiry_channel_id(&self) -> ChannelId {
        self.discord_mac_inquiry_channel_id
            .or(self.discord_channel_id)
            .expect("Channel configuration validated on load")
    }
}

fn default_associations_file() -> String {
    "associations.json".to_string()
}

fn default_discord_channel_capacity() -> usize {
    100
}

fn default_scan_interval_secs() -> u16 {
    600
}

fn default_persistence_interval_secs() -> u16 {
    60
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

fn deserialize_optional_channel_id<'de, D>(deserializer: D) -> Result<Option<ChannelId>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    match value {
        Some(s) if !s.trim().is_empty() => {
            let parsed: u64 = s
                .parse()
                .map_err(|_| serde::de::Error::custom("Channel ID must be a valid u64"))?;
            Ok(Some(ChannelId::new(parsed)))
        }
        _ => Ok(None),
    }
}
