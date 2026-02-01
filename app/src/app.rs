use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use futures::{StreamExt, stream::FusedStream};
use serde::{Deserialize, Serialize};
use tokio_stream::wrappers::IntervalStream;
use tracing::error;

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

impl DiscordUserAssociation {
    pub fn associated_user(&self) -> Option<DiscordUserId> {
        match self {
            DiscordUserAssociation::Associated(user_id) => Some(*user_id),
            DiscordUserAssociation::AskedNotToAssociateUser => None,
        }
    }
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

async fn handle_connected_addresses_change(
    previously_connected: &HashSet<MacAddress>,
    currently_connected: &HashSet<MacAddress>,
    output_connectors: &impl HasOutputConnectors,
    association: &HashMap<MacAddress, DiscordUserAssociation>,
) {
    fn map_both<T, R>((a, b): (T, T), f: impl Fn(T) -> R) -> (R, R) {
        (f(a), f(b))
    }

    fn hashset_diff_both_ways<T: Eq + std::hash::Hash + Copy>(
        left: &HashSet<T>,
        right: &HashSet<T>,
    ) -> (
        /* left - right */ HashSet<T>,
        /* right - left */ HashSet<T>,
    ) {
        (
            left.difference(right).copied().collect(),
            right.difference(left).copied().collect(),
        )
    }

    let (newly_connected_devices, disconnected_devices) =
        hashset_diff_both_ways(currently_connected, previously_connected);

    let (newly_connected_unknown_devices, disconnected_unknown_devices) = map_both(
        (&newly_connected_devices, &disconnected_devices),
        |connected| {
            connected
                .iter()
                .filter(|mac_addr|
                    // We don't want to log AskedNotToAssociateUser-MAC-addresses
                    // as devices of unknown users, so we test for absence of any association here.
                    !association.contains_key(mac_addr))
                .copied()
                .collect::<HashSet<_>>()
        },
    );

    for disconnected_unknown_device in disconnected_unknown_devices {
        output_connectors
            .report_unknown_user_disconnected(disconnected_unknown_device)
            .await;
    }

    for new_unknown_device in newly_connected_unknown_devices {
        output_connectors
            .report_unknown_user_connected(new_unknown_device)
            .await;
    }

    let (previously_connected_users, currently_connected_users) =
        map_both((previously_connected, currently_connected), |connected| {
            connected
                .iter()
                .filter_map(|mac_addr| {
                    association
                        .get(mac_addr)
                        .and_then(DiscordUserAssociation::associated_user)
                })
                .collect::<HashSet<_>>()
        });

    let (newly_connected_users, disconnected_users) =
        hashset_diff_both_ways(&currently_connected_users, &previously_connected_users);

    for new_user in newly_connected_users {
        let address_example = newly_connected_devices.iter().find(|mac_addr| {
            association.get(mac_addr) == Some(&DiscordUserAssociation::Associated(new_user))
        });

        if let Some(address_example) = address_example {
            output_connectors
                .report_user_connected(new_user, *address_example)
                .await;
        } else {
            error!(
                "Logic error: {}",
                anyhow::anyhow!("newly connected user has no associated connected device")
            );
        }
    }

    for disconnected_user in disconnected_users {
        let address_example = disconnected_devices.iter().find(|mac_addr| {
            association.get(mac_addr)
                == Some(&DiscordUserAssociation::Associated(disconnected_user))
        });

        if let Some(address_example) = address_example {
            output_connectors
                .report_user_disconnected(disconnected_user, *address_example)
                .await;
        } else {
            error!(
                "Logic error: {}",
                anyhow::anyhow!("disconnected user has no associated disconnected device")
            );
        }
    }
}

/// The application logic with abstract I/O connectors.
///
/// The input `Stream`s should run out of items only when it is impossible to
/// recover the stream and start producing events/requests again.
/// In the event of any of input `Stream`s' exhaustion, the `app` `Future` immediately returns,
/// signalling that the entire application should shut down for a restart.
pub async fn app(
    persistence_interval_secs: u16,
    initial_connected_addresses: HashSet<MacAddress>,
    // A stream of "set of MAC addresses currently connected to the network"
    mut connected_addresses_updates: impl FusedStream<Item = HashSet<MacAddress>> + Unpin,
    mut user_association_requests: impl FusedStream<Item = UserAssociationRequest> + Unpin,
    association_persistence: impl HasPersistingAssociationState,
    output_connectors: impl HasOutputConnectors,
) {
    let mut reconciliation_timer = Box::pin(
        IntervalStream::new(tokio::time::interval(Duration::from_secs(u64::from(
            persistence_interval_secs,
        ))))
        .fuse(),
    );

    let mut association_state = association_persistence
        .reconcile_and_persist_association_state(None)
        .await;
    let mut connected_addresses = initial_connected_addresses;

    loop {
        futures::select! {
            event = connected_addresses_updates.next() => {
                match event {
                    Some(latest_connected_addresses_set) => {
                        handle_connected_addresses_change(
                            &connected_addresses,
                            &latest_connected_addresses_set,
                            &output_connectors,
                            &association_state,
                        )
                        .await;
                        connected_addresses = latest_connected_addresses_set;
                    },
                    None => return,
                }
            },
            request = user_association_requests.next() => {
                use UserAssociationRequest::{AssociateRequest, NeverAskForAssociationInFutureRequest};
                use DiscordUserAssociation::{AskedNotToAssociateUser, Associated};
                match request {
                    Some(AssociateRequest(mac_address, discord_user_id)) => {
                        association_state.insert(mac_address, Associated(discord_user_id));
                    },
                    Some(NeverAskForAssociationInFutureRequest(mac_address)) => {
                        association_state.insert(mac_address, AskedNotToAssociateUser);
                    },
                    None => return,
                }
            },
            _ = reconciliation_timer.next() => {
                association_state = association_persistence
                    .reconcile_and_persist_association_state(Some(association_state))
                    .await;
            },
        };
    }
}
