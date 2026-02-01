use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use anyhow::Context;
use tokio::fs;
use tracing::{error, info};

use crate::app::{DiscordUserAssociation, HasPersistingAssociationState, MacAddress};

#[derive(Debug, Clone)]
pub struct JsonFileAssociationPersistence {
    path: PathBuf,
}

fn parsed_mac_addr_map_to_unparsed_map(
    map: &HashMap<MacAddress, DiscordUserAssociation>,
) -> HashMap<String, DiscordUserAssociation> {
    map.iter()
        .map(|(mac, association)| (mac.to_string(), *association))
        .collect()
}

fn unparsed_mac_addr_map_to_parsed_map(
    map: &HashMap<String, DiscordUserAssociation>,
) -> anyhow::Result<HashMap<MacAddress, DiscordUserAssociation>> {
    map.iter()
        .map(|(mac, association)| {
            MacAddress::parse(mac)
                .map(|mac| (mac, *association))
                .ok_or_else(|| anyhow::anyhow!("Invalid MAC address: {mac}"))
        })
        .collect()
}

impl JsonFileAssociationPersistence {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    async fn load_persisted(&self) -> HashMap<MacAddress, DiscordUserAssociation> {
        async fn inner(
            path: &PathBuf,
        ) -> anyhow::Result<HashMap<MacAddress, DiscordUserAssociation>> {
            let read_result = fs::read(&path).await;
            if let Err(ref err) = read_result
                && err.kind() != std::io::ErrorKind::NotFound
            {
                return Ok(HashMap::new());
            }

            let persisted_contents = read_result.context("reading associations file")?;
            let contents_parsed_to_json = serde_json::from_slice(&persisted_contents)
                .context("parsing associations file into JSON")?;

            let parsed_map = unparsed_mac_addr_map_to_parsed_map(&contents_parsed_to_json)
                .context("converting parsed JSON to MAC address map")?;

            Ok(parsed_map)
        }

        match inner(&self.path).await {
            Ok(map) => map,
            Err(err) => {
                error!(
                    "Failed to load persisted associations from {}: {err}",
                    self.path.display()
                );
                HashMap::new()
            }
        }
    }

    async fn persist(
        &self,
        map: &HashMap<MacAddress, DiscordUserAssociation>,
    ) -> anyhow::Result<()> {
        if let Some(parent) = Path::new(&self.path).parent()
            && let Err(err) = fs::create_dir_all(parent).await
        {
            anyhow::bail!(
                "Failed to create associations directory {}: {err}",
                parent.display()
            );
        }

        let json = match serde_json::to_vec_pretty(&parsed_mac_addr_map_to_unparsed_map(map)) {
            Ok(json) => json,
            Err(err) => {
                anyhow::bail!(
                    "Failed to serialize associations for {}: {err}",
                    self.path.display()
                );
            }
        };

        if let Err(err) = fs::write(&self.path, json).await {
            anyhow::bail!(
                "Failed to write associations file {}: {err}",
                self.path.display()
            );
        }

        Ok(())
    }
}

impl HasPersistingAssociationState for JsonFileAssociationPersistence {
    async fn reconcile_and_persist_association_state(
        &self,
        current: Option<HashMap<MacAddress, DiscordUserAssociation>>,
    ) -> HashMap<MacAddress, DiscordUserAssociation> {
        let persisted = self.load_persisted().await;
        let mut result = persisted.clone();

        let merged = match current {
            Some(current) => {
                for (mac, association) in current {
                    if let Some(existing) = persisted.get(&mac)
                        && existing != &association
                    {
                        info!(
                            "Overriding persisted association for {} (persisted: {:?}, current: {:?})",
                            mac, existing, association
                        );
                    }
                    result.insert(mac, association);
                }
                result
            }
            None => persisted,
        };

        if let Err(err) = self.persist(&merged).await {
            error!("Failed to persist merged association state: {err}");
        }

        merged
    }
}
