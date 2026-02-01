#![deny(clippy::all)]
#![deny(clippy::pedantic)]

use crate::config::Config;

mod app;
mod config;
mod discord;
mod mac_address_scanning;
mod state_persistence;
mod stream_ext;

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

    let initial_connected_addresses = mac_address_scanning::scan_once(config.scan_interval_secs)
        .await
        .expect("initial network scanning");
    let scanning_stream = mac_address_scanning::periodic_scanning(config.scan_interval_secs);

    let association_persistence =
        state_persistence::JsonFileAssociationPersistence::new(&config.associations_file);

    tokio::select! {
        result = discord_bot_client.start() => {
            if let Err(e) = result {
                anyhow::bail!("Discord client error: {e}");
            }
            anyhow::bail!("Discord client exited without error");
        }
        () = app::app(
            initial_connected_addresses,
            scanning_stream,
            association_requests_stream,
            association_persistence,
            output_connector,
        ) => {
            anyhow::bail!("App logic exited, shutting down");
        }
    }
}
