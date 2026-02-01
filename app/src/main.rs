use crate::{
    app::{DiscordUserAssociation, MacAddress},
    config::Config,
};
use std::collections::HashMap;

mod app;
mod config;
mod discord;

struct NoPersistence {}
impl app::HasPersistingAssociationState for NoPersistence {
    async fn reconcile_and_persist_association_state(
        &self,
        current: Option<HashMap<MacAddress, DiscordUserAssociation>>,
    ) -> HashMap<MacAddress, DiscordUserAssociation> {
        current.unwrap_or_default()
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::from_env().expect("failed to load configuration from environment");

    let (mut discord_bot_client, output_connector, association_requests_stream) =
        discord::new_discord_client_with_io_adapters(
            &config.discord_token,
            &config.discord_channel_id,
            config.discord_channel_capacity,
        )
        .await;

    tokio::select! {
        result = discord_bot_client.start() => {
            if let Err(e) = result {
                anyhow::bail!("Discord client error: {}", e);
            } else {
                anyhow::bail!("Discord client exited without error");
            }
        }
        // TODO: connect real network events stream and persistence
        _ = app::app(futures::stream::pending(), association_requests_stream, NoPersistence {}, output_connector) => {
            anyhow::bail!("App logic exited, shutting down");
        }
    }
}
