use crate::app::MacAddress;
use anyhow::Context;
use futures::stream::FusedStream;
use itertools::Itertools as _;
use std::collections::HashSet;
use std::time::Duration;
use tokio::process::Command;
use tracing::{error, warn};

pub async fn scan_once(scan_interval_secs: u16) -> anyhow::Result<HashSet<MacAddress>> {
    // Run `arp-scan` and read off its output.
    // We expect either
    //  - the bot process is running with an elevated privilege
    //  - arp-scan can be run without privilege (e.g. by running `sudo setcap cap_net_raw=ep "${which arp-scan}"` in advance)
    // and if neither of these conditions is met, the command should fail and we must bail out.
    let output = Command::new("arp-scan")
        .args([
            "--localnet",
            "--quiet",
            "--plain",
            "--ignoredups",
            "--retry=2",
            "--backoff=1.50",
            // >  This timeout is for the first packet sent to each host.
            // >  subsequent timeouts are multiplied by the backoff factor which is set with --backoff.
            // >             - Manual page arp-scan(1)
            //
            // We set timeout duration of arp requests to be about 0.3 times the scan interval.
            // What we intend by "scan interval" is the time between *beginnings* of consecutive scans.
            // Therefore, considering that the retry count and backoff factor are set to 2 and 1.5,
            // for 75%+ of times our scanning fiber will be waiting for ARP replies,
            // potentially fruitlessly waiting for nonexistent IP hosts (we have no way to know in advance
            // which IPs are actually occupied on the network).
            //
            // This design is justified by the fact that some devices, for instance
            // iPhones on power-saving mode (as reported by @sim1222),
            // buffer ARP requests and respond to them later in a batch
            // to save power in exchange for increased latency.
            &format!("--timeout={}", u32::from(scan_interval_secs) * 300),
        ])
        .output()
        .await
        .context("Failed to execute arp-scan")?;

    if !output.status.success() {
        anyhow::bail!(
            "arp-scan exited with status {}. stderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .filter_map(|line| {
            if let &[_, column_1] = line.split_whitespace().collect_vec().as_slice()
                && let Some(parsed) = MacAddress::parse(column_1)
            {
                return Some(parsed);
            }

            warn!("Ignoring line from arp-scan (unexpected format): {}", line);
            None
        })
        .collect())
}

pub fn periodic_scanning(
    scan_interval_secs: u16,
) -> impl FusedStream<Item = HashSet<MacAddress>> + Unpin {
    Box::pin(crate::stream_ext::repeat_task_until_empty(
        move || async move {
            let ((), result) = tokio::join!(
                tokio::time::sleep(Duration::from_secs(scan_interval_secs.into())),
                scan_once(scan_interval_secs)
            );

            match result {
                Ok(addresses) => Some(addresses),
                Err(err) => {
                    error!("Failed to scan network: {}", err);
                    None
                }
            }
        },
    ))
}
