mod app;
mod config;
mod discord;
mod network;
mod persistence;

use std::sync::Arc;

use anyhow::Result;
use futures::StreamExt;
use serenity::prelude::*;
use tokio_stream::wrappers::ReceiverStream;
use tracing::info;
use tracing_subscriber::EnvFilter;

use config::Config;
use discord::{DiscordOutputConnector, IanvsDiscordEventHandler, MessageTracker};
use network::NetworkScanner;
use persistence::GitStore;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env().add_directive("ianvs_app::app=debug".parse()?),
        )
        .init();

    info!("Starting Network Monitor Bot");

    // Load configuration
    let config = Config::from_env()?;
    info!(?config.discord_channel_id, ?config.git_repo_path, "Loaded configuration");

    // Create network scanner
    let scanner = NetworkScanner::new(config.scan_interval_secs, config.scan_channel_capacity);
    let network_events = scanner.start();

    // Create git store for persistence
    let git_store = GitStore::new(
        config.git_repo_path,
        config.associations_file,
        config.git_remote_name,
        config.git_branch_name,
        config.git_ssh_key_path,
    );
    git_store.validate_repo()?;

    // Create Discord client
    let message_tracker = MessageTracker::new();
    let output_connector = DiscordOutputConnector::new(
        &config.discord_token,
        config.discord_channel_id,
        message_tracker.clone(),
    );
    let (ev_handler, association_requests_rx) =
        IanvsDiscordEventHandler::new_with_association_request_rx_channel(
            config.discord_channel_id,
            message_tracker,
            config.discord_channel_capacity,
        );
    let mut client = Client::builder(
        &config.discord_token,
        GatewayIntents::GUILD_MESSAGE_REACTIONS,
    )
    .event_handler_arc(Arc::new(ev_handler))
    .await?;
    let association_requests_stream = ReceiverStream::new(association_requests_rx).fuse();

    tokio::select! {
        result = client.start() => {
            if let Err(e) = result {
                anyhow::bail!("Discord client error: {}", e);
            } else {
                anyhow::bail!("Discord client exited without error");
            }
        }
        _ = app::app(network_events, association_requests_stream, git_store, output_connector) => {
            anyhow::bail!("App logic exited, shutting down");
        }
    }
}
