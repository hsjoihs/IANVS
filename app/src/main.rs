use std::collections::HashSet;

use crate::config::Config;

mod app;
mod config;
mod discord;
mod state_persistence;

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

    let association_persistence =
        state_persistence::JsonFileAssociationPersistence::new(&config.associations_file);

    tokio::select! {
        result = discord_bot_client.start() => {
            if let Err(e) = result {
                anyhow::bail!("Discord client error: {}", e);
            } else {
                anyhow::bail!("Discord client exited without error");
            }
        }
        // TODO: connect real network events stream
        _ = app::app(
            HashSet::new(),
            futures::stream::pending(),
            association_requests_stream,
            association_persistence,
            output_connector,
        ) => {
            anyhow::bail!("App logic exited, shutting down");
        }
    }
}
