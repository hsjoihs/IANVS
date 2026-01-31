use std::collections::HashSet;
use std::io::ErrorKind;
use std::time::Duration;

use anyhow::anyhow;
use futures::stream::{FusedStream, StreamExt};
use procfs::net::ARPFlags;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, error, warn};

use crate::app::{MacAddress, UserNetworkEvent};

pub struct NetworkScanner {
    scan_interval: Duration,
    channel_capacity: usize,
}

impl NetworkScanner {
    pub fn new(scan_interval_secs: u64, channel_capacity: usize) -> Self {
        Self {
            scan_interval: Duration::from_secs(scan_interval_secs),
            channel_capacity,
        }
    }

    pub fn start(self) -> impl FusedStream<Item = UserNetworkEvent> {
        let (tx, rx) = mpsc::channel(self.channel_capacity);

        tokio::spawn(async move {
            self.run_scanner(tx).await;
        });

        ReceiverStream::new(rx).fuse()
    }

    async fn run_scanner(self, tx: mpsc::Sender<UserNetworkEvent>) {
        let mut seen_macs: HashSet<MacAddress> = HashSet::new();
        let mut pending_initial_scan = false;
        match self.scan_arp_table() {
            Ok(macs) => seen_macs = macs,
            Err(e) => {
                warn!("Initial ARP scan failed: {}", e);
                pending_initial_scan = true;
            }
        }
        let mut interval = tokio::time::interval(self.scan_interval);

        // Consume the initial immediate tick so subsequent scans align to the interval.
        interval.tick().await;

        loop {
            interval.tick().await;

            match self.scan_arp_table() {
                Ok(current_macs) => {
                    if pending_initial_scan {
                        seen_macs = current_macs;
                        pending_initial_scan = false;
                        continue;
                    }
                    for mac in current_macs.difference(&seen_macs) {
                        debug!(?mac, "New device detected");
                        if tx.send(UserNetworkEvent::Connected(*mac)).await.is_err() {
                            error!("Failed to send connected event, channel closed");
                            return;
                        }
                    }

                    for mac in seen_macs.difference(&current_macs) {
                        debug!(?mac, "Device disconnected");
                        if tx.send(UserNetworkEvent::Disconnected(*mac)).await.is_err() {
                            error!("Failed to send disconnected event, channel closed");
                            return;
                        }
                    }
                    seen_macs = current_macs;
                }
                Err(e) => {
                    warn!("Failed to scan ARP table: {}", e);
                }
            }
        }
    }

    fn scan_arp_table(&self) -> anyhow::Result<HashSet<MacAddress>> {
        let arp = procfs::net::arp().map_err(|e| match e {
            procfs::ProcError::Io(ref io_err, _)
                if io_err.kind() == ErrorKind::PermissionDenied =>
            {
                anyhow!("Permission denied reading /proc/net/arp; run with sufficient privileges")
            }
            _ => anyhow!(e),
        })?;
        let mut macs = HashSet::new();

        for entry in arp {
            // Skip incomplete entries (COM flag not set)
            if !entry.flags.contains(ARPFlags::COM) {
                continue;
            }

            // Get the hardware address bytes directly
            if let Some(hw_bytes) = entry.hw_address {
                // Skip broadcast and zero addresses
                if hw_bytes != [0xff, 0xff, 0xff, 0xff, 0xff, 0xff]
                    && hw_bytes != [0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
                {
                    macs.insert(MacAddress(hw_bytes));
                }
            }
        }

        debug!(count = macs.len(), "Scanned ARP table");
        Ok(macs)
    }
}
