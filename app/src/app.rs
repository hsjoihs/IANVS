use std::{collections::HashMap, time::Duration};

use futures::{StreamExt, stream::FusedStream};
use tokio_stream::wrappers::IntervalStream;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, derive_more::From)]
struct MacAddress([u8; 6]);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, derive_more::From)]
struct DiscordUserId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, derive_more::From)]
struct DiscordMessageId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, derive_more::From)]
struct UserConnectedEvent(MacAddress);

enum UserAssociationRequest {
    AssociateRequest(MacAddress, DiscordUserId),
    NeverAskForAssociationInFutureRequest(MacAddress),
}

enum DiscordUserAssociation {
    Associated(DiscordUserId),
    /// A user has requested to never be asked for association again.
    /// In this case we should entirely mute the events from this MAC address.
    AskedNotToAssociateUser,
}

trait HasPersistingAssociationState {
    async fn reconcile_and_persist_association_state(
        &self,
        current: Option<HashMap<MacAddress, DiscordUserAssociation>>,
    ) -> HashMap<MacAddress, DiscordUserAssociation>;
}

trait HasOutputConnectors {
    async fn report_user_connected(&self, user: DiscordUserId, address: MacAddress);

    async fn report_unknown_user_connected(&self, address: MacAddress);
}

/// The application logic with abstract I/O connectors.
///
/// The input `Stream`s should run out of items only when it is impossible to
/// recover the stream and start producing events/requests again.
/// In the event of any of input `Stream`s' exhaustion, the `app` `Future` immediately returns,
/// signalling that the entire application should shut down for a restart.
async fn app(
    user_connected_events: impl FusedStream<Item = UserConnectedEvent>,
    user_association_requests: impl FusedStream<Item = UserAssociationRequest>,
    association_persistence: impl HasPersistingAssociationState,
    output_connectors: impl HasOutputConnectors,
) {
    #[derive(derive_more::From)]
    enum InputEvent {
        UserConnectedEvent(UserConnectedEvent),
        UserAssociationRequest(UserAssociationRequest),
        PeriodicReconciliationTimer,
    }

    let user_connected_events = user_connected_events.fuse();
    futures::pin_mut!(user_connected_events);

    let user_association_requests = user_association_requests.fuse();
    futures::pin_mut!(user_association_requests);

    let reconciliation_timer =
        IntervalStream::new(tokio::time::interval(Duration::from_secs(120))).fuse();
    futures::pin_mut!(reconciliation_timer);

    let mut association_state = association_persistence
        .reconcile_and_persist_association_state(None)
        .await;

    loop {
        use DiscordUserAssociation::*;

        let next_event: InputEvent = futures::select! {
            event = user_connected_events.next() => {
                match event {
                    Some(event) => event.into(),
                    None => return,
                }
            },
            request = user_association_requests.next() => {
                match request {
                    Some(request) => request.into(),
                    None => return,
                }
            },
            _ = reconciliation_timer.next() => InputEvent::PeriodicReconciliationTimer,
        };

        match next_event {
            InputEvent::UserConnectedEvent(UserConnectedEvent(mac_addr)) => {
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
                };
            }
            InputEvent::UserAssociationRequest(request) => match request {
                UserAssociationRequest::AssociateRequest(mac_address, discord_user_id) => {
                    association_state.insert(mac_address, Associated(discord_user_id));
                }
                UserAssociationRequest::NeverAskForAssociationInFutureRequest(mac_address) => {
                    association_state.insert(mac_address, AskedNotToAssociateUser);
                }
            },
            InputEvent::PeriodicReconciliationTimer => {
                association_state = association_persistence
                    .reconcile_and_persist_association_state(Some(association_state))
                    .await;
            }
        }
    }
}
