use serenity::async_trait;
use serenity::client::{Context, EventHandler};
use serenity::model::channel::Reaction;
use serenity::model::gateway::Ready;
use serenity::model::id::ChannelId;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

use super::message_tracker::MessageTracker;
use crate::app::{DiscordUserId, UserAssociationRequest};

const ASSOCIATE_EMOJI: &str = "\u{1F44B}"; // 👋
const IGNORE_EMOJI: &str = "\u{274C}"; // ❌

pub struct IanvsDiscordEventHandler {
    channel_id: ChannelId,
    message_tracker: MessageTracker,
    association_tx: mpsc::Sender<UserAssociationRequest>,
}

impl IanvsDiscordEventHandler {
    pub fn new_with_association_request_rx_channel(
        channel_id: ChannelId,
        message_tracker: MessageTracker,
        channel_capacity: usize,
    ) -> (Self, mpsc::Receiver<UserAssociationRequest>) {
        let (tx, rx) = mpsc::channel(channel_capacity);

        let bot = Self {
            channel_id,
            message_tracker,
            association_tx: tx,
        };

        (bot, rx)
    }
}

#[async_trait]
impl EventHandler for IanvsDiscordEventHandler {
    async fn ready(&self, _ctx: Context, ready: Ready) {
        info!("Bot connected as {}", ready.user.name);
    }

    async fn reaction_add(&self, ctx: Context, reaction: Reaction) {
        // Only process reactions from our tracked channel
        if reaction.channel_id != self.channel_id {
            return;
        }

        let emoji_name = match &reaction.emoji {
            serenity::model::channel::ReactionType::Unicode(s) => s.as_str(),
            _ => return,
        };

        let message_id = reaction.message_id;
        let user_id = match reaction.user_id {
            Some(id) => id,
            None => return,
        };
        if user_id == ctx.cache.current_user().id {
            return;
        }

        // Look up the MAC address for this message
        let mac_address = match self.message_tracker.get(message_id).await {
            Some(mac) => mac,
            None => {
                debug!(?message_id, "Reaction on untracked message");
                return;
            }
        };

        let request = match emoji_name {
            ASSOCIATE_EMOJI => {
                info!(?mac_address, ?user_id, "User associating with device");
                UserAssociationRequest::AssociateRequest(mac_address, DiscordUserId(user_id.get()))
            }
            IGNORE_EMOJI => {
                info!(?mac_address, "User requesting to ignore device");
                UserAssociationRequest::NeverAskForAssociationInFutureRequest(mac_address)
            }
            _ => return,
        };

        if self.association_tx.send(request).await.is_err() {
            error!("Failed to send association request, channel closed");
        }

        // Remove tracking after association
        self.message_tracker.remove(message_id).await;
    }
}

pub fn reaction_emojis() -> (&'static str, &'static str) {
    (ASSOCIATE_EMOJI, IGNORE_EMOJI)
}
