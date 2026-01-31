use serenity::http::Http;
use serenity::model::channel::ReactionType;
use serenity::model::id::ChannelId;
use tracing::{error, info};

use super::bot::reaction_emojis;
use super::message_tracker::MessageTracker;
use crate::app::{DiscordUserId, HasOutputConnectors, MacAddress};

pub struct DiscordOutputConnector {
    http: Http,
    channel_id: ChannelId,
    message_tracker: MessageTracker,
}

impl DiscordOutputConnector {
    pub fn new(token: &str, channel_id: ChannelId, message_tracker: MessageTracker) -> Self {
        Self {
            http: Http::new(token),
            channel_id,
            message_tracker,
        }
    }
}

impl HasOutputConnectors for DiscordOutputConnector {
    async fn report_user_connected(&self, user: DiscordUserId, address: MacAddress) {
        let content = format!("<@{}> connected ({})", user.0, address);

        match self.channel_id.say(&self.http, &content).await {
            Ok(msg) => {
                info!(message_id = ?msg.id, "Sent user connection notification");
            }
            Err(e) => {
                error!("Failed to send message: {}", e);
            }
        }
    }

    async fn report_unknown_user_connected(&self, address: MacAddress) {
        let content = format!(
            "Unknown device connected: `{}`\nReact with {} to associate or {} to ignore",
            address,
            reaction_emojis().0,
            reaction_emojis().1
        );

        match self.channel_id.say(&self.http, &content).await {
            Ok(msg) => {
                info!(message_id = ?msg.id, "Sent unknown device notification");

                // Track this message for reaction handling
                self.message_tracker.track(msg.id, address).await;

                // Add reaction buttons
                let (associate, ignore) = reaction_emojis();
                let associate_result = msg
                    .react(&self.http, ReactionType::Unicode(associate.to_string()))
                    .await;
                if let Err(e) = &associate_result {
                    error!("Failed to add associate reaction: {}", e);
                }
                let ignore_result = msg
                    .react(&self.http, ReactionType::Unicode(ignore.to_string()))
                    .await;
                if let Err(e) = &ignore_result {
                    error!("Failed to add ignore reaction: {}", e);
                }

                if associate_result.is_err() || ignore_result.is_err() {
                    self.message_tracker.remove(msg.id).await;
                }
            }
            Err(e) => {
                error!("Failed to send message: {}", e);
            }
        }
    }

    async fn report_user_disconnected(&self, user: DiscordUserId, address: MacAddress) {
        let content = format!("<@{}> disconnected ({})", user.0, address);

        match self.channel_id.say(&self.http, &content).await {
            Ok(msg) => {
                info!(message_id = ?msg.id, "Sent user disconnection notification");
            }
            Err(e) => {
                error!("Failed to send message: {}", e);
            }
        }
    }

    async fn report_unknown_user_disconnected(&self, address: MacAddress) {
        let content = format!("Unknown device disconnected: `{}`", address);

        match self.channel_id.say(&self.http, &content).await {
            Ok(msg) => {
                info!(message_id = ?msg.id, "Sent unknown device disconnection notification");
            }
            Err(e) => {
                error!("Failed to send message: {}", e);
            }
        }
    }
}
