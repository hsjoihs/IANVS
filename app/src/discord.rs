use futures::StreamExt;
use futures::stream::FusedStream;
use serenity::all::{ButtonStyle, CacheHttp, GatewayIntents, Interaction};
use serenity::async_trait;
use serenity::builder::{
    CreateAllowedMentions, CreateButton, CreateInteractionResponse,
    CreateInteractionResponseMessage, CreateMessage,
};
use serenity::client::{Context, EventHandler};
use serenity::model::gateway::Ready;
use serenity::model::id::{ChannelId, UserId};
use serenity::model::mention::Mentionable;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, error, info};

use crate::app::{DiscordUserId, MacAddress, UserAssociationRequest};

const ASSOCIATE_BUTTON_PREFIX: &str = "ianvs:associate:";
const NEVER_ASK_BUTTON_PREFIX: &str = "ianvs:never_ask:";

#[derive(Debug, Clone, Copy)]
enum AssociationButtonAction {
    Associate,
    NeverAsk,
}

fn parse_association_button_action(
    custom_id: &str,
) -> Option<(AssociationButtonAction, MacAddress)> {
    if let Some(suffix) = custom_id.strip_prefix(ASSOCIATE_BUTTON_PREFIX) {
        return MacAddress::parse(suffix).map(|mac| (AssociationButtonAction::Associate, mac));
    }
    if let Some(suffix) = custom_id.strip_prefix(NEVER_ASK_BUTTON_PREFIX) {
        return MacAddress::parse(suffix).map(|mac| (AssociationButtonAction::NeverAsk, mac));
    }
    None
}

fn associate_button_id(address: MacAddress) -> String {
    format!("{ASSOCIATE_BUTTON_PREFIX}{address}")
}

fn never_ask_button_id(address: MacAddress) -> String {
    format!("{NEVER_ASK_BUTTON_PREFIX}{address}")
}

struct IanvsDiscordBotEventHandler {
    /// A channel through which user requests incoming from Discord are tunneled
    association_tx: mpsc::Sender<UserAssociationRequest>,
}

impl IanvsDiscordBotEventHandler {
    pub fn new_with_association_request_rx_channel(
        channel_capacity: usize,
    ) -> (Self, mpsc::Receiver<UserAssociationRequest>) {
        let (tx, rx) = mpsc::channel(channel_capacity);

        let bot = Self { association_tx: tx };

        (bot, rx)
    }
}

#[async_trait]
impl EventHandler for IanvsDiscordBotEventHandler {
    async fn ready(&self, _ctx: Context, ready: Ready) {
        info!("Bot connected as {}", ready.user.name);
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        let Some(component) = interaction.message_component() else {
            return;
        };

        if component.message.author.id != ctx.cache.current_user().id {
            return;
        }

        let Some((action, mac_address)) =
            parse_association_button_action(component.data.custom_id.as_str())
        else {
            debug!(
                "Ignoring unknown component interaction. {{custom_id: {}, message_id: {}}}",
                component.data.custom_id, component.message.id
            );
            return;
        };

        let response_message = match action {
            AssociationButtonAction::Associate => {
                let discord_user_id = DiscordUserId(component.user.id.get());
                let request =
                    UserAssociationRequest::AssociateRequest(mac_address, discord_user_id);
                if let Err(err) = self.association_tx.send(request).await {
                    error!("Failed to enqueue user association request: {err}");
                    "Error: failed to link device."
                } else {
                    "Thanks! I've linked this device to you."
                }
            }
            AssociationButtonAction::NeverAsk => {
                let request =
                    UserAssociationRequest::NeverAskForAssociationInFutureRequest(mac_address);
                if let Err(err) = self.association_tx.send(request).await {
                    error!("Failed to enqueue user association request: {err}");
                    "Error: failed to mark device as ignored."
                } else {
                    "Got it. I won't ask about, nor notify connections from this device again."
                }
            }
        };

        let response = CreateInteractionResponse::Message(
            CreateInteractionResponseMessage::new()
                .content(response_message)
                .ephemeral(true),
        );
        if let Err(err) = component.create_response(&ctx.http, response).await {
            error!("Failed to respond to component interaction: {err}");
        }
    }
}

struct IanvsDiscordBotOutputAdapter<H: CacheHttp> {
    http: H,
    notification_destination_channel: ChannelId,
}

impl<H: CacheHttp> IanvsDiscordBotOutputAdapter<H> {
    async fn send_unknown_device_message(&self, content: &str, address: MacAddress) {
        let builder = CreateMessage::new()
            .content(content)
            .button(
                CreateButton::new(associate_button_id(address))
                    .label("Claim this device")
                    .style(ButtonStyle::Success),
            )
            .button(
                CreateButton::new(never_ask_button_id(address))
                    .label("Never ask again")
                    .style(ButtonStyle::Danger),
            );

        if let Err(err) = self
            .notification_destination_channel
            .send_message(&self.http, builder)
            .await
        {
            error!("Failed to send unknown-device message: {err}");
        }
    }
}

impl<H: CacheHttp> crate::app::HasOutputConnectors for IanvsDiscordBotOutputAdapter<H> {
    async fn report_user_connected(&self, user: DiscordUserId, address: crate::app::MacAddress) {
        info!(
            "Known user connected: user_id={} mac_address={}",
            user.0, address
        );
        let user_id = UserId::new(user.0);
        let builder = CreateMessage::new()
            .content(format!("{} connected to the network.", user_id.mention()))
            .allowed_mentions(CreateAllowedMentions::new().users(vec![user_id]));

        if let Err(err) = self
            .notification_destination_channel
            .send_message(&self.http, builder)
            .await
        {
            error!("Failed to send user-connected message: {err}");
        }
    }

    async fn report_unknown_user_connected(&self, address: crate::app::MacAddress) {
        info!("Unknown user connected: mac_address={}", address);
        self.send_unknown_device_message(
            "An unrecognized device connected. If this is you, click to claim it.",
            address,
        )
        .await;
    }

    async fn report_user_disconnected(&self, user: DiscordUserId, address: crate::app::MacAddress) {
        info!(
            "Known user disconnected: user_id={} mac_address={}",
            user.0, address
        );
        let user_id = UserId::new(user.0);
        let builder = CreateMessage::new()
            .content(format!("{} left the network.", user_id.mention()))
            .allowed_mentions(CreateAllowedMentions::new().users(vec![user_id]));

        if let Err(err) = self
            .notification_destination_channel
            .send_message(&self.http, builder)
            .await
        {
            error!("Failed to send user-disconnected message: {err}");
        }
    }

    async fn report_unknown_user_disconnected(&self, address: crate::app::MacAddress) {
        info!("Unknown user disconnected: mac_address={}", address);
        self.send_unknown_device_message(
            "An unrecognized device disconnected. If this is you, click to claim it.",
            address,
        )
        .await;
    }
}

pub async fn new_discord_client_with_io_adapters(
    bot_token: &str,
    communication_channel_id: &ChannelId,
    association_request_channel_capacity: usize,
) -> (
    serenity::Client,
    impl crate::app::HasOutputConnectors,
    impl FusedStream<Item = UserAssociationRequest> + Unpin,
) {
    let (handler, rx_channel) =
        IanvsDiscordBotEventHandler::new_with_association_request_rx_channel(
            association_request_channel_capacity,
        );

    let client = serenity::Client::builder(
        bot_token,
        // We wouldn't need any intent because we listen to users through button interactions
        GatewayIntents::empty(),
    )
    .event_handler(handler)
    .await
    .expect("Failed to create Discord client");

    let output_adapter = IanvsDiscordBotOutputAdapter {
        http: client.http.clone(),
        notification_destination_channel: *communication_channel_id,
    };

    (
        client,
        output_adapter,
        ReceiverStream::new(rx_channel).fuse(),
    )
}
