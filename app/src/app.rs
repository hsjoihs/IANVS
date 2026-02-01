use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use futures::{StreamExt, stream::FusedStream};
use serde::{Deserialize, Serialize};
use tokio_stream::wrappers::IntervalStream;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, derive_more::From)]
pub struct MacAddress(pub [u8; 6]);

impl MacAddress {
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 6 {
            return None;
        }
        let mut bytes = [0u8; 6];
        for (i, part) in parts.iter().enumerate() {
            // note: We would be accepting strings like 0:0:0:0:0:0 as MAC addresses
            //       but here we don't need to be so strict as to reject them
            bytes[i] = u8::from_str_radix(part, 16).ok()?;
        }
        Some(MacAddress(bytes))
    }
}

impl std::fmt::Display for MacAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5]
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, derive_more::From, Serialize, Deserialize)]
pub struct DiscordUserId(pub u64);

#[derive(Debug)]
pub enum UserAssociationRequest {
    AssociateRequest(MacAddress, DiscordUserId),
    NeverAskForAssociationInFutureRequest(MacAddress),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiscordUserAssociation {
    Associated(DiscordUserId),
    /// A user has requested to never be asked for association again.
    /// In this case we should entirely mute the events from this MAC address.
    AskedNotToAssociateUser,
}

pub trait HasPersistingAssociationState {
    async fn reconcile_and_persist_association_state(
        &self,
        current: Option<HashMap<MacAddress, DiscordUserAssociation>>,
    ) -> HashMap<MacAddress, DiscordUserAssociation>;
}

pub trait HasOutputConnectors {
    async fn report_user_connected(&self, user: DiscordUserId, address: MacAddress);

    async fn report_unknown_user_connected(&self, address: MacAddress);

    async fn report_user_disconnected(&self, user: DiscordUserId, address: MacAddress);

    async fn report_unknown_user_disconnected(&self, address: MacAddress);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum NetworkDeviceDiff {
    Connected(MacAddress),
    Disconnected(MacAddress),
}

impl NetworkDeviceDiff {
    pub fn set_from_diff(
        previously_connected: &HashSet<MacAddress>,
        currently_connected: &HashSet<MacAddress>,
    ) -> Vec<NetworkDeviceDiff> {
        let newly_connected = currently_connected
            .difference(previously_connected)
            .copied()
            .map(NetworkDeviceDiff::Connected);
        let disconnected = previously_connected
            .difference(currently_connected)
            .copied()
            .map(NetworkDeviceDiff::Disconnected);

        newly_connected.chain(disconnected).collect()
    }
}

#[derive(derive_more::From)]
enum InputEvent {
    DeviceDiffDetected(NetworkDeviceDiff),
    UserAssociationRequested(UserAssociationRequest),
    ReconciliationTimerInvoked,
}

async fn process_input_event_and_update_association_state(
    event: InputEvent,
    output_connectors: &impl HasOutputConnectors,
    association_persistence: &impl HasPersistingAssociationState,
    mut association_state: HashMap<MacAddress, DiscordUserAssociation>,
) -> HashMap<MacAddress, DiscordUserAssociation> {
    use DiscordUserAssociation::{AskedNotToAssociateUser, Associated};
    match event {
        InputEvent::DeviceDiffDetected(NetworkDeviceDiff::Connected(mac_addr)) => {
            match association_state.get(&mac_addr) {
                Some(Associated(discord_user)) => {
                    output_connectors
                        .report_user_connected(*discord_user, mac_addr)
                        .await;
                }
                Some(AskedNotToAssociateUser) => (),
                None => {
                    output_connectors
                        .report_unknown_user_connected(mac_addr)
                        .await;
                }
            }
        }
        InputEvent::DeviceDiffDetected(NetworkDeviceDiff::Disconnected(mac_addr)) => {
            match association_state.get(&mac_addr) {
                Some(Associated(discord_user)) => {
                    output_connectors
                        .report_user_disconnected(*discord_user, mac_addr)
                        .await;
                }
                Some(AskedNotToAssociateUser) => (),
                None => {
                    output_connectors
                        .report_unknown_user_disconnected(mac_addr)
                        .await;
                }
            }
        }
        InputEvent::UserAssociationRequested(request) => match request {
            UserAssociationRequest::AssociateRequest(mac_address, discord_user_id) => {
                association_state.insert(mac_address, Associated(discord_user_id));
            }
            UserAssociationRequest::NeverAskForAssociationInFutureRequest(mac_address) => {
                association_state.insert(mac_address, AskedNotToAssociateUser);
            }
        },
        InputEvent::ReconciliationTimerInvoked => {
            association_state = association_persistence
                .reconcile_and_persist_association_state(Some(association_state))
                .await;
        }
    }

    association_state
}

/// The application logic with abstract I/O connectors.
///
/// The input `Stream`s should run out of items only when it is impossible to
/// recover the stream and start producing events/requests again.
/// In the event of any of input `Stream`s' exhaustion, the `app` `Future` immediately returns,
/// signalling that the entire application should shut down for a restart.
pub async fn app(
    initial_connected_addresses: HashSet<MacAddress>,
    // A stream of "set of MAC addresses currently connected to the network"
    mut connected_addresses_updates: impl FusedStream<Item = HashSet<MacAddress>> + Unpin,
    mut user_association_requests: impl FusedStream<Item = UserAssociationRequest> + Unpin,
    association_persistence: impl HasPersistingAssociationState,
    output_connectors: impl HasOutputConnectors,
) {
    let mut reconciliation_timer =
        Box::pin(IntervalStream::new(tokio::time::interval(Duration::from_secs(120))).fuse());

    let mut association_state = association_persistence
        .reconcile_and_persist_association_state(None)
        .await;
    let mut connected_addresses = initial_connected_addresses;

    loop {
        let next_events_to_process: Vec<InputEvent> = futures::select! {
            event = connected_addresses_updates.next() => {
                match event {
                    Some(latest_connected_addresses_set) => {
                        let events: Vec<InputEvent> = NetworkDeviceDiff::set_from_diff(
                            &connected_addresses,
                            &latest_connected_addresses_set,
                        ).into_iter().map(Into::into).collect();
                        connected_addresses = latest_connected_addresses_set;
                        events
                    },
                    None => return,
                }
            },
            request = user_association_requests.next() => {
                match request {
                    Some(request) => vec![request.into()],
                    None => return,
                }
            },
            _ = reconciliation_timer.next() => vec![InputEvent::ReconciliationTimerInvoked],
        };

        for event in next_events_to_process {
            association_state = process_input_event_and_update_association_state(
                event,
                &output_connectors,
                &association_persistence,
                association_state,
            )
            .await;
        }
    }
}
