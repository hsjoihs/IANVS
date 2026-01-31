use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serenity::model::id::MessageId;
use tokio::sync::RwLock;

use crate::app::MacAddress;

#[derive(Debug, Clone)]
pub struct MessageTracker {
    inner: Arc<RwLock<HashMap<MessageId, TrackedMessage>>>,
}

impl MessageTracker {
    const DEFAULT_TTL: Duration = Duration::from_secs(60 * 60);
    const CLEANUP_INTERVAL: Duration = Duration::from_secs(60);

    pub fn new() -> Self {
        let inner = Arc::new(RwLock::new(HashMap::<MessageId, TrackedMessage>::new()));
        let ttl = Self::DEFAULT_TTL;
        let cleanup_inner = inner.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Self::CLEANUP_INTERVAL);
            loop {
                interval.tick().await;
                let cutoff = Instant::now() - ttl;
                let mut map = cleanup_inner.write().await;
                map.retain(|_, entry| entry.inserted_at >= cutoff);
            }
        });

        Self { inner }
    }

    pub async fn track(&self, message_id: MessageId, mac_address: MacAddress) {
        let inserted_at = Instant::now();
        let mut map = self.inner.write().await;
        map.insert(
            message_id,
            TrackedMessage {
                mac_address,
                inserted_at,
            },
        );
    }

    pub async fn get(&self, message_id: MessageId) -> Option<MacAddress> {
        let map = self.inner.read().await;
        map.get(&message_id).map(|entry| entry.mac_address)
    }

    pub async fn remove(&self, message_id: MessageId) -> Option<MacAddress> {
        let mut map = self.inner.write().await;
        map.remove(&message_id).map(|entry| entry.mac_address)
    }
}

#[derive(Debug, Clone, Copy)]
struct TrackedMessage {
    mac_address: MacAddress,
    inserted_at: Instant,
}
